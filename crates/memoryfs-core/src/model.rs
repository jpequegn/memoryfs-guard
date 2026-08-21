use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::migration::CURRENT_SCHEMA_VERSION;

fn current_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteType {
    Fact,
    Instruction,
    Decision,
    Preference,
    Observation,
    Policy,
    Procedure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    Trusted,
    Reviewed,
    Untrusted,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub environments: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: String,
    #[serde(default)]
    pub author: Option<String>,
    pub observed_at: DateTime<Utc>,
    #[serde(default)]
    pub source_revision: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validity {
    #[serde(default)]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(default)]
    pub valid_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suppression {
    pub code: String,
    pub reason: String,
    pub until: DateTime<Utc>,
    #[serde(default)]
    pub approved_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    #[serde(default = "current_schema_version")]
    pub schema_version: u32,
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub note_type: NoteType,
    pub scope: Scope,
    pub provenance: Provenance,
    pub trust: TrustState,
    pub sensitivity: Sensitivity,
    #[serde(default)]
    pub validity: Validity,
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
    #[serde(default)]
    pub retention_days: Option<u32>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub suppressions: Vec<Suppression>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    Markdown,
    Wiki,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryLink {
    pub target: String,
    pub label: Option<String>,
    pub kind: LinkKind,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub target: String,
    pub alt_text: Option<String>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedNote {
    pub note: Note,
    pub body: String,
    pub headings: Vec<Heading>,
    pub links: Vec<MemoryLink>,
    pub attachments: Vec<Attachment>,
    pub diagnostics: Vec<Diagnostic>,
    pub source_path: String,
    pub source: String,
}
