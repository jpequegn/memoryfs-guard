use std::{
    collections::{BTreeSet, HashMap},
    fmt::Write as _,
};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    LintContext, NoteType, ParsedNote, Sensitivity, Severity, SourceLocation, TrustState,
    lint_vault,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingPolicy {
    Metadata,
    Lexical,
    VectorFixture,
    BoundedLinks,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallerScope {
    pub project: String,
    pub agent: Option<String>,
    pub environment: Option<String>,
    pub max_sensitivity: Sensitivity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompileRequest {
    pub task: String,
    pub caller: CallerScope,
    #[serde(default)]
    pub allowed_note_types: BTreeSet<NoteType>,
    pub token_budget: usize,
    #[serde(default)]
    pub required_evidence: BTreeSet<String>,
    pub policy_version: String,
    pub ranking: RankingPolicy,
    #[serde(default = "default_stale_after_days")]
    pub stale_after_days: i64,
    #[serde(default = "default_link_depth")]
    pub max_link_depth: usize,
}

const fn default_stale_after_days() -> i64 {
    180
}

const fn default_link_depth() -> usize {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalStatus {
    Current,
    Stale,
    Expired,
    NotYetValid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStatus {
    Clear,
    Conflicted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityStatus {
    Authorized,
    Unauthorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateDecision {
    Included,
    Excluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateReceipt {
    pub note_id: String,
    pub decision: CandidateDecision,
    pub reason: String,
    pub score: i64,
    pub estimated_tokens: usize,
    pub temporal_status: TemporalStatus,
    pub conflict_status: ConflictStatus,
    pub authority_status: AuthorityStatus,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextExcerpt {
    pub note_id: String,
    pub title: Option<String>,
    pub text: String,
    pub provenance_source: String,
    pub estimated_tokens: usize,
    pub score: i64,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestReceipt {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextPack {
    pub schema_version: u32,
    pub task: String,
    pub policy_version: String,
    pub vault_digest: String,
    pub total_estimated_tokens: usize,
    pub excerpts: Vec<ContextExcerpt>,
    pub candidates: Vec<CandidateReceipt>,
    pub receipt: DigestReceipt,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompileError {
    #[error("required evidence '{note_id}' was not found")]
    RequiredEvidenceMissing { note_id: String },
    #[error("required evidence '{note_id}' was refused: {reason}")]
    RequiredEvidenceRefused { note_id: String, reason: String },
}

#[derive(Debug)]
struct Evaluated<'a> {
    note: &'a ParsedNote,
    score: i64,
    tokens: usize,
    temporal: TemporalStatus,
    conflict: ConflictStatus,
    authority: AuthorityStatus,
    rejection: Option<String>,
    required: bool,
}

/// Compiles an authorized, bounded, and fully explained context pack.
///
/// # Errors
///
/// Returns an error when required evidence is missing, unauthorized, stale,
/// contradictory, or cannot fit in the requested token budget.
#[allow(clippy::too_many_lines)]
pub fn compile_context(
    notes: &[ParsedNote],
    request: &CompileRequest,
    now: DateTime<Utc>,
) -> Result<ContextPack, CompileError> {
    let by_id: HashMap<_, _> = notes
        .iter()
        .map(|parsed| (parsed.note.id.as_str(), parsed))
        .collect();
    for required in &request.required_evidence {
        if !by_id.contains_key(required.as_str()) {
            return Err(CompileError::RequiredEvidenceMissing {
                note_id: required.clone(),
            });
        }
    }

    let task_terms = terms(&request.task);
    let inbound = inbound_link_counts(notes);
    let lint_receipt = lint_vault(notes, &LintContext::new(now));
    let lint_errors: HashMap<_, _> = lint_receipt
        .findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error && !finding.suppressed)
        .map(|finding| (finding.location.file.as_str(), finding.code.as_str()))
        .collect();
    let mut evaluated: Vec<_> = notes
        .iter()
        .map(|note| {
            evaluate(
                note,
                request,
                now,
                &task_terms,
                &by_id,
                &inbound,
                &lint_errors,
            )
        })
        .collect();
    evaluated.sort_by(|left, right| {
        right
            .required
            .cmp(&left.required)
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.note.note.id.cmp(&right.note.note.id))
            .then_with(|| left.note.source_path.cmp(&right.note.source_path))
    });

    for candidate in evaluated.iter().filter(|candidate| candidate.required) {
        if let Some(reason) = &candidate.rejection {
            return Err(CompileError::RequiredEvidenceRefused {
                note_id: candidate.note.note.id.clone(),
                reason: reason.clone(),
            });
        }
    }
    let required_tokens: usize = evaluated
        .iter()
        .filter(|candidate| candidate.required)
        .map(|candidate| candidate.tokens)
        .sum();
    if required_tokens > request.token_budget {
        let note_id = evaluated
            .iter()
            .find(|candidate| candidate.required)
            .map_or_else(String::new, |candidate| candidate.note.note.id.clone());
        return Err(CompileError::RequiredEvidenceRefused {
            note_id,
            reason: format!(
                "required evidence needs {required_tokens} tokens but the budget is {}",
                request.token_budget
            ),
        });
    }

    let mut used = 0;
    let mut excerpts = Vec::new();
    let mut candidates = Vec::with_capacity(evaluated.len());
    for candidate in evaluated {
        let location = source_location(candidate.note);
        let (decision, reason) = if let Some(reason) = candidate.rejection {
            (CandidateDecision::Excluded, reason)
        } else if used + candidate.tokens > request.token_budget {
            (
                CandidateDecision::Excluded,
                format!(
                    "excluded because {} tokens would exceed the {} token budget",
                    candidate.tokens, request.token_budget
                ),
            )
        } else {
            used += candidate.tokens;
            excerpts.push(ContextExcerpt {
                note_id: candidate.note.note.id.clone(),
                title: candidate.note.note.title.clone(),
                text: candidate.note.body.trim().to_owned(),
                provenance_source: candidate.note.note.provenance.source.clone(),
                estimated_tokens: candidate.tokens,
                score: candidate.score,
                location: location.clone(),
            });
            let required = if candidate.required { "required " } else { "" };
            (
                CandidateDecision::Included,
                format!(
                    "included as {required}authorized evidence with ranking score {}",
                    candidate.score
                ),
            )
        };
        candidates.push(CandidateReceipt {
            note_id: candidate.note.note.id.clone(),
            decision,
            reason,
            score: candidate.score,
            estimated_tokens: candidate.tokens,
            temporal_status: candidate.temporal,
            conflict_status: candidate.conflict,
            authority_status: candidate.authority,
            location,
        });
    }

    excerpts.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.note_id.cmp(&right.note_id))
    });
    candidates.sort_by(|left, right| left.note_id.cmp(&right.note_id));
    let vault_digest = digest_sorted_notes(notes);
    let mut pack = ContextPack {
        schema_version: 1,
        task: request.task.clone(),
        policy_version: request.policy_version.clone(),
        vault_digest,
        total_estimated_tokens: used,
        excerpts,
        candidates,
        receipt: DigestReceipt {
            algorithm: "sha256".to_owned(),
            key_id: "content-addressed-v1".to_owned(),
            value: String::new(),
        },
    };
    pack.receipt.value = digest_json(&pack);
    Ok(pack)
}

fn evaluate<'a>(
    parsed: &'a ParsedNote,
    request: &CompileRequest,
    now: DateTime<Utc>,
    task_terms: &BTreeSet<String>,
    by_id: &HashMap<&str, &ParsedNote>,
    inbound: &HashMap<String, usize>,
    lint_errors: &HashMap<&str, &str>,
) -> Evaluated<'a> {
    let temporal = temporal_status(parsed, request.stale_after_days, now);
    let conflict = if parsed
        .note
        .conflicts_with
        .iter()
        .any(|target| by_id.contains_key(target.as_str()))
    {
        ConflictStatus::Conflicted
    } else {
        ConflictStatus::Clear
    };
    let rejection = rejection_reason(parsed, request, temporal, conflict, lint_errors);
    let authority = if rejection.as_deref().is_some_and(|reason| {
        reason.contains("scope") || reason.contains("sensitivity") || reason.contains("trust")
    }) {
        AuthorityStatus::Unauthorized
    } else {
        AuthorityStatus::Authorized
    };
    let score = rank(parsed, request.ranking, task_terms, now, inbound);
    Evaluated {
        note: parsed,
        score,
        tokens: estimate_tokens(&parsed.body),
        temporal,
        conflict,
        authority,
        rejection,
        required: request.required_evidence.contains(&parsed.note.id),
    }
}

fn rejection_reason(
    parsed: &ParsedNote,
    request: &CompileRequest,
    temporal: TemporalStatus,
    conflict: ConflictStatus,
    lint_errors: &HashMap<&str, &str>,
) -> Option<String> {
    let note = &parsed.note;
    if !request.allowed_note_types.is_empty()
        && !request.allowed_note_types.contains(&note.note_type)
    {
        return Some("note type is not allowed by the task envelope".to_owned());
    }
    if !scope_contains(&note.scope.projects, Some(&request.caller.project))
        || !scope_contains(&note.scope.agents, request.caller.agent.as_deref())
        || !scope_contains(
            &note.scope.environments,
            request.caller.environment.as_deref(),
        )
    {
        return Some("caller is outside the note scope".to_owned());
    }
    if note.sensitivity > request.caller.max_sensitivity {
        return Some("note sensitivity exceeds caller clearance".to_owned());
    }
    if matches!(note.trust, TrustState::Untrusted | TrustState::Quarantined) {
        return Some("note trust state is not eligible for context".to_owned());
    }
    if let Some(code) = lint_errors.get(parsed.source_path.as_str()) {
        return Some(format!("note failed lint rule {code}"));
    }
    match temporal {
        TemporalStatus::Current => {}
        TemporalStatus::Stale => return Some("note is stale".to_owned()),
        TemporalStatus::Expired => return Some("note validity has expired".to_owned()),
        TemporalStatus::NotYetValid => return Some("note is not yet valid".to_owned()),
    }
    if conflict == ConflictStatus::Conflicted {
        return Some("note has an unresolved conflict".to_owned());
    }
    None
}

fn temporal_status(
    parsed: &ParsedNote,
    stale_after_days: i64,
    now: DateTime<Utc>,
) -> TemporalStatus {
    if parsed
        .note
        .validity
        .valid_from
        .is_some_and(|from| from > now)
    {
        return TemporalStatus::NotYetValid;
    }
    if parsed
        .note
        .validity
        .valid_until
        .is_some_and(|until| until < now)
    {
        return TemporalStatus::Expired;
    }
    if matches!(
        parsed.note.note_type,
        NoteType::Fact | NoteType::Observation
    ) && now - parsed.note.provenance.observed_at > Duration::days(stale_after_days)
    {
        return TemporalStatus::Stale;
    }
    TemporalStatus::Current
}

fn scope_contains(values: &[String], caller_value: Option<&str>) -> bool {
    values.is_empty()
        || values.iter().any(|value| value == "*")
        || caller_value.is_some_and(|caller| values.iter().any(|value| value == caller))
}

fn rank(
    parsed: &ParsedNote,
    policy: RankingPolicy,
    task_terms: &BTreeSet<String>,
    now: DateTime<Utc>,
    inbound: &HashMap<String, usize>,
) -> i64 {
    let metadata = metadata_score(parsed, now);
    let lexical = lexical_score(parsed, task_terms);
    match policy {
        RankingPolicy::Metadata => metadata,
        RankingPolicy::Lexical => metadata + lexical,
        RankingPolicy::VectorFixture => metadata + vector_fixture_score(parsed, task_terms),
        RankingPolicy::BoundedLinks => {
            metadata
                + lexical
                + i64::try_from(*inbound.get(&parsed.note.id).unwrap_or(&0)).unwrap_or(i64::MAX)
                    * 50
        }
    }
}

fn metadata_score(parsed: &ParsedNote, now: DateTime<Utc>) -> i64 {
    let trust = match parsed.note.trust {
        TrustState::Trusted => 400,
        TrustState::Reviewed => 250,
        TrustState::Untrusted => 50,
        TrustState::Quarantined => 0,
    };
    let age_days = (now - parsed.note.provenance.observed_at).num_days().max(0);
    trust + (200 - age_days).max(0)
}

fn lexical_score(parsed: &ParsedNote, task_terms: &BTreeSet<String>) -> i64 {
    let body_terms = terms(&parsed.body);
    let title_terms = parsed
        .note
        .title
        .as_deref()
        .map_or_else(BTreeSet::new, terms);
    let body_overlap = task_terms.intersection(&body_terms).count();
    let title_overlap = task_terms.intersection(&title_terms).count();
    i64::try_from(body_overlap * 1000 + title_overlap * 500).unwrap_or(i64::MAX)
}

fn vector_fixture_score(parsed: &ParsedNote, task_terms: &BTreeSet<String>) -> i64 {
    const DIMENSIONS: usize = 64;
    let mut query = [0_i64; DIMENSIONS];
    let mut document = [0_i64; DIMENSIONS];
    for term in task_terms {
        add_hashed_term(&mut query, term);
    }
    for term in terms(&format!(
        "{} {}",
        parsed.note.title.as_deref().unwrap_or_default(),
        parsed.body
    )) {
        add_hashed_term(&mut document, &term);
    }
    query
        .iter()
        .zip(document)
        .map(|(left, right)| left * right)
        .sum::<i64>()
        * 100
}

fn add_hashed_term(vector: &mut [i64; 64], term: &str) {
    let digest = Sha256::digest(term.as_bytes());
    let index = usize::from(digest[0]) % vector.len();
    let sign = if digest[1] & 1 == 0 { 1 } else { -1 };
    vector[index] += sign;
}

fn terms(text: &str) -> BTreeSet<String> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|term| term.len() > 1)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn inbound_link_counts(notes: &[ParsedNote]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for link in notes.iter().flat_map(|note| &note.links) {
        let target = link.target.split(['#', '|']).next().unwrap_or(&link.target);
        *counts.entry(target.to_owned()).or_default() += 1;
    }
    counts
}

fn estimate_tokens(text: &str) -> usize {
    let characters = text.chars().count();
    characters.div_ceil(4).max(1)
}

fn source_location(parsed: &ParsedNote) -> SourceLocation {
    SourceLocation {
        file: parsed.source_path.clone(),
        line: 1,
        column: 1,
    }
}

fn digest_sorted_notes(notes: &[ParsedNote]) -> String {
    let mut ordered: Vec<_> = notes.iter().collect();
    ordered.sort_by_key(|note| (&note.note.id, &note.source_path));
    digest_json(&ordered)
}

fn digest_json(value: &impl Serialize) -> String {
    let bytes = serde_json::to_vec(value).expect("context pack types serialize");
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a string succeeds");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use chrono::TimeZone;

    use super::*;
    use crate::parse_note;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 12, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    fn baseline(id: &str, body: &str) -> ParsedNote {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/vault/01-current-policy.md");
        let source = fs::read_to_string(fixture).expect("fixture is readable");
        let mut parsed = parse_note(format!("{id}.md"), &source).expect("fixture parses");
        parsed.note.id = id.to_owned();
        parsed.note.title = Some(id.replace('-', " "));
        parsed.note.note_type = NoteType::Fact;
        parsed.note.scope.projects = vec!["memoryfs".to_owned()];
        parsed.note.scope.agents = vec!["compiler".to_owned()];
        parsed.note.scope.environments = vec!["development".to_owned()];
        parsed.note.provenance.source = "test".to_owned();
        parsed.note.provenance.author = Some("tester".to_owned());
        parsed.note.provenance.observed_at = now() - Duration::days(1);
        parsed.note.trust = TrustState::Trusted;
        parsed.note.sensitivity = Sensitivity::Internal;
        parsed.note.validity.valid_from = None;
        parsed.note.validity.valid_until = None;
        parsed.note.retention_days = None;
        parsed.note.confidence = Some(0.9);
        parsed.note.supersedes.clear();
        parsed.note.conflicts_with.clear();
        parsed.note.suppressions.clear();
        parsed.body = body.to_owned();
        parsed.links.clear();
        parsed.attachments.clear();
        parsed
    }

    fn request(ranking: RankingPolicy) -> CompileRequest {
        CompileRequest {
            task: "prepare a production deployment checklist".to_owned(),
            caller: CallerScope {
                project: "memoryfs".to_owned(),
                agent: Some("compiler".to_owned()),
                environment: Some("development".to_owned()),
                max_sensitivity: Sensitivity::Internal,
            },
            allowed_note_types: BTreeSet::new(),
            token_budget: 1500,
            required_evidence: BTreeSet::new(),
            policy_version: "policy-v1".to_owned(),
            ranking,
            stale_after_days: 180,
            max_link_depth: 1,
        }
    }

    #[test]
    fn all_ranking_policies_prefer_relevant_evidence_and_explain_every_candidate() {
        let relevant = baseline(
            "deployment-checklist",
            "Prepare the production deployment checklist and review approvals.",
        );
        let unrelated = baseline("lunch-menu", "Choose soup and salad for lunch.");

        for ranking in [
            RankingPolicy::Metadata,
            RankingPolicy::Lexical,
            RankingPolicy::VectorFixture,
            RankingPolicy::BoundedLinks,
        ] {
            let pack = compile_context(
                &[relevant.clone(), unrelated.clone()],
                &request(ranking),
                now(),
            )
            .expect("pack compiles");
            assert_eq!(pack.candidates.len(), 2);
            assert!(pack.candidates.iter().all(|candidate| {
                !candidate.reason.is_empty()
                    && candidate.location.line > 0
                    && candidate.location.column > 0
            }));
            if ranking != RankingPolicy::Metadata {
                assert_eq!(pack.excerpts[0].note_id, "deployment-checklist");
            }
        }
    }

    #[test]
    fn budget_excludes_low_ranked_candidates_without_overflowing() {
        let first = baseline("deployment", "production deployment checklist");
        let second = baseline("other", "a different supporting note");
        let mut request = request(RankingPolicy::Lexical);
        request.token_budget = 8;
        let pack = compile_context(&[first, second], &request, now()).expect("pack compiles");

        assert!(pack.total_estimated_tokens <= 8);
        assert!(pack.excerpts.len() < pack.candidates.len());
        assert!(pack.candidates.iter().any(|candidate| {
            candidate.decision == CandidateDecision::Excluded
                && candidate.reason.contains("token budget")
        }));
    }

    #[test]
    fn refuses_required_evidence_that_is_unauthorized_stale_conflicted_or_over_budget() {
        let mut unauthorized = baseline("required", "required evidence");
        unauthorized.note.sensitivity = Sensitivity::Restricted;
        assert_refused(unauthorized, request(RankingPolicy::Lexical), "sensitivity");

        let mut stale = baseline("required", "required evidence");
        stale.note.provenance.observed_at = now() - Duration::days(181);
        assert_refused(stale, request(RankingPolicy::Lexical), "stale");

        let mut conflicted = baseline("required", "required evidence");
        conflicted.note.conflicts_with.push("other".to_owned());
        let mut other = baseline("other", "contradictory evidence");
        other.note.conflicts_with.push("required".to_owned());
        let mut conflict_request = request(RankingPolicy::Lexical);
        conflict_request
            .required_evidence
            .insert("required".to_owned());
        let error = compile_context(&[conflicted, other], &conflict_request, now())
            .expect_err("conflicted evidence is refused");
        assert!(error.to_string().contains("unresolved conflict"));

        let oversized = baseline("required", &"word ".repeat(100));
        let mut small_request = request(RankingPolicy::Lexical);
        small_request.token_budget = 2;
        assert_refused(oversized, small_request, "budget");
    }

    fn assert_refused(note: ParsedNote, mut request: CompileRequest, expected: &str) {
        request.required_evidence.insert("required".to_owned());
        let error = compile_context(&[note], &request, now()).expect_err("evidence is refused");
        assert!(
            error.to_string().contains(expected),
            "expected {expected} in {error}"
        );
    }

    #[test]
    fn untrusted_memory_never_enters_a_pack() {
        let mut untrusted = baseline("untrusted", "production deployment checklist");
        untrusted.note.trust = TrustState::Untrusted;
        let trusted = baseline("trusted", "supporting evidence");
        let pack = compile_context(
            &[untrusted, trusted],
            &request(RankingPolicy::Lexical),
            now(),
        )
        .expect("pack compiles");

        assert!(
            pack.excerpts
                .iter()
                .all(|excerpt| excerpt.note_id != "untrusted")
        );
        let rejected = pack
            .candidates
            .iter()
            .find(|candidate| candidate.note_id == "untrusted")
            .expect("untrusted candidate has a receipt");
        assert_eq!(rejected.authority_status, AuthorityStatus::Unauthorized);
        assert_eq!(rejected.decision, CandidateDecision::Excluded);
    }

    #[test]
    fn pack_and_digest_are_reproducible_across_input_order() {
        let first = baseline("one", "production deployment evidence");
        let second = baseline("two", "deployment checklist approvals");
        let request = request(RankingPolicy::Lexical);
        let forward = compile_context(&[first.clone(), second.clone()], &request, now())
            .expect("pack compiles");
        let reverse = compile_context(&[second, first], &request, now()).expect("pack compiles");

        assert_eq!(forward, reverse);
        let mut unsigned = forward.clone();
        let expected = unsigned.receipt.value.clone();
        unsigned.receipt.value.clear();
        assert_eq!(expected, digest_json(&unsigned));
    }

    #[test]
    fn synthetic_vault_compiles_a_bounded_pack_with_a_receipt_for_all_ten_notes() {
        let vault = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault");
        let mut paths: Vec<_> = fs::read_dir(vault)
            .expect("vault exists")
            .map(|entry| entry.expect("entry is readable").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
            .collect();
        paths.sort();
        let notes: Vec<_> = paths
            .iter()
            .map(|path| {
                parse_note(
                    path.display().to_string(),
                    &fs::read_to_string(path).expect("fixture is readable"),
                )
                .expect("fixture parses")
            })
            .collect();
        let mut request = request(RankingPolicy::BoundedLinks);
        request.caller.environment = Some("production".to_owned());
        let pack = compile_context(&notes, &request, now()).expect("pack compiles");

        assert_eq!(pack.candidates.len(), 10);
        assert!(pack.total_estimated_tokens <= 1500);
        assert_eq!(pack.receipt.algorithm, "sha256");
        assert_eq!(pack.receipt.value.len(), 64);
        assert!(
            pack.candidates
                .iter()
                .all(|candidate| !candidate.reason.is_empty())
        );
    }
}
