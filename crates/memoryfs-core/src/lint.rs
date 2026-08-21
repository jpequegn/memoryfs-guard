use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::Write as _,
};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{LinkKind, NoteType, ParsedNote, Sensitivity, Severity, SourceLocation, TrustState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleInfo {
    pub code: &'static str,
    pub category: &'static str,
    pub default_severity: Severity,
}

pub const RULE_CATALOG: &[RuleInfo] = &[
    rule("empty_id", "correctness", Severity::Error),
    rule("duplicate_id", "correctness", Severity::Error),
    rule("missing_title", "correctness", Severity::Warning),
    rule("empty_provenance_source", "provenance", Severity::Error),
    rule(
        "instruction_author_missing",
        "provenance",
        Severity::Warning,
    ),
    rule("observation_in_future", "time", Severity::Error),
    rule("stale_observation", "time", Severity::Warning),
    rule("invalid_validity_range", "time", Severity::Error),
    rule("expired_note", "time", Severity::Warning),
    rule("retention_expired", "retention", Severity::Warning),
    rule("retention_too_long", "retention", Severity::Warning),
    rule("confidence_out_of_range", "correctness", Severity::Error),
    rule("broken_wiki_link", "correctness", Severity::Error),
    rule("broken_markdown_link", "correctness", Severity::Error),
    rule("orphan_attachment", "correctness", Severity::Warning),
    rule("link_budget_exceeded", "budget", Severity::Warning),
    rule("attachment_budget_exceeded", "budget", Severity::Warning),
    rule("body_budget_exceeded", "budget", Severity::Warning),
    rule("self_supersession", "correctness", Severity::Error),
    rule("missing_superseded_note", "correctness", Severity::Error),
    rule("supersession_cycle", "correctness", Severity::Error),
    rule("missing_conflict_target", "correctness", Severity::Error),
    rule("asymmetric_conflict", "correctness", Severity::Warning),
    rule("project_scope_missing", "scope", Severity::Warning),
    rule("authority_expansion", "scope", Severity::Error),
    rule("untrusted_instruction", "provenance", Severity::Error),
    rule("restricted_scope_too_broad", "privacy", Severity::Error),
    rule("secret_canary", "privacy", Severity::Error),
    rule("credential_pattern", "privacy", Severity::Error),
    rule("cross_project_leak", "privacy", Severity::Error),
    rule("duplicate_content", "correctness", Severity::Warning),
    rule("quarantined_note_active", "provenance", Severity::Warning),
];

const fn rule(code: &'static str, category: &'static str, default_severity: Severity) -> RuleInfo {
    RuleInfo {
        code,
        category,
        default_severity,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintContext {
    pub now: DateTime<Utc>,
    pub project: Option<String>,
    pub agent: Option<String>,
    pub environment: Option<String>,
    pub known_attachments: BTreeSet<String>,
    pub stale_after_days: i64,
    pub max_retention_days: u32,
    pub max_body_bytes: usize,
    pub max_links: usize,
    pub max_attachments: usize,
}

impl LintContext {
    #[must_use]
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            now,
            project: None,
            agent: None,
            environment: None,
            known_attachments: BTreeSet::new(),
            stale_after_days: 180,
            max_retention_days: 3650,
            max_body_bytes: 64 * 1024,
            max_links: 64,
            max_attachments: 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintFinding {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub note_id: String,
    pub location: SourceLocation,
    pub suppressed: bool,
    pub suppression_reason: Option<String>,
    pub suppression_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintReceipt {
    pub schema_version: u32,
    pub evaluated_at: DateTime<Utc>,
    pub vault_digest: String,
    pub findings: Vec<LintFinding>,
    pub digest: String,
}

#[must_use]
pub fn lint_vault(notes: &[ParsedNote], context: &LintContext) -> LintReceipt {
    let mut findings = Vec::new();
    let by_id = index_by_id(notes);

    lint_per_note(notes, context, &by_id, &mut findings);
    lint_relationships(notes, &by_id, &mut findings);
    lint_duplicates(notes, &mut findings);
    apply_suppressions(notes, context.now, &mut findings);
    findings.sort_by(|left, right| {
        (
            &left.code,
            &left.location.file,
            left.location.line,
            left.location.column,
            &left.message,
        )
            .cmp(&(
                &right.code,
                &right.location.file,
                right.location.line,
                right.location.column,
                &right.message,
            ))
    });

    let vault_digest = vault_digest(notes);
    let payload = ReceiptPayload {
        schema_version: 1,
        evaluated_at: context.now,
        vault_digest: &vault_digest,
        findings: &findings,
    };
    let digest = digest_json(&payload);
    LintReceipt {
        schema_version: 1,
        evaluated_at: context.now,
        vault_digest,
        findings,
        digest,
    }
}

#[derive(Serialize)]
struct ReceiptPayload<'a> {
    schema_version: u32,
    evaluated_at: DateTime<Utc>,
    vault_digest: &'a str,
    findings: &'a [LintFinding],
}

#[allow(clippy::too_many_lines)]
fn lint_per_note(
    notes: &[ParsedNote],
    context: &LintContext,
    by_id: &HashMap<&str, Vec<&ParsedNote>>,
    findings: &mut Vec<LintFinding>,
) {
    for parsed in notes {
        let note = &parsed.note;
        if note.id.trim().is_empty() {
            push(
                findings,
                parsed,
                "empty_id",
                "note identifier is empty",
                None,
            );
        }
        if note
            .title
            .as_ref()
            .is_none_or(|title| title.trim().is_empty())
        {
            push(
                findings,
                parsed,
                "missing_title",
                "note title is missing",
                None,
            );
        }
        if note.provenance.source.trim().is_empty() {
            push(
                findings,
                parsed,
                "empty_provenance_source",
                "provenance source is empty",
                None,
            );
        }
        if matches!(note.note_type, NoteType::Instruction) && note.provenance.author.is_none() {
            push(
                findings,
                parsed,
                "instruction_author_missing",
                "instruction has no accountable author",
                None,
            );
        }
        if note.provenance.observed_at > context.now + Duration::minutes(5) {
            push(
                findings,
                parsed,
                "observation_in_future",
                "provenance timestamp is in the future",
                None,
            );
        }
        if matches!(note.note_type, NoteType::Fact | NoteType::Observation)
            && context.now - note.provenance.observed_at > Duration::days(context.stale_after_days)
        {
            push(
                findings,
                parsed,
                "stale_observation",
                "fact or observation is older than the staleness threshold",
                None,
            );
        }
        if let (Some(from), Some(until)) = (note.validity.valid_from, note.validity.valid_until)
            && from > until
        {
            push(
                findings,
                parsed,
                "invalid_validity_range",
                "valid_from occurs after valid_until",
                None,
            );
        }
        if note
            .validity
            .valid_until
            .is_some_and(|until| until < context.now)
        {
            push(
                findings,
                parsed,
                "expired_note",
                "note validity has expired",
                None,
            );
        }
        if let Some(days) = note.retention_days {
            if note.provenance.observed_at + Duration::days(i64::from(days)) < context.now {
                push(
                    findings,
                    parsed,
                    "retention_expired",
                    "note exceeded its retention period",
                    None,
                );
            }
            if days > context.max_retention_days {
                push(
                    findings,
                    parsed,
                    "retention_too_long",
                    "retention period exceeds policy",
                    None,
                );
            }
        }
        if note
            .confidence
            .is_some_and(|value| !(0.0..=1.0).contains(&value))
        {
            push(
                findings,
                parsed,
                "confidence_out_of_range",
                "confidence must be between zero and one",
                None,
            );
        }
        lint_links(parsed, by_id, context, findings);
        if parsed.links.len() > context.max_links {
            push(
                findings,
                parsed,
                "link_budget_exceeded",
                "note exceeds the link budget",
                None,
            );
        }
        if parsed.attachments.len() > context.max_attachments {
            push(
                findings,
                parsed,
                "attachment_budget_exceeded",
                "note exceeds the attachment budget",
                None,
            );
        }
        if parsed.body.len() > context.max_body_bytes {
            push(
                findings,
                parsed,
                "body_budget_exceeded",
                "note body exceeds the byte budget",
                None,
            );
        }
        for target in &note.supersedes {
            if target == &note.id {
                push(
                    findings,
                    parsed,
                    "self_supersession",
                    "note supersedes itself",
                    None,
                );
            } else if !by_id.contains_key(target.as_str()) {
                push(
                    findings,
                    parsed,
                    "missing_superseded_note",
                    "superseded note does not exist",
                    None,
                );
            }
        }
        if note.scope.projects.is_empty() {
            push(
                findings,
                parsed,
                "project_scope_missing",
                "note has no project scope",
                None,
            );
        }
        if matches!(note.note_type, NoteType::Instruction | NoteType::Policy)
            && (note.scope.projects.iter().any(|value| value == "*")
                || note.scope.agents.iter().any(|value| value == "*")
                || note.scope.environments.iter().any(|value| value == "*"))
        {
            push(
                findings,
                parsed,
                "authority_expansion",
                "instruction or policy grants wildcard authority",
                None,
            );
        }
        if matches!(note.note_type, NoteType::Instruction)
            && matches!(note.trust, TrustState::Untrusted | TrustState::Quarantined)
        {
            push(
                findings,
                parsed,
                "untrusted_instruction",
                "untrusted content attempts to provide instructions",
                None,
            );
        }
        if note.sensitivity == Sensitivity::Restricted
            && (note.scope.projects.is_empty() || note.scope.projects.len() > 1)
        {
            push(
                findings,
                parsed,
                "restricted_scope_too_broad",
                "restricted note must target exactly one project",
                None,
            );
        }
        if parsed.body.contains("AKIAIOSFODNN7EXAMPLE") {
            push(
                findings,
                parsed,
                "secret_canary",
                "synthetic secret canary detected",
                find_location(parsed, "AKIAIOSFODNN7EXAMPLE"),
            );
        }
        if contains_credential_pattern(&parsed.body) {
            push(
                findings,
                parsed,
                "credential_pattern",
                "credential-like token detected",
                find_location(parsed, "AKIA"),
            );
        }
        if matches!(note.trust, TrustState::Quarantined) {
            push(
                findings,
                parsed,
                "quarantined_note_active",
                "quarantined note is present in the active vault",
                None,
            );
        }
    }
}

fn lint_links(
    parsed: &ParsedNote,
    by_id: &HashMap<&str, Vec<&ParsedNote>>,
    context: &LintContext,
    findings: &mut Vec<LintFinding>,
) {
    for link in &parsed.links {
        let target = normalized_target(&link.target);
        let resolved = resolve_target(target, by_id);
        if resolved.is_none() && !is_external(&link.target) {
            let code = if link.kind == LinkKind::Wiki {
                "broken_wiki_link"
            } else {
                "broken_markdown_link"
            };
            push(
                findings,
                parsed,
                code,
                "link target does not exist",
                Some(link.location.clone()),
            );
        }
        if let Some(target_note) = resolved
            && parsed.note.sensitivity != Sensitivity::Public
            && scopes_are_disjoint(
                &parsed.note.scope.projects,
                &target_note.note.scope.projects,
            )
        {
            push(
                findings,
                parsed,
                "cross_project_leak",
                "non-public link crosses disjoint project scopes",
                Some(link.location.clone()),
            );
        }
    }
    for attachment in &parsed.attachments {
        if !context.known_attachments.contains(&attachment.target) {
            push(
                findings,
                parsed,
                "orphan_attachment",
                "attachment is not present in the attachment index",
                Some(attachment.location.clone()),
            );
        }
    }
}

fn lint_relationships(
    notes: &[ParsedNote],
    by_id: &HashMap<&str, Vec<&ParsedNote>>,
    findings: &mut Vec<LintFinding>,
) {
    for parsed in notes {
        for target in &parsed.note.conflicts_with {
            match by_id
                .get(target.as_str())
                .and_then(|matches| matches.first())
            {
                None => push(
                    findings,
                    parsed,
                    "missing_conflict_target",
                    "conflict target does not exist",
                    None,
                ),
                Some(other) if !other.note.conflicts_with.contains(&parsed.note.id) => push(
                    findings,
                    parsed,
                    "asymmetric_conflict",
                    "conflict relationship is not reciprocal",
                    None,
                ),
                Some(_) => {}
            }
        }
    }

    let cycle_ids = supersession_cycle_ids(notes, by_id);
    for parsed in notes {
        if cycle_ids.contains(&parsed.note.id) {
            push(
                findings,
                parsed,
                "supersession_cycle",
                "note participates in a supersession cycle",
                None,
            );
        }
    }
}

fn lint_duplicates(notes: &[ParsedNote], findings: &mut Vec<LintFinding>) {
    let mut ids: BTreeMap<&str, Vec<&ParsedNote>> = BTreeMap::new();
    let mut bodies: BTreeMap<&str, Vec<&ParsedNote>> = BTreeMap::new();
    for parsed in notes {
        ids.entry(&parsed.note.id).or_default().push(parsed);
        let body = parsed.body.trim();
        if !body.is_empty() {
            bodies.entry(body).or_default().push(parsed);
        }
    }
    for matches in ids.values().filter(|matches| matches.len() > 1) {
        for parsed in matches {
            push(
                findings,
                parsed,
                "duplicate_id",
                "identifier is used by more than one note",
                None,
            );
        }
    }
    for matches in bodies.values().filter(|matches| matches.len() > 1) {
        for parsed in matches {
            push(
                findings,
                parsed,
                "duplicate_content",
                "body is duplicated by another note",
                None,
            );
        }
    }
}

fn supersession_cycle_ids(
    notes: &[ParsedNote],
    by_id: &HashMap<&str, Vec<&ParsedNote>>,
) -> HashSet<String> {
    let mut cycle_ids = HashSet::new();
    for parsed in notes {
        let origin = parsed.note.id.as_str();
        let mut stack = parsed.note.supersedes.clone();
        let mut seen = HashSet::new();
        while let Some(id) = stack.pop() {
            if id == origin {
                cycle_ids.insert(origin.to_owned());
                break;
            }
            if seen.insert(id.clone())
                && let Some(next) = by_id.get(id.as_str()).and_then(|matches| matches.first())
            {
                stack.extend(next.note.supersedes.iter().cloned());
            }
        }
    }
    cycle_ids
}

fn apply_suppressions(notes: &[ParsedNote], now: DateTime<Utc>, findings: &mut [LintFinding]) {
    for finding in findings {
        let Some(note) = notes
            .iter()
            .find(|note| note.source_path == finding.location.file)
        else {
            continue;
        };
        if let Some(suppression) = note
            .note
            .suppressions
            .iter()
            .find(|suppression| suppression.code == finding.code && suppression.until >= now)
        {
            finding.suppressed = true;
            finding.suppression_reason = Some(suppression.reason.clone());
            finding.suppression_until = Some(suppression.until);
        }
    }
}

fn index_by_id(notes: &[ParsedNote]) -> HashMap<&str, Vec<&ParsedNote>> {
    let mut index: HashMap<&str, Vec<&ParsedNote>> = HashMap::new();
    for note in notes {
        index.entry(&note.note.id).or_default().push(note);
    }
    index
}

fn resolve_target<'a>(
    target: &str,
    by_id: &'a HashMap<&str, Vec<&'a ParsedNote>>,
) -> Option<&'a ParsedNote> {
    if let Some(note) = by_id.get(target).and_then(|matches| matches.first()) {
        return Some(*note);
    }
    by_id.values().flatten().copied().find(|note| {
        note.source_path == target
            || note.source_path.ends_with(&format!("/{target}"))
            || note
                .source_path
                .rsplit('/')
                .next()
                .is_some_and(|name| name == target)
    })
}

fn normalized_target(target: &str) -> &str {
    target.split(['#', '|']).next().unwrap_or(target)
}

fn is_external(target: &str) -> bool {
    target.starts_with("https://") || target.starts_with("http://") || target.starts_with("mailto:")
}

fn scopes_are_disjoint(left: &[String], right: &[String]) -> bool {
    !left.is_empty()
        && !right.is_empty()
        && !left
            .iter()
            .any(|project| right.contains(project) || project == "*")
        && !right.iter().any(|project| project == "*")
}

fn contains_credential_pattern(body: &str) -> bool {
    body.split_whitespace().any(|raw| {
        let token = raw
            .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '_');
        (token.starts_with("AKIA") && token.len() == 20)
            || (token.starts_with("ghp_") && token.len() >= 20)
    })
}

fn push(
    findings: &mut Vec<LintFinding>,
    parsed: &ParsedNote,
    code: &str,
    message: &str,
    location: Option<SourceLocation>,
) {
    let severity = RULE_CATALOG
        .iter()
        .find(|rule| rule.code == code)
        .map_or(Severity::Error, |rule| rule.default_severity);
    findings.push(LintFinding {
        code: code.to_owned(),
        severity,
        message: message.to_owned(),
        note_id: parsed.note.id.clone(),
        location: location.unwrap_or_else(|| SourceLocation {
            file: parsed.source_path.clone(),
            line: 1,
            column: 1,
        }),
        suppressed: false,
        suppression_reason: None,
        suppression_until: None,
    });
}

fn find_location(parsed: &ParsedNote, needle: &str) -> Option<SourceLocation> {
    let offset = parsed.source.find(needle)?;
    let prefix = &parsed.source[..offset];
    Some(SourceLocation {
        file: parsed.source_path.clone(),
        line: prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
        column: prefix
            .rsplit_once('\n')
            .map_or(prefix.len() + 1, |(_, tail)| tail.len() + 1),
    })
}

fn vault_digest(notes: &[ParsedNote]) -> String {
    let mut ordered: Vec<_> = notes.iter().collect();
    ordered.sort_by_key(|note| &note.source_path);
    digest_json(&ordered)
}

fn digest_json(value: &impl Serialize) -> String {
    let bytes = serde_json::to_vec(value).expect("lint receipt types serialize");
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
    use crate::{Attachment, MemoryLink, Suppression, parse_note};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 12, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    fn baseline() -> ParsedNote {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/vault/01-current-policy.md");
        let source = fs::read_to_string(path).expect("fixture is readable");
        let mut parsed = parse_note("base.md", &source).expect("fixture parses");
        parsed.note.id = "base".to_owned();
        parsed.note.title = Some("Baseline".to_owned());
        parsed.note.note_type = NoteType::Fact;
        parsed.note.scope.projects = vec!["memoryfs".to_owned()];
        parsed.note.scope.agents = vec!["compiler".to_owned()];
        parsed.note.scope.environments = vec!["development".to_owned()];
        parsed.note.provenance.source = "test".to_owned();
        parsed.note.provenance.author = Some("tester".to_owned());
        parsed.note.provenance.observed_at = Utc
            .with_ymd_and_hms(2026, 8, 1, 0, 0, 0)
            .single()
            .expect("valid timestamp");
        parsed.note.trust = TrustState::Trusted;
        parsed.note.sensitivity = Sensitivity::Internal;
        parsed.note.validity.valid_from = None;
        parsed.note.validity.valid_until = None;
        parsed.note.retention_days = Some(30);
        parsed.note.confidence = Some(0.9);
        parsed.note.supersedes.clear();
        parsed.note.conflicts_with.clear();
        parsed.note.suppressions.clear();
        parsed.body = "Unique baseline body".to_owned();
        parsed.links.clear();
        parsed.attachments.clear();
        parsed.diagnostics.clear();
        parsed
    }

    fn context() -> LintContext {
        LintContext::new(now())
    }

    fn location(file: &str) -> SourceLocation {
        SourceLocation {
            file: file.to_owned(),
            line: 2,
            column: 3,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn trigger_case(code: &str) -> (Vec<ParsedNote>, LintContext) {
        let mut note = baseline();
        let mut context = context();
        match code {
            "empty_id" => note.note.id.clear(),
            "duplicate_id" => {
                let mut duplicate = note.clone();
                duplicate.source_path = "duplicate.md".to_owned();
                duplicate.body = "Different body".to_owned();
                return (vec![note, duplicate], context);
            }
            "missing_title" => note.note.title = None,
            "empty_provenance_source" => note.note.provenance.source.clear(),
            "instruction_author_missing" => {
                note.note.note_type = NoteType::Instruction;
                note.note.provenance.author = None;
            }
            "observation_in_future" => note.note.provenance.observed_at = now() + Duration::days(1),
            "stale_observation" => note.note.provenance.observed_at = now() - Duration::days(181),
            "invalid_validity_range" => {
                note.note.validity.valid_from = Some(now() + Duration::days(2));
                note.note.validity.valid_until = Some(now() + Duration::days(1));
            }
            "expired_note" => note.note.validity.valid_until = Some(now() - Duration::days(1)),
            "retention_expired" => {
                note.note.note_type = NoteType::Decision;
                note.note.provenance.observed_at = now() - Duration::days(31);
            }
            "retention_too_long" => note.note.retention_days = Some(3651),
            "confidence_out_of_range" => note.note.confidence = Some(1.1),
            "broken_wiki_link" => note.links.push(MemoryLink {
                target: "missing".to_owned(),
                label: None,
                kind: LinkKind::Wiki,
                location: location("base.md"),
            }),
            "broken_markdown_link" => note.links.push(MemoryLink {
                target: "missing.md".to_owned(),
                label: None,
                kind: LinkKind::Markdown,
                location: location("base.md"),
            }),
            "orphan_attachment" => note.attachments.push(Attachment {
                target: "missing.png".to_owned(),
                alt_text: None,
                location: location("base.md"),
            }),
            "link_budget_exceeded" => {
                context.max_links = 0;
                note.links.push(MemoryLink {
                    target: "https://example.com".to_owned(),
                    label: None,
                    kind: LinkKind::Markdown,
                    location: location("base.md"),
                });
            }
            "attachment_budget_exceeded" => {
                context.max_attachments = 0;
                context.known_attachments.insert("known.png".to_owned());
                note.attachments.push(Attachment {
                    target: "known.png".to_owned(),
                    alt_text: None,
                    location: location("base.md"),
                });
            }
            "body_budget_exceeded" => context.max_body_bytes = 1,
            "self_supersession" => note.note.supersedes.push("base".to_owned()),
            "missing_superseded_note" => note.note.supersedes.push("missing".to_owned()),
            "supersession_cycle" => {
                note.note.supersedes.push("second".to_owned());
                let mut second = baseline();
                second.note.id = "second".to_owned();
                second.source_path = "second.md".to_owned();
                second.body = "Second body".to_owned();
                second.note.supersedes.push("base".to_owned());
                return (vec![note, second], context);
            }
            "missing_conflict_target" => note.note.conflicts_with.push("missing".to_owned()),
            "asymmetric_conflict" => {
                note.note.conflicts_with.push("second".to_owned());
                let mut second = baseline();
                second.note.id = "second".to_owned();
                second.source_path = "second.md".to_owned();
                second.body = "Second body".to_owned();
                return (vec![note, second], context);
            }
            "project_scope_missing" => note.note.scope.projects.clear(),
            "authority_expansion" => {
                note.note.note_type = NoteType::Policy;
                note.note.scope.agents = vec!["*".to_owned()];
            }
            "untrusted_instruction" => {
                note.note.note_type = NoteType::Instruction;
                note.note.trust = TrustState::Untrusted;
            }
            "restricted_scope_too_broad" => {
                note.note.sensitivity = Sensitivity::Restricted;
                note.note.scope.projects = vec!["one".to_owned(), "two".to_owned()];
            }
            "secret_canary" => {
                note.body = "AKIAIOSFODNN7EXAMPLE".to_owned();
                note.source.push_str("\nAKIAIOSFODNN7EXAMPLE");
            }
            "credential_pattern" => {
                note.body = "ghp_1234567890abcdefghij".to_owned();
                note.source.push_str("\nghp_1234567890abcdefghij");
            }
            "cross_project_leak" => {
                note.links.push(MemoryLink {
                    target: "second".to_owned(),
                    label: None,
                    kind: LinkKind::Wiki,
                    location: location("base.md"),
                });
                let mut second = baseline();
                second.note.id = "second".to_owned();
                second.note.scope.projects = vec!["other".to_owned()];
                second.source_path = "second.md".to_owned();
                second.body = "Second body".to_owned();
                return (vec![note, second], context);
            }
            "duplicate_content" => {
                let mut duplicate = note.clone();
                duplicate.note.id = "second".to_owned();
                duplicate.source_path = "second.md".to_owned();
                return (vec![note, duplicate], context);
            }
            "quarantined_note_active" => note.note.trust = TrustState::Quarantined,
            _ => panic!("missing positive fixture for {code}"),
        }
        (vec![note], context)
    }

    #[test]
    fn rule_matrix_has_positive_and_negative_fixture_for_every_rule() {
        assert!(RULE_CATALOG.len() >= 25);
        let clean = lint_vault(&[baseline()], &context());
        assert!(
            clean.findings.is_empty(),
            "baseline must be a negative fixture"
        );

        for rule in RULE_CATALOG {
            let (notes, context) = trigger_case(rule.code);
            let receipt = lint_vault(&notes, &context);
            assert!(
                receipt
                    .findings
                    .iter()
                    .any(|finding| finding.code == rule.code),
                "positive fixture did not trigger {}: {:#?}",
                rule.code,
                receipt.findings
            );
        }
    }

    #[test]
    fn active_suppression_is_recorded_but_expired_one_does_not_hide_finding() {
        let mut active = baseline();
        active.note.title = None;
        active.note.suppressions.push(Suppression {
            code: "missing_title".to_owned(),
            reason: "migration in progress".to_owned(),
            until: now() + Duration::days(1),
            approved_by: Some("owner".to_owned()),
        });
        let active_receipt = lint_vault(&[active], &context());
        let finding = active_receipt
            .findings
            .iter()
            .find(|finding| finding.code == "missing_title")
            .expect("finding is retained");
        assert!(finding.suppressed);
        assert_eq!(
            finding.suppression_reason.as_deref(),
            Some("migration in progress")
        );

        let mut expired = baseline();
        expired.note.title = None;
        expired.note.suppressions.push(Suppression {
            code: "missing_title".to_owned(),
            reason: "expired exception".to_owned(),
            until: now() - Duration::seconds(1),
            approved_by: None,
        });
        let expired_receipt = lint_vault(&[expired], &context());
        assert!(
            expired_receipt
                .findings
                .iter()
                .any(|finding| finding.code == "missing_title" && !finding.suppressed)
        );
    }

    #[test]
    fn receipt_digest_is_reproducible_and_order_independent() {
        let first = baseline();
        let mut second = baseline();
        second.note.id = "second".to_owned();
        second.source_path = "second.md".to_owned();
        second.body = "Second body".to_owned();
        let forward = lint_vault(&[first.clone(), second.clone()], &context());
        let reverse = lint_vault(&[second, first], &context());

        assert_eq!(forward.vault_digest, reverse.vault_digest);
        assert_eq!(forward.digest, reverse.digest);
        assert_eq!(
            forward,
            lint_vault(
                &[baseline(), {
                    let mut note = baseline();
                    note.note.id = "second".to_owned();
                    note.source_path = "second.md".to_owned();
                    note.body = "Second body".to_owned();
                    note
                }],
                &context()
            )
        );
    }
}
