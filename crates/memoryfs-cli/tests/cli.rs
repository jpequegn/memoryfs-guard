use std::{fs, path::PathBuf};

use tempfile::TempDir;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn parse_and_compile_emit_machine_readable_json() {
    let root = repository_root();
    let mut parse = assert_cmd::cargo::cargo_bin_cmd!("memoryfs");
    parse
        .args(["parse", "fixtures/vault/01-current-policy.md"])
        .current_dir(&root)
        .assert()
        .success()
        .stdout(predicates::str::contains("policy-current"));

    let mut compile = assert_cmd::cargo::cargo_bin_cmd!("memoryfs");
    compile
        .args([
            "compile",
            "fixtures/vault",
            "--task",
            "prepare a deployment",
            "--project",
            "memoryfs",
            "--agent",
            "release-agent",
            "--environment",
            "production",
            "--now",
            "2026-08-21T12:00:00Z",
        ])
        .current_dir(root)
        .assert()
        .success()
        .stdout(predicates::str::contains("candidate"));
}

#[test]
fn status_reports_attention_and_strict_mode_fails_on_active_errors() {
    let root = repository_root();
    let mut status = assert_cmd::cargo::cargo_bin_cmd!("memoryfs");
    status
        .args(["status", "fixtures/vault", "--now", "2026-08-21T12:00:00Z"])
        .current_dir(&root)
        .assert()
        .success()
        .stdout(predicates::str::contains("ATTENTION"))
        .stdout(predicates::str::contains("notes: 10"));

    let mut strict = assert_cmd::cargo::cargo_bin_cmd!("memoryfs");
    strict
        .args([
            "status",
            "fixtures/vault",
            "--now",
            "2026-08-21T12:00:00Z",
            "--strict",
        ])
        .current_dir(root)
        .assert()
        .failure()
        .stderr(predicates::str::contains("active error findings"));
}

#[test]
fn demo_generates_complete_reports_from_a_clean_output_directory() {
    let root = repository_root();
    let output = TempDir::new().expect("temporary output directory");
    let mut demo = assert_cmd::cargo::cargo_bin_cmd!("memoryfs");
    demo.args([
        "demo",
        "--vault",
        "fixtures/vault",
        "--output",
        output.path().to_str().expect("UTF-8 path"),
    ])
    .current_dir(root)
    .assert()
    .success()
    .stdout(predicates::str::contains("parsed=10"));

    for artifact in [
        "parsed.json",
        "parsed.md",
        "lint.json",
        "lint.md",
        "context-pack.json",
        "context-pack.md",
        "ablation.json",
        "ablation.md",
        "summary.json",
        "summary.md",
    ] {
        let path = output.path().join(artifact);
        assert!(path.is_file(), "missing {}", path.display());
        assert!(fs::metadata(path).expect("artifact metadata").len() > 0);
    }
}

#[test]
fn init_creates_a_parseable_vault_and_refuses_accidental_overwrite() {
    let directory = TempDir::new().expect("temporary directory");
    let destination = directory.path().join("workspace");
    let mut init = assert_cmd::cargo::cargo_bin_cmd!("memoryfs");
    init.arg("init").arg(&destination).assert().success();
    assert!(destination.join("memoryfs.toml").is_file());
    assert!(destination.join("vault/welcome.md").is_file());

    let mut parse = assert_cmd::cargo::cargo_bin_cmd!("memoryfs");
    parse
        .arg("parse")
        .arg(destination.join("vault"))
        .assert()
        .success()
        .stdout(predicates::str::contains("welcome"));

    let mut overwrite = assert_cmd::cargo::cargo_bin_cmd!("memoryfs");
    overwrite
        .arg("init")
        .arg(destination)
        .assert()
        .failure()
        .stderr(predicates::str::contains("--force"));
}
