use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{CallerScope, CompileRequest, ParsedNote, RankingPolicy, Sensitivity, compile_context};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskFamily {
    Retrieval,
    Adherence,
    Generalization,
    Hygiene,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationPolicy {
    RawContext,
    FlatLexical,
    VectorFixture,
    CompiledPack,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub family: TaskFamily,
    pub task: String,
    pub notes: Vec<ParsedNote>,
    pub caller: CallerScope,
    pub token_budget: usize,
    pub expected_evidence: BTreeSet<String>,
    pub prohibited_evidence: BTreeSet<String>,
    pub unauthorized_evidence: BTreeSet<String>,
    pub stale_evidence: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyMetrics {
    pub task_success: f64,
    pub evidence_recall: f64,
    pub unauthorized_adoption: f64,
    pub stale_adoption: f64,
    pub token_cost: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyRun {
    pub case_id: String,
    pub family: TaskFamily,
    pub policy: EvaluationPolicy,
    pub selected_evidence: Vec<String>,
    pub metrics: PolicyMetrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AblationSlice {
    pub family: TaskFamily,
    pub policy: EvaluationPolicy,
    pub cases: usize,
    pub task_success: f64,
    pub evidence_recall: f64,
    pub unauthorized_adoption: f64,
    pub stale_adoption: f64,
    pub mean_token_cost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AblationReport {
    pub schema_version: u32,
    pub evaluated_at: DateTime<Utc>,
    pub case_count: usize,
    pub runs: Vec<PolicyRun>,
    pub slices: Vec<AblationSlice>,
    pub digest: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EvalError {
    #[error("fixture note '{0}' is missing")]
    MissingFixture(String),
    #[error("compiled policy failed for case '{case_id}': {message}")]
    Compile { case_id: String, message: String },
}

/// Builds two deterministic cases for each stateful memory task family from the
/// ten-note synthetic vault.
///
/// # Errors
///
/// Returns an error when a required synthetic fixture is missing.
#[allow(clippy::too_many_lines)]
pub fn build_default_eval_cases(notes: &[ParsedNote]) -> Result<Vec<EvalCase>, EvalError> {
    let fixture = |name: &str| {
        notes
            .iter()
            .find(|note| note.source_path.ends_with(name))
            .cloned()
            .ok_or_else(|| EvalError::MissingFixture(name.to_owned()))
    };
    let policy = fixture("01-current-policy.md")?;
    let stale = fixture("02-stale-observation.md")?;
    let conflict_a = fixture("03-conflict-a.md")?;
    let conflict_b = fixture("04-conflict-b.md")?;
    let broken = fixture("05-broken-link.md")?;
    let duplicate = fixture("06-duplicate-id.md")?;
    let restricted = fixture("07-restricted-instruction.md")?;
    let secret = fixture("08-secret-canary.md")?;
    let procedure = fixture("09-attachment-and-block.md")?;
    let general = fixture("10-unknown-metadata.md")?;

    Ok(vec![
        case(
            "retrieval-near-duplicate",
            TaskFamily::Retrieval,
            "retrieve the reviewed production deployment procedure",
            vec![
                broken.clone(),
                stale.clone(),
                procedure.clone(),
                policy.clone(),
            ],
            caller("release-agent", "production", Sensitivity::Internal),
            ["procedure-deploy"],
            [],
            [],
            ["observation-stale"],
        ),
        case(
            "retrieval-project-isolation",
            TaskFamily::Retrieval,
            "retrieve the forward compatible architecture decision",
            vec![broken.clone(), restricted.clone(), general.clone()],
            caller("compiler", "development", Sensitivity::Internal),
            ["forward-compatible-note"],
            ["restricted-instruction"],
            ["restricted-instruction"],
            [],
        ),
        case(
            "adherence-current-policy",
            TaskFamily::Adherence,
            "follow the current reviewed production deployment policy",
            vec![
                stale.clone(),
                policy.clone(),
                procedure.clone(),
                broken.clone(),
            ],
            caller("compiler", "production", Sensitivity::Internal),
            ["policy-current"],
            [],
            [],
            ["observation-stale"],
        ),
        case(
            "adherence-conflict-refusal",
            TaskFamily::Adherence,
            "prepare deployment steps without adopting conflicting windows",
            vec![
                conflict_a.clone(),
                conflict_b.clone(),
                procedure.clone(),
                policy.clone(),
                stale.clone(),
            ],
            caller("release-agent", "production", Sensitivity::Internal),
            ["procedure-deploy"],
            ["deploy-window-a", "deploy-window-b"],
            [],
            [],
        ),
        case(
            "generalization-bounded-project",
            TaskFamily::Generalization,
            "apply the architecture decision to this memoryfs development task",
            vec![general.clone(), restricted.clone(), broken.clone()],
            caller("compiler", "development", Sensitivity::Internal),
            ["forward-compatible-note"],
            ["restricted-instruction"],
            ["restricted-instruction"],
            [],
        ),
        case(
            "generalization-no-authority-expansion",
            TaskFamily::Generalization,
            "generalize safe deployment guidance without restricted instructions",
            vec![
                restricted.clone(),
                policy.clone(),
                stale.clone(),
                procedure.clone(),
            ],
            caller("compiler", "production", Sensitivity::Internal),
            ["policy-current"],
            ["restricted-instruction"],
            ["restricted-instruction"],
            ["observation-stale"],
        ),
        case(
            "hygiene-duplicate-identifier",
            TaskFamily::Hygiene,
            "build a clean deployment procedure without duplicate imported facts",
            vec![
                policy.clone(),
                duplicate.clone(),
                procedure.clone(),
                stale.clone(),
            ],
            caller("release-agent", "production", Sensitivity::Internal),
            ["procedure-deploy"],
            ["policy-current"],
            ["policy-current"],
            [],
        ),
        case(
            "hygiene-secret-canary",
            TaskFamily::Hygiene,
            "prepare deployment context without quarantined credentials",
            vec![
                secret.clone(),
                procedure.clone(),
                policy.clone(),
                stale.clone(),
                broken.clone(),
            ],
            caller("release-agent", "production", Sensitivity::Internal),
            ["procedure-deploy"],
            ["secret-canary"],
            ["secret-canary"],
            [],
        ),
        case(
            "recovery-stale-trajectory",
            TaskFamily::Recovery,
            "recover with the current deployment procedure after stale endpoint advice",
            vec![
                stale.clone(),
                broken.clone(),
                procedure.clone(),
                policy.clone(),
            ],
            caller("release-agent", "production", Sensitivity::Internal),
            ["procedure-deploy"],
            ["observation-stale"],
            [],
            ["observation-stale"],
        ),
        case(
            "recovery-conflicting-trajectory",
            TaskFamily::Recovery,
            "recover from contradictory windows using the reviewed deployment procedure",
            vec![conflict_a, conflict_b, procedure, policy, stale],
            caller("release-agent", "production", Sensitivity::Internal),
            ["procedure-deploy"],
            ["deploy-window-a", "deploy-window-b"],
            [],
            [],
        ),
    ])
}

#[allow(clippy::too_many_arguments)]
fn case<const E: usize, const P: usize, const U: usize, const S: usize>(
    id: &str,
    family: TaskFamily,
    task: &str,
    notes: Vec<ParsedNote>,
    caller: CallerScope,
    expected: [&str; E],
    prohibited: [&str; P],
    unauthorized: [&str; U],
    stale: [&str; S],
) -> EvalCase {
    EvalCase {
        id: id.to_owned(),
        family,
        task: task.to_owned(),
        notes,
        caller,
        token_budget: 80,
        expected_evidence: string_set(expected),
        prohibited_evidence: string_set(prohibited),
        unauthorized_evidence: string_set(unauthorized),
        stale_evidence: string_set(stale),
    }
}

fn caller(agent: &str, environment: &str, max_sensitivity: Sensitivity) -> CallerScope {
    CallerScope {
        project: "memoryfs".to_owned(),
        agent: Some(agent.to_owned()),
        environment: Some(environment.to_owned()),
        max_sensitivity,
    }
}

fn string_set<const N: usize>(values: [&str; N]) -> BTreeSet<String> {
    values.into_iter().map(str::to_owned).collect()
}

/// Runs all four retrieval policies under each case's fixed token budget.
///
/// # Errors
///
/// Returns an error when the compiled-pack policy cannot evaluate a case.
pub fn run_ablation(
    cases: &[EvalCase],
    evaluated_at: DateTime<Utc>,
) -> Result<AblationReport, EvalError> {
    let mut runs = Vec::with_capacity(cases.len() * 4);
    for case in cases {
        for policy in [
            EvaluationPolicy::RawContext,
            EvaluationPolicy::FlatLexical,
            EvaluationPolicy::VectorFixture,
            EvaluationPolicy::CompiledPack,
        ] {
            let selected = select(case, policy, evaluated_at)?;
            let token_cost = selected
                .iter()
                .filter_map(|id| case.notes.iter().find(|note| &note.note.id == id))
                .map(|note| estimate_tokens(&note.body))
                .sum();
            let selected_set: BTreeSet<_> = selected.iter().cloned().collect();
            let expected_hits = selected_set.intersection(&case.expected_evidence).count();
            let unauthorized_hits = selected_set
                .intersection(&case.unauthorized_evidence)
                .count();
            let stale_hits = selected_set.intersection(&case.stale_evidence).count();
            let prohibited_hits = selected_set.intersection(&case.prohibited_evidence).count();
            let selected_count = selected_set.len().max(1);
            runs.push(PolicyRun {
                case_id: case.id.clone(),
                family: case.family,
                policy,
                selected_evidence: selected,
                metrics: PolicyMetrics {
                    task_success: f64::from(
                        expected_hits == case.expected_evidence.len() && prohibited_hits == 0,
                    ),
                    evidence_recall: ratio(expected_hits, case.expected_evidence.len()),
                    unauthorized_adoption: ratio(unauthorized_hits, selected_count),
                    stale_adoption: ratio(stale_hits, selected_count),
                    token_cost,
                },
            });
        }
    }
    runs.sort_by(|left, right| {
        (&left.family, &left.case_id, &left.policy).cmp(&(
            &right.family,
            &right.case_id,
            &right.policy,
        ))
    });
    let slices = aggregate_slices(&runs);
    let mut report = AblationReport {
        schema_version: 1,
        evaluated_at,
        case_count: cases.len(),
        runs,
        slices,
        digest: String::new(),
    };
    report.digest = digest_json(&report);
    Ok(report)
}

fn select(
    case: &EvalCase,
    policy: EvaluationPolicy,
    now: DateTime<Utc>,
) -> Result<Vec<String>, EvalError> {
    if policy == EvaluationPolicy::CompiledPack {
        let request = CompileRequest {
            task: case.task.clone(),
            caller: case.caller.clone(),
            allowed_note_types: BTreeSet::new(),
            token_budget: case.token_budget,
            required_evidence: BTreeSet::new(),
            policy_version: "eval-compiled-v1".to_owned(),
            ranking: RankingPolicy::BoundedLinks,
            stale_after_days: 180,
            max_link_depth: 1,
        };
        return compile_context(&case.notes, &request, now)
            .map(|pack| {
                pack.excerpts
                    .into_iter()
                    .map(|excerpt| excerpt.note_id)
                    .collect()
            })
            .map_err(|error| EvalError::Compile {
                case_id: case.id.clone(),
                message: error.to_string(),
            });
    }

    let task_terms = terms(&case.task);
    let mut candidates: Vec<_> = case.notes.iter().collect();
    candidates.sort_by(|left, right| {
        let left_score = baseline_score(left, policy, &task_terms);
        let right_score = baseline_score(right, policy, &task_terms);
        right_score
            .cmp(&left_score)
            .then_with(|| left.source_path.cmp(&right.source_path))
    });
    let mut used = 0;
    let mut selected = Vec::new();
    for note in candidates {
        let tokens = estimate_tokens(&note.body);
        if used + tokens <= case.token_budget {
            used += tokens;
            selected.push(note.note.id.clone());
        }
    }
    Ok(selected)
}

fn baseline_score(
    note: &ParsedNote,
    policy: EvaluationPolicy,
    task_terms: &BTreeSet<String>,
) -> i64 {
    match policy {
        EvaluationPolicy::RawContext => 0,
        EvaluationPolicy::FlatLexical => {
            let note_terms = terms(&format!(
                "{} {}",
                note.note.title.as_deref().unwrap_or_default(),
                note.body
            ));
            i64::try_from(task_terms.intersection(&note_terms).count()).unwrap_or(i64::MAX)
        }
        EvaluationPolicy::VectorFixture => vector_score(note, task_terms),
        EvaluationPolicy::CompiledPack => unreachable!("compiled policy has a dedicated path"),
    }
}

fn vector_score(note: &ParsedNote, task_terms: &BTreeSet<String>) -> i64 {
    let mut query = [0_i64; 64];
    let mut document = [0_i64; 64];
    for term in task_terms {
        add_term(&mut query, term);
    }
    for term in terms(&format!(
        "{} {}",
        note.note.title.as_deref().unwrap_or_default(),
        note.body
    )) {
        add_term(&mut document, &term);
    }
    query.iter().zip(document).map(|(a, b)| a * b).sum()
}

fn add_term(vector: &mut [i64; 64], term: &str) {
    let digest = Sha256::digest(term.as_bytes());
    let index = usize::from(digest[0]) % vector.len();
    vector[index] += if digest[1] & 1 == 0 { 1 } else { -1 };
}

fn terms(text: &str) -> BTreeSet<String> {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|term| term.len() > 1)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        usize_to_f64(numerator) / usize_to_f64(denominator)
    }
}

fn aggregate_slices(runs: &[PolicyRun]) -> Vec<AblationSlice> {
    let mut grouped: BTreeMap<(TaskFamily, EvaluationPolicy), Vec<&PolicyMetrics>> =
        BTreeMap::new();
    for run in runs {
        grouped
            .entry((run.family, run.policy))
            .or_default()
            .push(&run.metrics);
    }
    grouped
        .into_iter()
        .map(|((family, policy), metrics)| {
            let cases = metrics.len();
            AblationSlice {
                family,
                policy,
                cases,
                task_success: mean(metrics.iter().map(|metric| metric.task_success), cases),
                evidence_recall: mean(metrics.iter().map(|metric| metric.evidence_recall), cases),
                unauthorized_adoption: mean(
                    metrics.iter().map(|metric| metric.unauthorized_adoption),
                    cases,
                ),
                stale_adoption: mean(metrics.iter().map(|metric| metric.stale_adoption), cases),
                mean_token_cost: mean(
                    metrics.iter().map(|metric| usize_to_f64(metric.token_cost)),
                    cases,
                ),
            }
        })
        .collect()
}

fn mean(values: impl Iterator<Item = f64>, count: usize) -> f64 {
    values.sum::<f64>() / usize_to_f64(count.max(1))
}

fn usize_to_f64(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

#[must_use]
pub fn render_ablation_markdown(report: &AblationReport) -> String {
    let mut output = format!(
        "# Context Reliability Ablation\n\nCases: {}  \nDigest: `{}`\n\n| Family | Policy | Cases | Success | Recall | Unauthorized | Stale | Mean tokens |\n|---|---|---:|---:|---:|---:|---:|---:|\n",
        report.case_count, report.digest
    );
    for slice in &report.slices {
        writeln!(
            output,
            "| {} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.1} |",
            enum_name(slice.family),
            enum_name(slice.policy),
            slice.cases,
            slice.task_success,
            slice.evidence_recall,
            slice.unauthorized_adoption,
            slice.stale_adoption,
            slice.mean_token_cost,
        )
        .expect("writing to a string succeeds");
    }
    output
}

fn enum_name(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .expect("enum serializes")
        .as_str()
        .expect("enum serializes as a string")
        .replace('_', " ")
}

fn digest_json(value: &impl Serialize) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(serde_json::to_vec(value).expect("report serializes")) {
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

    fn synthetic_vault() -> Vec<ParsedNote> {
        let vault = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault");
        let mut paths: Vec<_> = fs::read_dir(vault)
            .expect("vault exists")
            .map(|entry| entry.expect("entry is readable").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
            .collect();
        paths.sort();
        paths
            .iter()
            .map(|path| {
                parse_note(
                    path.display().to_string(),
                    &fs::read_to_string(path).expect("fixture is readable"),
                )
                .expect("fixture parses")
            })
            .collect()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 21, 12, 0, 0)
            .single()
            .expect("valid timestamp")
    }

    #[test]
    fn ablation_covers_two_cases_per_family_and_every_policy_slice() {
        let cases = build_default_eval_cases(&synthetic_vault()).expect("cases build");
        let report = run_ablation(&cases, now()).expect("ablation runs");

        assert_eq!(cases.len(), 10);
        for family in [
            TaskFamily::Retrieval,
            TaskFamily::Adherence,
            TaskFamily::Generalization,
            TaskFamily::Hygiene,
            TaskFamily::Recovery,
        ] {
            assert_eq!(cases.iter().filter(|case| case.family == family).count(), 2);
            for policy in [
                EvaluationPolicy::RawContext,
                EvaluationPolicy::FlatLexical,
                EvaluationPolicy::VectorFixture,
                EvaluationPolicy::CompiledPack,
            ] {
                assert!(report.slices.iter().any(|slice| {
                    slice.family == family && slice.policy == policy && slice.cases == 2
                }));
            }
        }
        assert_eq!(report.runs.len(), 40);
        assert_eq!(report.slices.len(), 20);
        assert_eq!(report.digest.len(), 64);
    }

    #[test]
    fn compiled_policy_has_no_unauthorized_or_stale_adoption() {
        let cases = build_default_eval_cases(&synthetic_vault()).expect("cases build");
        let report = run_ablation(&cases, now()).expect("ablation runs");
        let compiled: Vec<_> = report
            .runs
            .iter()
            .filter(|run| run.policy == EvaluationPolicy::CompiledPack)
            .collect();

        assert!(
            compiled
                .iter()
                .all(|run| run.metrics.unauthorized_adoption == 0.0)
        );
        assert!(compiled.iter().all(|run| run.metrics.stale_adoption == 0.0));
        assert!(compiled.iter().all(|run| run.metrics.token_cost <= 80));
    }

    #[test]
    fn report_is_deterministic_and_renders_all_slices() {
        let cases = build_default_eval_cases(&synthetic_vault()).expect("cases build");
        let first = run_ablation(&cases, now()).expect("ablation runs");
        let second = run_ablation(&cases, now()).expect("ablation repeats");

        assert_eq!(first, second);
        let markdown = render_ablation_markdown(&first);
        assert_eq!(
            markdown
                .lines()
                .filter(|line| line.starts_with('|'))
                .count(),
            22
        );
        assert!(markdown.contains("retrieval"));
        assert!(markdown.contains("compiled pack"));
    }
}
