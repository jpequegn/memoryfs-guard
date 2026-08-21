use std::fmt::Write as _;

use memoryfs_core::{ContextPack, LintReceipt, ParsedNote};

pub fn parsed_markdown(notes: &[ParsedNote]) -> String {
    let mut output =
        "# Parsed Notes\n\n| ID | Type | Trust | Sensitivity | Path |\n|---|---|---|---|---|\n"
            .to_owned();
    for note in notes {
        writeln!(
            output,
            "| {} | {:?} | {:?} | {:?} | `{}` |",
            escape(&note.note.id),
            note.note.note_type,
            note.note.trust,
            note.note.sensitivity,
            escape(&note.source_path),
        )
        .expect("writing to a string succeeds");
    }
    output
}

pub fn lint_markdown(receipt: &LintReceipt) -> String {
    let active = receipt
        .findings
        .iter()
        .filter(|finding| !finding.suppressed)
        .count();
    let mut output = format!(
        "# Memory Lint Receipt\n\nActive findings: {active}  \nDigest: `{}`\n\n| Severity | Rule | Note | Location | Reason |\n|---|---|---|---|---|\n",
        receipt.digest
    );
    for finding in &receipt.findings {
        writeln!(
            output,
            "| {:?}{} | `{}` | `{}` | `{}:{}` | {} |",
            finding.severity,
            if finding.suppressed {
                " (suppressed)"
            } else {
                ""
            },
            finding.code,
            escape(&finding.note_id),
            escape(&finding.location.file),
            finding.location.line,
            escape(&finding.message),
        )
        .expect("writing to a string succeeds");
    }
    output
}

pub fn pack_markdown(pack: &ContextPack) -> String {
    let mut output = format!(
        "# Context Pack\n\nTask: {}  \nTokens: {}  \nDigest: `{}`\n\n## Evidence\n\n",
        escape(&pack.task),
        pack.total_estimated_tokens,
        pack.receipt.value
    );
    for excerpt in &pack.excerpts {
        writeln!(
            output,
            "### {}\n\nSource: `{}`  \nScore: {}  \nEstimated tokens: {}\n\n{}\n",
            escape(excerpt.title.as_deref().unwrap_or(&excerpt.note_id)),
            escape(&excerpt.provenance_source),
            excerpt.score,
            excerpt.estimated_tokens,
            excerpt.text,
        )
        .expect("writing to a string succeeds");
    }
    output.push_str("## Candidate Decisions\n\n| Decision | Note | Temporal | Conflict | Authority | Reason |\n|---|---|---|---|---|---|\n");
    for candidate in &pack.candidates {
        writeln!(
            output,
            "| {:?} | `{}` | {:?} | {:?} | {:?} | {} |",
            candidate.decision,
            escape(&candidate.note_id),
            candidate.temporal_status,
            candidate.conflict_status,
            candidate.authority_status,
            escape(&candidate.reason),
        )
        .expect("writing to a string succeeds");
    }
    output
}

fn escape(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}
