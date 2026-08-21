mod commands;
mod report;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "memoryfs", about = "Compile and lint Git-backed agent memory")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the workspace version.
    Version,
    /// Create a minimal vault and configuration.
    Init {
        #[arg(default_value = ".")]
        directory: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Parse one note or every Markdown note in a vault.
    Parse {
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },
    /// Lint a vault and emit a reproducible receipt.
    Lint {
        vault: PathBuf,
        #[arg(long)]
        now: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },
    /// Compile an authorized context pack for a task.
    Compile {
        vault: PathBuf,
        #[arg(long)]
        task: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        agent: Option<String>,
        #[arg(long)]
        environment: Option<String>,
        #[arg(long, value_enum, default_value_t = Clearance::Internal)]
        clearance: Clearance,
        #[arg(long, default_value_t = 1500)]
        budget: usize,
        #[arg(long, value_enum, default_value_t = Ranking::Lexical)]
        ranking: Ranking,
        #[arg(long = "require")]
        required_evidence: Vec<String>,
        #[arg(long)]
        now: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },
    /// Compare two committed vault states semantically.
    Diff {
        before: String,
        after: String,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "vault")]
        vault_root: PathBuf,
    },
    /// Generate a detached patch proposal without changing the vault.
    Propose {
        revision: String,
        path: String,
        proposed_file: PathBuf,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "vault")]
        vault_root: PathBuf,
    },
    /// Rebuild a deterministic index from a commit.
    Index {
        #[arg(default_value = "HEAD")]
        revision: String,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long, default_value = "vault")]
        vault_root: PathBuf,
    },
    /// Summarize whether a vault needs attention.
    Status {
        vault: PathBuf,
        #[arg(long)]
        now: Option<String>,
        #[arg(long)]
        strict: bool,
    },
    /// Parse, lint, compile, evaluate, and report the synthetic vault.
    Demo {
        #[arg(long, default_value = "fixtures/vault")]
        vault: PathBuf,
        #[arg(long, default_value = "artifacts/demo")]
        output: PathBuf,
        #[arg(long, default_value = "2026-08-21T12:00:00Z")]
        now: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Markdown,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Ranking {
    Metadata,
    Lexical,
    Vector,
    Links,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Clearance {
    Public,
    Internal,
    Confidential,
    Restricted,
}

fn main() -> Result<()> {
    commands::run(Cli::parse().command)
}
