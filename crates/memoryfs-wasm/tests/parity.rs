use std::{collections::BTreeSet, fs, path::PathBuf};

use chrono::{TimeZone, Utc};
use memoryfs_core::{
    CallerScope, CompileRequest, LintContext, RankingPolicy, Sensitivity, compile_context,
    lint_vault, parse_note,
};
use memoryfs_wasm::{compile_pack_json, graph_vault_json, lint_vault_json, parse_note_json};
use serde_json::{Value, json};

fn fixture(name: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/vault")
            .join(name),
    )
    .expect("fixture is readable")
}

#[test]
fn parse_export_matches_native_core_output() {
    let source = fixture("01-current-policy.md");
    let native = serde_json::to_string(
        &parse_note("01-current-policy.md", &source).expect("native parser succeeds"),
    )
    .expect("native output serializes");
    let exported = parse_note_json("01-current-policy.md", &source).expect("export succeeds");

    assert_eq!(exported, native);
}

#[test]
fn lint_export_matches_native_core_output() {
    let files = json!([
        {"path": "01-current-policy.md", "source": fixture("01-current-policy.md")},
        {"path": "08-secret-canary.md", "source": fixture("08-secret-canary.md")}
    ]);
    let notes = files
        .as_array()
        .expect("array")
        .iter()
        .map(|file| {
            parse_note(
                file["path"].as_str().expect("path"),
                file["source"].as_str().expect("source"),
            )
            .expect("native parser succeeds")
        })
        .collect::<Vec<_>>();
    let now = Utc
        .with_ymd_and_hms(2026, 8, 21, 12, 0, 0)
        .single()
        .expect("valid timestamp");
    let native = serde_json::to_string(&lint_vault(&notes, &LintContext::new(now)))
        .expect("native output serializes");
    let exported =
        lint_vault_json(&files.to_string(), "2026-08-21T12:00:00Z").expect("export succeeds");

    assert_eq!(exported, native);
}

#[test]
fn graph_export_is_sorted_and_source_backed() {
    let files = json!([
        {"path": "09-attachment-and-block.md", "source": fixture("09-attachment-and-block.md")},
        {"path": "01-current-policy.md", "source": fixture("01-current-policy.md")}
    ]);
    let graph: Value =
        serde_json::from_str(&graph_vault_json(&files.to_string()).expect("graph export succeeds"))
            .expect("graph response is JSON");

    assert_eq!(graph["nodes"][0]["id"], "policy-current");
    assert!(
        graph["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .all(|edge| edge["line"].as_u64().is_some_and(|line| line > 0))
    );
}

#[test]
fn compile_export_matches_native_core_output() {
    let files = json!([
        {"path": "09-attachment-and-block.md", "source": fixture("09-attachment-and-block.md")},
        {"path": "10-unknown-metadata.md", "source": fixture("10-unknown-metadata.md")}
    ]);
    let notes = files
        .as_array()
        .expect("array")
        .iter()
        .map(|file| {
            parse_note(
                file["path"].as_str().expect("path"),
                file["source"].as_str().expect("source"),
            )
            .expect("native parser succeeds")
        })
        .collect::<Vec<_>>();
    let request = CompileRequest {
        task: "prepare a production deployment".to_owned(),
        caller: CallerScope {
            project: "memoryfs".to_owned(),
            agent: Some("release-agent".to_owned()),
            environment: Some("production".to_owned()),
            max_sensitivity: Sensitivity::Internal,
        },
        allowed_note_types: BTreeSet::new(),
        token_budget: 1500,
        required_evidence: BTreeSet::new(),
        policy_version: "parity-v1".to_owned(),
        ranking: RankingPolicy::Lexical,
        stale_after_days: 180,
        max_link_depth: 1,
    };
    let now = Utc
        .with_ymd_and_hms(2026, 8, 21, 12, 0, 0)
        .single()
        .expect("valid timestamp");
    let native = serde_json::to_string(
        &compile_context(&notes, &request, now).expect("native compiler succeeds"),
    )
    .expect("native output serializes");
    let exported = compile_pack_json(
        &files.to_string(),
        &serde_json::to_string(&request).expect("request serializes"),
        "2026-08-21T12:00:00Z",
    )
    .expect("export succeeds");

    assert_eq!(exported, native);
}
