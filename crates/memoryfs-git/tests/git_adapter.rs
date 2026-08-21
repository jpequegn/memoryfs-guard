use std::{collections::BTreeSet, fs, path::Path};

use git2::{IndexAddOption, Oid, Repository, Signature};
use memoryfs_git::{ChangeKind, GitVault, SemanticCategory, ValidationMode};
use tempfile::TempDir;

fn note(id: &str, note_type: &str, body: &str, sensitivity: &str, valid_until: &str) -> String {
    format!(
        "---\n\
schema_version: 1\n\
id: {id}\n\
title: {id}\n\
type: {note_type}\n\
scope:\n  projects: [memoryfs]\n  agents: [compiler]\n  environments: [development]\n\
provenance:\n  source: test\n  author: tester\n  observed_at: \"2026-08-20T00:00:00Z\"\n\
trust: trusted\n\
sensitivity: {sensitivity}\n\
validity:\n  valid_until: \"{valid_until}\"\n\
---\n\n# {id}\n\n{body}\n"
    )
}

fn setup() -> (TempDir, Repository, Oid) {
    let directory = TempDir::new().expect("temporary directory");
    let repository = Repository::init(directory.path()).expect("repository initializes");
    fs::create_dir(directory.path().join("vault")).expect("vault directory is created");
    fs::write(
        directory.path().join("vault/fact.md"),
        note(
            "fact",
            "fact",
            "Initial fact.",
            "internal",
            "2027-01-01T00:00:00Z",
        ),
    )
    .expect("note is written");
    let first = commit_all(&repository, "initial vault", &[]);
    (directory, repository, first)
}

fn commit_all(repository: &Repository, message: &str, parents: &[Oid]) -> Oid {
    let mut index = repository.index().expect("index opens");
    index
        .add_all(["vault"], IndexAddOption::DEFAULT, None)
        .expect("files are staged");
    index.write().expect("index writes");
    let tree_id = index.write_tree().expect("tree writes");
    let tree = repository.find_tree(tree_id).expect("tree loads");
    let signature = Signature::now("Test", "test@example.com").expect("signature builds");
    let parent_commits: Vec<_> = parents
        .iter()
        .map(|oid| repository.find_commit(*oid).expect("parent loads"))
        .collect();
    let parent_refs: Vec<_> = parent_commits.iter().collect();
    repository
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .expect("commit succeeds")
}

#[test]
fn semantic_diff_covers_content_permissions_links_and_validity_with_locations() {
    let (directory, repository, first) = setup();
    fs::write(
        directory.path().join("vault/fact.md"),
        note(
            "fact",
            "fact",
            "Updated fact linking to [[instruction]].",
            "confidential",
            "2028-01-01T00:00:00Z",
        ),
    )
    .expect("fact is updated");
    fs::write(
        directory.path().join("vault/instruction.md"),
        note(
            "instruction",
            "instruction",
            "Perform the reviewed action.",
            "internal",
            "2027-01-01T00:00:00Z",
        ),
    )
    .expect("instruction is written");
    let second = commit_all(&repository, "update vault", &[first]);
    drop(repository);

    let vault = GitVault::open(directory.path(), "vault").expect("vault opens");
    let diffs = vault
        .semantic_diff(&first.to_string(), &second.to_string())
        .expect("diff succeeds");
    let categories: BTreeSet<_> = diffs
        .iter()
        .flat_map(|diff| diff.changes.iter().map(|change| change.category))
        .collect();

    assert!(categories.contains(&SemanticCategory::Fact));
    assert!(categories.contains(&SemanticCategory::Instruction));
    assert!(categories.contains(&SemanticCategory::Permission));
    assert!(categories.contains(&SemanticCategory::Link));
    assert!(categories.contains(&SemanticCategory::Validity));
    assert!(diffs.iter().all(|diff| {
        diff.before_digest
            .as_ref()
            .is_none_or(|digest| digest.len() == 64)
            && diff
                .after_digest
                .as_ref()
                .is_none_or(|digest| digest.len() == 64)
            && diff.changes.iter().all(|change| {
                change
                    .before_location
                    .as_ref()
                    .or(change.after_location.as_ref())
                    .is_some_and(|location| location.line > 0)
            })
    }));
    assert!(diffs.iter().any(|diff| diff.kind == ChangeKind::Added));
}

#[test]
fn proposal_stays_detached_until_explicit_apply_and_checks_its_base() {
    let (directory, repository, first) = setup();
    drop(repository);
    let vault = GitVault::open(directory.path(), "vault").expect("vault opens");
    let path = directory.path().join("vault/fact.md");
    let before = fs::read_to_string(&path).expect("working note reads");
    let proposed = before.replace("Initial fact.", "Proposed fact.");
    let proposal = vault
        .propose(&first.to_string(), "vault/fact.md", &proposed)
        .expect("proposal builds");

    assert_eq!(fs::read_to_string(&path).expect("note reads"), before);
    assert_ne!(proposal.expected_before_digest, proposal.proposed_digest);
    assert_eq!(proposal.semantic_diff.kind, ChangeKind::Modified);

    vault
        .apply_proposal(&proposal)
        .expect("proposal applies explicitly");
    assert_eq!(fs::read_to_string(&path).expect("note reads"), proposed);
    assert!(vault.apply_proposal(&proposal).is_err());
}

#[test]
fn commit_indexes_are_reproducible_and_rollback_restores_the_prior_index() {
    let (directory, repository, first) = setup();
    let vault = GitVault::open(directory.path(), "vault").expect("vault opens");
    let original = vault
        .rebuild_index(&first.to_string())
        .expect("index rebuilds");
    assert_eq!(
        original,
        vault
            .rebuild_index(&first.to_string())
            .expect("index rebuilds identically")
    );
    drop(vault);

    fs::write(
        directory.path().join("vault/fact.md"),
        note(
            "fact",
            "fact",
            "Changed after the first commit.",
            "internal",
            "2027-01-01T00:00:00Z",
        ),
    )
    .expect("note changes");
    let second = commit_all(&repository, "second", &[first]);
    drop(repository);
    let vault = GitVault::open(directory.path(), "vault").expect("vault reopens");
    let updated = vault
        .rebuild_index(&second.to_string())
        .expect("updated index rebuilds");
    let rolled_back = vault
        .rebuild_index(&first.to_string())
        .expect("rollback index rebuilds");

    assert_ne!(updated.digest, original.digest);
    assert_eq!(rolled_back, original);
}

#[test]
fn pre_commit_reads_staged_content_while_ci_reads_the_requested_commit() {
    let (directory, repository, first) = setup();
    let staged_source = note(
        "fact",
        "fact",
        "Staged but not committed.",
        "internal",
        "2027-01-01T00:00:00Z",
    );
    let relative = Path::new("vault/fact.md");
    fs::write(directory.path().join(relative), &staged_source).expect("note changes");
    let mut index = repository.index().expect("index opens");
    index.add_path(relative).expect("note stages");
    index.write().expect("index writes");
    drop(repository);

    let vault = GitVault::open(directory.path(), "vault").expect("vault opens");
    let staged = vault
        .validation_snapshot(&ValidationMode::PreCommit)
        .expect("staged snapshot loads");
    let committed = vault
        .validation_snapshot(&ValidationMode::Ci {
            revision: first.to_string(),
        })
        .expect("commit snapshot loads");

    assert!(staged.identity.oid.starts_with("INDEX:"));
    assert_eq!(
        staged.notes[0].body.trim(),
        "# fact\n\nStaged but not committed."
    );
    assert_eq!(committed.notes[0].body.trim(), "# fact\n\nInitial fact.");
}
