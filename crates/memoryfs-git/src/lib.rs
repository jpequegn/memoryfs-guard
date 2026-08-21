#![doc = "Native Git adapter for committed and staged memory vaults."]

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    path::{Component, Path, PathBuf},
};

use git2::{Index, ObjectType, Repository, TreeWalkMode, TreeWalkResult};
use memoryfs_core::{NoteType, ParsedNote, SourceLocation, parse_note};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitVaultError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("I/O operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("{path}: file is not UTF-8")]
    NonUtf8 { path: String },
    #[error("{path}: {message}")]
    Parse { path: String, message: String },
    #[error("repository has no working tree")]
    BareRepository,
    #[error("proposal path '{0}' is outside the configured vault")]
    UnsafeProposalPath(String),
    #[error("proposal base digest does not match the working file")]
    ProposalBaseMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitIdentity {
    pub oid: String,
    pub summary: String,
    pub author: String,
    pub timestamp_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultSnapshot {
    pub identity: CommitIdentity,
    pub notes: Vec<ParsedNote>,
    pub digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticCategory {
    Fact,
    Instruction,
    Permission,
    Link,
    Validity,
    Metadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub category: SemanticCategory,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub before_location: Option<SourceLocation>,
    pub after_location: Option<SourceLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticDiff {
    pub path: String,
    pub kind: ChangeKind,
    pub before_digest: Option<String>,
    pub after_digest: Option<String>,
    pub changes: Vec<FieldChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedPatch {
    pub base_commit: String,
    pub path: String,
    pub expected_before_digest: String,
    pub proposed_source: String,
    pub proposed_digest: String,
    pub semantic_diff: SemanticDiff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub id: String,
    pub path: String,
    pub note_type: NoteType,
    pub source_digest: String,
    pub link_targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultIndex {
    pub source_identity: String,
    pub entries: Vec<IndexEntry>,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationMode {
    PreCommit,
    Ci { revision: String },
}

pub struct GitVault {
    repository: Repository,
    vault_root: PathBuf,
}

impl GitVault {
    /// Opens a repository and configures the repository-relative vault root.
    ///
    /// # Errors
    ///
    /// Returns an error when the repository cannot be opened or the vault path
    /// attempts to escape the repository.
    pub fn open(
        repository: impl AsRef<Path>,
        vault_root: impl AsRef<Path>,
    ) -> Result<Self, GitVaultError> {
        let vault_root = validate_relative(vault_root.as_ref())?;
        Ok(Self {
            repository: Repository::open(repository)?,
            vault_root,
        })
    }

    /// Reads and parses every Markdown note from a commit tree.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown revision, unreadable blob, or invalid note.
    pub fn snapshot(&self, revision: &str) -> Result<VaultSnapshot, GitVaultError> {
        let object = self.repository.revparse_single(revision)?;
        let commit = object.peel_to_commit()?;
        let tree = commit.tree()?;
        let sources = self.sources_from_tree(&tree)?;
        let identity = CommitIdentity {
            oid: commit.id().to_string(),
            summary: commit.summary().unwrap_or_default().to_owned(),
            author: commit.author().name().unwrap_or_default().to_owned(),
            timestamp_seconds: commit.time().seconds(),
        };
        build_snapshot(identity, sources)
    }

    /// Reads the staged index for pre-commit validation, or a commit for CI.
    ///
    /// # Errors
    ///
    /// Returns an error when staged blobs or the selected revision cannot be read.
    pub fn validation_snapshot(
        &self,
        mode: &ValidationMode,
    ) -> Result<VaultSnapshot, GitVaultError> {
        match mode {
            ValidationMode::Ci { revision } => self.snapshot(revision),
            ValidationMode::PreCommit => {
                let index = self.repository.index()?;
                let sources = self.sources_from_index(&index)?;
                let tree_oid = index_tree_digest(&index);
                build_snapshot(
                    CommitIdentity {
                        oid: format!("INDEX:{tree_oid}"),
                        summary: "staged index".to_owned(),
                        author: String::new(),
                        timestamp_seconds: 0,
                    },
                    sources,
                )
            }
        }
    }

    /// Rebuilds a deterministic index from the selected commit.
    ///
    /// # Errors
    ///
    /// Returns an error when the commit snapshot cannot be loaded.
    pub fn rebuild_index(&self, revision: &str) -> Result<VaultIndex, GitVaultError> {
        let snapshot = self.snapshot(revision)?;
        Ok(index_snapshot(&snapshot))
    }

    /// Produces semantic changes between two committed vault states.
    ///
    /// # Errors
    ///
    /// Returns an error when either revision cannot be loaded.
    pub fn semantic_diff(
        &self,
        before_revision: &str,
        after_revision: &str,
    ) -> Result<Vec<SemanticDiff>, GitVaultError> {
        let before = self.snapshot(before_revision)?;
        let after = self.snapshot(after_revision)?;
        Ok(diff_snapshots(&before, &after))
    }

    /// Creates a detached proposal against a committed note without writing it.
    ///
    /// # Errors
    ///
    /// Returns an error when the base note cannot be found or either version fails parsing.
    pub fn propose(
        &self,
        base_revision: &str,
        path: &str,
        proposed_source: &str,
    ) -> Result<ProposedPatch, GitVaultError> {
        let snapshot = self.snapshot(base_revision)?;
        let before = snapshot
            .notes
            .iter()
            .find(|note| note.source_path == path)
            .ok_or_else(|| git2::Error::from_str("proposal base note was not found"))?;
        let after = parse_note(path, proposed_source).map_err(|error| GitVaultError::Parse {
            path: path.to_owned(),
            message: error.to_string(),
        })?;
        let semantic_diff = diff_note(Some(before), Some(&after), path);
        Ok(ProposedPatch {
            base_commit: snapshot.identity.oid,
            path: path.to_owned(),
            expected_before_digest: source_digest(&before.source),
            proposed_source: proposed_source.to_owned(),
            proposed_digest: source_digest(proposed_source),
            semantic_diff,
        })
    }

    /// Explicitly writes a previously generated proposal to the working tree.
    ///
    /// # Errors
    ///
    /// Returns an error when the path escapes the vault, the working tree is
    /// unavailable, or the file no longer matches the proposal base.
    pub fn apply_proposal(&self, proposal: &ProposedPatch) -> Result<PathBuf, GitVaultError> {
        let relative = validate_relative(Path::new(&proposal.path))?;
        if !relative.starts_with(&self.vault_root) {
            return Err(GitVaultError::UnsafeProposalPath(proposal.path.clone()));
        }
        let working_directory = self
            .repository
            .workdir()
            .ok_or(GitVaultError::BareRepository)?;
        let target = working_directory.join(relative);
        let existing = fs::read_to_string(&target)?;
        if source_digest(&existing) != proposal.expected_before_digest {
            return Err(GitVaultError::ProposalBaseMismatch);
        }
        fs::write(&target, &proposal.proposed_source)?;
        Ok(target)
    }

    fn sources_from_tree(
        &self,
        tree: &git2::Tree<'_>,
    ) -> Result<Vec<(String, String)>, GitVaultError> {
        let mut sources = Vec::new();
        tree.walk(TreeWalkMode::PreOrder, |root, entry| {
            let Some(name) = entry.name() else {
                return TreeWalkResult::Ok;
            };
            let path = format!("{root}{name}");
            if entry.kind() == Some(ObjectType::Blob)
                && is_markdown(Path::new(&path))
                && Path::new(&path).starts_with(&self.vault_root)
            {
                sources.push((path, entry.id()));
            }
            TreeWalkResult::Ok
        })?;
        sources.sort_by(|left, right| left.0.cmp(&right.0));
        sources
            .into_iter()
            .map(|(path, oid)| {
                let blob = self.repository.find_blob(oid)?;
                let source = std::str::from_utf8(blob.content())
                    .map_err(|_| GitVaultError::NonUtf8 { path: path.clone() })?;
                Ok((path, source.to_owned()))
            })
            .collect()
    }

    fn sources_from_index(&self, index: &Index) -> Result<Vec<(String, String)>, GitVaultError> {
        let mut sources = Vec::new();
        for entry in index.iter().filter(|entry| index_stage(entry.flags) == 0) {
            let path = std::str::from_utf8(&entry.path).map_err(|_| GitVaultError::NonUtf8 {
                path: "<index>".to_owned(),
            })?;
            if is_markdown(Path::new(path)) && Path::new(path).starts_with(&self.vault_root) {
                let blob = self.repository.find_blob(entry.id)?;
                let source =
                    std::str::from_utf8(blob.content()).map_err(|_| GitVaultError::NonUtf8 {
                        path: path.to_owned(),
                    })?;
                sources.push((path.to_owned(), source.to_owned()));
            }
        }
        sources.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(sources)
    }
}

fn validate_relative(path: &Path) -> Result<PathBuf, GitVaultError> {
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(GitVaultError::UnsafeProposalPath(
            path.display().to_string(),
        ));
    }
    Ok(path.to_owned())
}

fn build_snapshot(
    identity: CommitIdentity,
    sources: Vec<(String, String)>,
) -> Result<VaultSnapshot, GitVaultError> {
    let notes: Vec<_> = sources
        .into_iter()
        .map(|(path, source)| {
            parse_note(&path, &source).map_err(|error| GitVaultError::Parse {
                path,
                message: error.to_string(),
            })
        })
        .collect::<Result<_, _>>()?;
    let digest = digest_json(&notes);
    Ok(VaultSnapshot {
        identity,
        notes,
        digest,
    })
}

fn index_snapshot(snapshot: &VaultSnapshot) -> VaultIndex {
    let mut entries: Vec<_> = snapshot
        .notes
        .iter()
        .map(|note| {
            let mut targets: Vec<_> = note.links.iter().map(|link| link.target.clone()).collect();
            targets.sort();
            IndexEntry {
                id: note.note.id.clone(),
                path: note.source_path.clone(),
                note_type: note.note.note_type.clone(),
                source_digest: source_digest(&note.source),
                link_targets: targets,
            }
        })
        .collect();
    entries.sort_by(|left, right| (&left.id, &left.path).cmp(&(&right.id, &right.path)));
    let source_identity = snapshot.identity.oid.clone();
    let digest = digest_json(&(source_identity.as_str(), &entries));
    VaultIndex {
        source_identity,
        entries,
        digest,
    }
}

fn diff_snapshots(before: &VaultSnapshot, after: &VaultSnapshot) -> Vec<SemanticDiff> {
    let before_by_path: BTreeMap<_, _> = before
        .notes
        .iter()
        .map(|note| (note.source_path.as_str(), note))
        .collect();
    let after_by_path: BTreeMap<_, _> = after
        .notes
        .iter()
        .map(|note| (note.source_path.as_str(), note))
        .collect();
    let paths: BTreeSet<_> = before_by_path
        .keys()
        .chain(after_by_path.keys())
        .copied()
        .collect();
    paths
        .into_iter()
        .filter_map(|path| {
            let before_note = before_by_path.get(path).copied();
            let after_note = after_by_path.get(path).copied();
            (before_note != after_note).then(|| diff_note(before_note, after_note, path))
        })
        .collect()
}

fn diff_note(before: Option<&ParsedNote>, after: Option<&ParsedNote>, path: &str) -> SemanticDiff {
    let kind = match (before, after) {
        (None, Some(_)) => ChangeKind::Added,
        (Some(_), None) => ChangeKind::Removed,
        (Some(_), Some(_)) => ChangeKind::Modified,
        (None, None) => unreachable!("a diff always has one side"),
    };
    let mut changes = Vec::new();
    compare_field(
        &mut changes,
        "content",
        content_category(before.or(after).expect("one side exists")),
        before.map(|note| json!(note.body)),
        after.map(|note| json!(note.body)),
        before.map(note_location),
        after.map(note_location),
    );
    compare_field(
        &mut changes,
        "permissions",
        SemanticCategory::Permission,
        before.map(permission_value),
        after.map(permission_value),
        before.map(note_location),
        after.map(note_location),
    );
    compare_field(
        &mut changes,
        "links",
        SemanticCategory::Link,
        before.map(link_value),
        after.map(link_value),
        first_link_location(before),
        first_link_location(after),
    );
    compare_field(
        &mut changes,
        "validity",
        SemanticCategory::Validity,
        before.map(validity_value),
        after.map(validity_value),
        before.map(note_location),
        after.map(note_location),
    );
    compare_field(
        &mut changes,
        "metadata",
        SemanticCategory::Metadata,
        before.map(metadata_value),
        after.map(metadata_value),
        before.map(note_location),
        after.map(note_location),
    );
    SemanticDiff {
        path: path.to_owned(),
        kind,
        before_digest: before.map(|note| source_digest(&note.source)),
        after_digest: after.map(|note| source_digest(&note.source)),
        changes,
    }
}

fn compare_field(
    changes: &mut Vec<FieldChange>,
    field: &str,
    category: SemanticCategory,
    before: Option<Value>,
    after: Option<Value>,
    before_location: Option<SourceLocation>,
    after_location: Option<SourceLocation>,
) {
    if before != after {
        changes.push(FieldChange {
            field: field.to_owned(),
            category,
            before,
            after,
            before_location,
            after_location,
        });
    }
}

fn content_category(note: &ParsedNote) -> SemanticCategory {
    match note.note.note_type {
        NoteType::Instruction | NoteType::Policy | NoteType::Procedure => {
            SemanticCategory::Instruction
        }
        NoteType::Fact | NoteType::Observation => SemanticCategory::Fact,
        NoteType::Decision | NoteType::Preference => SemanticCategory::Metadata,
    }
}

fn permission_value(note: &ParsedNote) -> Value {
    json!({
        "scope": note.note.scope,
        "trust": note.note.trust,
        "sensitivity": note.note.sensitivity,
    })
}

fn link_value(note: &ParsedNote) -> Value {
    json!({
        "links": note.links.iter().map(|link| (&link.target, &link.kind)).collect::<Vec<_>>(),
        "attachments": note.attachments.iter().map(|item| &item.target).collect::<Vec<_>>(),
        "supersedes": note.note.supersedes,
        "conflicts_with": note.note.conflicts_with,
    })
}

fn validity_value(note: &ParsedNote) -> Value {
    json!({
        "validity": note.note.validity,
        "observed_at": note.note.provenance.observed_at,
        "retention_days": note.note.retention_days,
    })
}

fn metadata_value(note: &ParsedNote) -> Value {
    json!({
        "id": note.note.id,
        "title": note.note.title,
        "type": note.note.note_type,
        "provenance": note.note.provenance,
        "confidence": note.note.confidence,
        "extra": note.note.extra,
    })
}

fn note_location(note: &ParsedNote) -> SourceLocation {
    SourceLocation {
        file: note.source_path.clone(),
        line: 1,
        column: 1,
    }
}

fn first_link_location(note: Option<&ParsedNote>) -> Option<SourceLocation> {
    note.and_then(|note| {
        note.links
            .first()
            .map(|link| link.location.clone())
            .or_else(|| note.attachments.first().map(|item| item.location.clone()))
            .or_else(|| Some(note_location(note)))
    })
}

fn index_tree_digest(index: &Index) -> String {
    let entries: Vec<_> = index
        .iter()
        .filter(|entry| index_stage(entry.flags) == 0)
        .map(|entry| (entry.path, entry.id.to_string(), entry.mode))
        .collect();
    digest_json(&entries)
}

fn source_digest(source: &str) -> String {
    hex_digest(source.as_bytes())
}

const fn index_stage(flags: u16) -> u16 {
    (flags >> 12) & 0x3
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn digest_json(value: &impl Serialize) -> String {
    hex_digest(&serde_json::to_vec(value).expect("Git adapter contracts serialize"))
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a string succeeds");
    }
    encoded
}
