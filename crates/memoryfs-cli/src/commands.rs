use std::{collections::BTreeSet, fs, path::Path, time::SystemTime};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, SecondsFormat, Utc};
use memoryfs_core::{
    CallerScope, CompileRequest, LintContext, LintReceipt, ParsedNote, RankingPolicy, Sensitivity,
    Severity, build_default_eval_cases, compile_context, lint_vault, parse_note,
    render_ablation_markdown, run_ablation,
};
use memoryfs_git::GitVault;
use serde::Serialize;
use walkdir::WalkDir;

use crate::{Clearance, Command, OutputFormat, Ranking, report};

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Version => println!("{}", memoryfs_core::version()),
        Command::Init { directory, force } => init(&directory, force)?,
        Command::Parse { path, format } => {
            let notes = load_notes(&path)?;
            emit(format, &notes, &report::parsed_markdown(&notes))?;
        }
        Command::Lint { vault, now, format } => {
            let notes = load_notes(&vault)?;
            let receipt = lint_vault(&notes, &lint_context(parse_time(now.as_deref())?));
            emit(format, &receipt, &report::lint_markdown(&receipt))?;
        }
        Command::Compile {
            vault,
            task,
            project,
            agent,
            environment,
            clearance,
            budget,
            ranking,
            required_evidence,
            now,
            format,
        } => {
            let notes = load_notes(&vault)?;
            let request = compile_request(
                task,
                caller_scope(project, agent, environment, clearance),
                budget,
                ranking,
                required_evidence,
                "cli-v1",
            );
            let pack = compile_context(&notes, &request, parse_time(now.as_deref())?)?;
            emit(format, &pack, &report::pack_markdown(&pack))?;
        }
        Command::Diff {
            before,
            after,
            repo,
            vault_root,
        } => {
            let vault = GitVault::open(repo, vault_root)?;
            print_json(&vault.semantic_diff(&before, &after)?)?;
        }
        Command::Propose {
            revision,
            path,
            proposed_file,
            repo,
            vault_root,
        } => {
            let source = fs::read_to_string(&proposed_file)
                .with_context(|| format!("cannot read {}", proposed_file.display()))?;
            let vault = GitVault::open(repo, vault_root)?;
            print_json(&vault.propose(&revision, &path, &source)?)?;
        }
        Command::Index {
            revision,
            repo,
            vault_root,
        } => {
            let vault = GitVault::open(repo, vault_root)?;
            print_json(&vault.rebuild_index(&revision)?)?;
        }
        Command::Status { vault, now, strict } => {
            status(&vault, parse_time(now.as_deref())?, strict)?;
        }
        Command::Demo { vault, output, now } => demo(&vault, &output, parse_time(Some(&now))?)?,
    }
    Ok(())
}

fn init(directory: &Path, force: bool) -> Result<()> {
    let vault = directory.join("vault");
    let config = directory.join("memoryfs.toml");
    let welcome = vault.join("welcome.md");
    if !force && (config.exists() || welcome.exists()) {
        bail!("memoryfs files already exist; pass --force to replace them");
    }
    fs::create_dir_all(&vault)?;
    let observed_at =
        DateTime::<Utc>::from(SystemTime::now()).to_rfc3339_opts(SecondsFormat::Secs, true);
    fs::write(
        &config,
        "schema_version = 1\nvault = \"vault\"\npolicy_version = \"default-v1\"\n",
    )?;
    fs::write(
        &welcome,
        format!(
            "---\nschema_version: 1\nid: welcome\ntitle: Welcome memory\ntype: fact\nscope:\n  projects: [memoryfs]\n  agents: []\n  environments: [development]\nprovenance:\n  source: user\n  observed_at: \"{observed_at}\"\ntrust: reviewed\nsensitivity: internal\n---\n\n# Welcome memory\n\nReplace this note with reviewed, source-backed memory.\n"
        ),
    )?;
    println!("Initialized {}", directory.display());
    Ok(())
}

fn status(vault: &Path, now: DateTime<Utc>, strict: bool) -> Result<()> {
    let notes = load_notes(vault)?;
    let receipt = lint_vault(&notes, &lint_context(now));
    let errors = active_count(&receipt, Severity::Error);
    let warnings = active_count(&receipt, Severity::Warning);
    let state = if errors > 0 {
        "ATTENTION"
    } else if warnings > 0 {
        "REVIEW"
    } else {
        "HEALTHY"
    };
    println!("{state}");
    println!("notes: {}", notes.len());
    println!("errors: {errors}");
    println!("warnings: {warnings}");
    println!("receipt: {}", receipt.digest);
    if strict && errors > 0 {
        bail!("vault has {errors} active error findings");
    }
    Ok(())
}

fn demo(vault: &Path, output: &Path, now: DateTime<Utc>) -> Result<()> {
    let notes = load_notes(vault)?;
    let lint = lint_vault(&notes, &lint_context(now));
    let request = compile_request(
        "Prepare a safe production deployment".to_owned(),
        caller_scope(
            "memoryfs".to_owned(),
            Some("release-agent".to_owned()),
            Some("production".to_owned()),
            Clearance::Internal,
        ),
        1500,
        Ranking::Links,
        Vec::new(),
        "demo-v1",
    );
    let pack = compile_context(&notes, &request, now)?;
    let cases = build_default_eval_cases(&notes)?;
    let ablation = run_ablation(&cases, now)?;
    let active_errors = active_count(&lint, Severity::Error);
    let included = pack.excerpts.len();
    let excluded = pack.candidates.len() - included;

    fs::create_dir_all(output)?;
    write_json(&output.join("parsed.json"), &notes)?;
    write_text(&output.join("parsed.md"), &report::parsed_markdown(&notes))?;
    write_json(&output.join("lint.json"), &lint)?;
    write_text(&output.join("lint.md"), &report::lint_markdown(&lint))?;
    write_json(&output.join("context-pack.json"), &pack)?;
    write_text(
        &output.join("context-pack.md"),
        &report::pack_markdown(&pack),
    )?;
    write_json(&output.join("ablation.json"), &ablation)?;
    write_text(
        &output.join("ablation.md"),
        &render_ablation_markdown(&ablation),
    )?;
    let summary = DemoSummary {
        parsed_notes: notes.len(),
        active_errors,
        included,
        excluded,
        pack_digest: &pack.receipt.value,
        ablation_digest: &ablation.digest,
    };
    write_json(&output.join("summary.json"), &summary)?;
    write_text(
        &output.join("summary.md"),
        &format!(
            "# MemoryFS Guard Demo\n\n- Parsed notes: {}\n- Active lint errors: {}\n- Included candidates: {}\n- Excluded candidates: {}\n- Pack digest: `{}`\n- Ablation digest: `{}`\n",
            notes.len(),
            active_errors,
            included,
            excluded,
            pack.receipt.value,
            ablation.digest
        ),
    )?;
    println!("Demo complete: {}", output.display());
    println!(
        "parsed={} errors={active_errors} included={included} excluded={excluded}",
        notes.len()
    );
    Ok(())
}

#[derive(Serialize)]
struct DemoSummary<'a> {
    parsed_notes: usize,
    active_errors: usize,
    included: usize,
    excluded: usize,
    pack_digest: &'a str,
    ablation_digest: &'a str,
}

fn compile_request(
    task: String,
    caller: CallerScope,
    budget: usize,
    ranking: Ranking,
    required_evidence: Vec<String>,
    policy_version: &str,
) -> CompileRequest {
    CompileRequest {
        task,
        caller,
        allowed_note_types: BTreeSet::new(),
        token_budget: budget,
        required_evidence: required_evidence.into_iter().collect(),
        policy_version: policy_version.to_owned(),
        ranking: ranking_policy(ranking),
        stale_after_days: 180,
        max_link_depth: 1,
    }
}

fn load_notes(path: &Path) -> Result<Vec<ParsedNote>> {
    let mut paths = if path.is_file() {
        vec![path.to_owned()]
    } else {
        WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(walkdir::DirEntry::into_path)
            .filter(|entry| {
                entry.is_file()
                    && entry
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            })
            .collect()
    };
    paths.sort();
    if paths.is_empty() {
        bail!("no Markdown notes found at {}", path.display());
    }
    paths
        .into_iter()
        .map(|note_path| {
            let source = fs::read_to_string(&note_path)
                .with_context(|| format!("cannot read {}", note_path.display()))?;
            parse_note(note_path.display().to_string(), &source).map_err(anyhow::Error::from)
        })
        .collect()
}

fn parse_time(value: Option<&str>) -> Result<DateTime<Utc>> {
    value.map_or_else(
        || Ok(DateTime::<Utc>::from(SystemTime::now())),
        |timestamp| {
            DateTime::parse_from_rfc3339(timestamp)
                .map(|parsed| parsed.with_timezone(&Utc))
                .with_context(|| format!("invalid RFC 3339 timestamp: {timestamp}"))
        },
    )
}

fn lint_context(now: DateTime<Utc>) -> LintContext {
    LintContext::new(now)
}

fn active_count(receipt: &LintReceipt, severity: Severity) -> usize {
    receipt
        .findings
        .iter()
        .filter(|finding| !finding.suppressed && finding.severity == severity)
        .count()
}

fn emit<T: Serialize>(format: OutputFormat, value: &T, markdown: &str) -> Result<()> {
    match format {
        OutputFormat::Json => print_json(value),
        OutputFormat::Markdown => {
            print!("{markdown}");
            Ok(())
        }
    }
}

fn print_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    write_text(path, &format!("{}\n", serde_json::to_string_pretty(value)?))
}

fn write_text(path: &Path, value: &str) -> Result<()> {
    fs::write(path, value).with_context(|| format!("cannot write {}", path.display()))
}

const fn sensitivity(clearance: Clearance) -> Sensitivity {
    match clearance {
        Clearance::Public => Sensitivity::Public,
        Clearance::Internal => Sensitivity::Internal,
        Clearance::Confidential => Sensitivity::Confidential,
        Clearance::Restricted => Sensitivity::Restricted,
    }
}

fn caller_scope(
    project: String,
    agent: Option<String>,
    environment: Option<String>,
    clearance: Clearance,
) -> CallerScope {
    CallerScope {
        project,
        agent,
        environment,
        max_sensitivity: sensitivity(clearance),
    }
}

const fn ranking_policy(ranking: Ranking) -> RankingPolicy {
    match ranking {
        Ranking::Metadata => RankingPolicy::Metadata,
        Ranking::Lexical => RankingPolicy::Lexical,
        Ranking::Vector => RankingPolicy::VectorFixture,
        Ranking::Links => RankingPolicy::BoundedLinks,
    }
}
