#![doc = "Pure memory parsing, validation, and context compilation."]

pub mod compiler;
pub mod lint;
pub mod migration;
pub mod model;
pub mod parser;

pub use compiler::{
    AuthorityStatus, CallerScope, CandidateDecision, CandidateReceipt, CompileError,
    CompileRequest, ConflictStatus, ContextExcerpt, ContextPack, DigestReceipt, RankingPolicy,
    TemporalStatus, compile_context,
};
pub use lint::{LintContext, LintFinding, LintReceipt, RULE_CATALOG, RuleInfo, lint_vault};
pub use migration::{CURRENT_SCHEMA_VERSION, migrate_front_matter};
pub use model::{
    Attachment, Diagnostic, Heading, LinkKind, MemoryLink, Note, NoteType, ParsedNote, Provenance,
    Scope, Sensitivity, Severity, SourceLocation, Suppression, TrustState, Validity,
};
pub use parser::{ParseError, parse_note};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[must_use]
pub const fn version() -> &'static str {
    VERSION
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), "0.1.0");
    }
}
