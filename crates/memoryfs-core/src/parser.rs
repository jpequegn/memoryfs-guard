use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};
use serde_json::Value;
use thiserror::Error;

use crate::{
    Attachment, Diagnostic, Heading, LinkKind, MemoryLink, Note, ParsedNote, Severity,
    SourceLocation, migrate_front_matter,
};

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("{path}: missing YAML front matter")]
    MissingFrontMatter { path: String },
    #[error("{path}: front matter is not terminated")]
    UnterminatedFrontMatter { path: String },
    #[error("{path}: invalid YAML front matter: {source}")]
    InvalidYaml {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("{path}: invalid note contract: {source}")]
    InvalidContract {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path}: front matter migration failed: {message}")]
    Migration { path: String, message: String },
    #[error("{path}: {resource} limit exceeded ({actual} > {limit})")]
    ResourceLimit {
        path: String,
        resource: &'static str,
        limit: usize,
        actual: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParserLimits {
    pub max_file_bytes: usize,
    pub max_front_matter_bytes: usize,
    pub max_body_bytes: usize,
    pub max_links: usize,
    pub max_attachments: usize,
    pub max_heading_depth: u8,
}

impl Default for ParserLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 1024 * 1024,
            max_front_matter_bytes: 64 * 1024,
            max_body_bytes: 960 * 1024,
            max_links: 512,
            max_attachments: 128,
            max_heading_depth: 6,
        }
    }
}

/// Parses one versioned Markdown memory note without changing its source text.
///
/// # Errors
///
/// Returns an error when front matter is missing, malformed, unsupported, or does
/// not satisfy the note contract.
pub fn parse_note(path: impl Into<String>, source: &str) -> Result<ParsedNote, ParseError> {
    parse_note_with_limits(path, source, &ParserLimits::default())
}

/// Parses one memory note while enforcing caller-selected resource limits.
///
/// # Errors
///
/// Returns an error when a resource limit is exceeded or the note is invalid.
pub fn parse_note_with_limits(
    path: impl Into<String>,
    source: &str,
    limits: &ParserLimits,
) -> Result<ParsedNote, ParseError> {
    let path = path.into();
    enforce_limit(&path, "file bytes", limits.max_file_bytes, source.len())?;
    let (front_matter, body, body_offset) = split_front_matter(&path, source)?;
    enforce_limit(
        &path,
        "front matter bytes",
        limits.max_front_matter_bytes,
        front_matter.len(),
    )?;
    enforce_limit(&path, "body bytes", limits.max_body_bytes, body.len())?;
    let yaml: Value =
        serde_yaml::from_str(front_matter).map_err(|source| ParseError::InvalidYaml {
            path: path.clone(),
            source,
        })?;
    let source_version = yaml
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let migrated = migrate_front_matter(yaml).map_err(|error| ParseError::Migration {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let note: Note =
        serde_json::from_value(migrated).map_err(|source| ParseError::InvalidContract {
            path: path.clone(),
            source,
        })?;

    let (headings, mut links, mut attachments) = parse_markdown(&path, source, body, body_offset);
    parse_wiki_constructs(
        &path,
        source,
        body,
        body_offset,
        &mut links,
        &mut attachments,
    );
    enforce_limit(&path, "links", limits.max_links, links.len())?;
    enforce_limit(
        &path,
        "attachments",
        limits.max_attachments,
        attachments.len(),
    )?;
    if let Some(heading) = headings
        .iter()
        .find(|heading| heading.level > limits.max_heading_depth)
    {
        return Err(ParseError::ResourceLimit {
            path,
            resource: "heading depth",
            limit: usize::from(limits.max_heading_depth),
            actual: usize::from(heading.level),
        });
    }
    links.sort_by_key(|link| (link.location.line, link.location.column));
    attachments.sort_by_key(|attachment| (attachment.location.line, attachment.location.column));

    let diagnostics = (source_version == 0)
        .then(|| Diagnostic {
            code: "front_matter_migrated".to_owned(),
            severity: Severity::Info,
            message: "legacy front matter was migrated to schema version 1".to_owned(),
            location: SourceLocation {
                file: path.clone(),
                line: 1,
                column: 1,
            },
        })
        .into_iter()
        .collect();

    Ok(ParsedNote {
        note,
        body: body.to_owned(),
        headings,
        links,
        attachments,
        diagnostics,
        source_path: path,
        source: source.to_owned(),
    })
}

fn enforce_limit(
    path: &str,
    resource: &'static str,
    limit: usize,
    actual: usize,
) -> Result<(), ParseError> {
    if actual > limit {
        return Err(ParseError::ResourceLimit {
            path: path.to_owned(),
            resource,
            limit,
            actual,
        });
    }
    Ok(())
}

fn split_front_matter<'a>(
    path: &str,
    source: &'a str,
) -> Result<(&'a str, &'a str, usize), ParseError> {
    let mut lines = source.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return Err(ParseError::MissingFrontMatter {
            path: path.to_owned(),
        });
    };
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return Err(ParseError::MissingFrontMatter {
            path: path.to_owned(),
        });
    }

    let front_start = first.len();
    let mut offset = front_start;
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let body_offset = offset + line.len();
            return Ok((
                &source[front_start..offset],
                &source[body_offset..],
                body_offset,
            ));
        }
        offset += line.len();
    }

    Err(ParseError::UnterminatedFrontMatter {
        path: path.to_owned(),
    })
}

fn parse_markdown(
    path: &str,
    source: &str,
    body: &str,
    body_offset: usize,
) -> (Vec<Heading>, Vec<MemoryLink>, Vec<Attachment>) {
    let parser = Parser::new_ext(body, Options::ENABLE_HEADING_ATTRIBUTES).into_offset_iter();
    let mut headings = Vec::new();
    let mut links = Vec::new();
    let mut attachments = Vec::new();
    let mut active_heading: Option<(u8, usize, String)> = None;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                active_heading = Some((heading_level(level), range.start, String::new()));
            }
            Event::End(pulldown_cmark::TagEnd::Heading(_)) => {
                if let Some((level, offset, text)) = active_heading.take() {
                    headings.push(Heading {
                        level,
                        text,
                        location: location(path, source, body_offset + offset),
                    });
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, heading_text)) = active_heading.as_mut() {
                    heading_text.push_str(&text);
                }
            }
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                let target = dest_url.into_string();
                let kind = if target.contains("#^") {
                    LinkKind::Block
                } else {
                    LinkKind::Markdown
                };
                links.push(MemoryLink {
                    target,
                    label: (!title.is_empty()).then(|| title.into_string()),
                    kind,
                    location: location(path, source, body_offset + range.start),
                });
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => attachments.push(Attachment {
                target: dest_url.into_string(),
                alt_text: (!title.is_empty()).then(|| title.into_string()),
                location: location(path, source, body_offset + range.start),
            }),
            _ => {}
        }
    }

    (headings, links, attachments)
}

fn parse_wiki_constructs(
    path: &str,
    source: &str,
    body: &str,
    body_offset: usize,
    links: &mut Vec<MemoryLink>,
    attachments: &mut Vec<Attachment>,
) {
    let mut cursor = 0;
    while let Some(relative_start) = body[cursor..].find("[[") {
        let start = cursor + relative_start;
        let content_start = start + 2;
        let Some(relative_end) = body[content_start..].find("]]") else {
            break;
        };
        let end = content_start + relative_end;
        let content = &body[content_start..end];
        let (target, label) = content
            .split_once('|')
            .map_or((content, None), |(target, label)| (target, Some(label)));
        let is_attachment = start > 0 && body.as_bytes()[start - 1] == b'!';
        let construct_start = if is_attachment { start - 1 } else { start };
        let construct_location = location(path, source, body_offset + construct_start);

        if is_attachment {
            attachments.push(Attachment {
                target: target.to_owned(),
                alt_text: label.map(str::to_owned),
                location: construct_location,
            });
        } else {
            links.push(MemoryLink {
                target: target.to_owned(),
                label: label.map(str::to_owned),
                kind: if target.contains("#^") {
                    LinkKind::Block
                } else {
                    LinkKind::Wiki
                },
                location: construct_location,
            });
        }
        cursor = end + 2;
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn location(path: &str, source: &str, offset: usize) -> SourceLocation {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len() + 1, |(_, tail)| tail.len() + 1);
    SourceLocation {
        file: path.to_owned(),
        line,
        column,
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, fs, path::PathBuf};

    use proptest::prelude::*;
    use serde_json::{Value, json};

    use super::*;

    fn fixture(name: &str) -> String {
        fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/vault")
                .join(name),
        )
        .expect("fixture is readable")
    }

    #[test]
    fn parses_golden_markdown_locations_and_preserves_source() {
        let source = fixture("01-current-policy.md");
        let parsed = parse_note("01-current-policy.md", &source).expect("fixture parses");

        assert_eq!(parsed.source, source);
        assert_eq!(parsed.note.id, "policy-current");
        assert_eq!(parsed.headings[0].text, "Current deployment policy");
        assert_eq!(parsed.headings[0].location.line, 23);
        assert_eq!(parsed.links[0].target, "procedure-deploy");
        assert_eq!(parsed.links[0].location.line, 26);
        assert_eq!(parsed.attachments[0].target, "architecture.png");
        assert_eq!(parsed.attachments[0].location.line, 28);
    }

    #[test]
    fn parses_every_synthetic_vault_note_deterministically() {
        let vault = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault");
        let mut paths: Vec<_> = fs::read_dir(vault)
            .expect("vault exists")
            .map(|entry| entry.expect("entry is readable").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .collect();
        paths.sort();
        assert_eq!(paths.len(), 10);

        for path in paths {
            let source = fs::read_to_string(&path).expect("note is readable");
            let first = parse_note(path.display().to_string(), &source).expect("note parses");
            let second = parse_note(path.display().to_string(), &source).expect("note parses");
            assert_eq!(first, second);
            assert_eq!(first.source, source);
        }
    }

    #[test]
    fn survives_json_round_trip_with_unknown_metadata() {
        let parsed = parse_note("10-unknown-metadata.md", &fixture("10-unknown-metadata.md"))
            .expect("fixture parses");
        let encoded = serde_json::to_value(&parsed.note).expect("serializes");
        let decoded: Note = serde_json::from_value(encoded).expect("deserializes");

        assert_eq!(decoded.id, parsed.note.id);
        assert_eq!(decoded.extra["future_extension"]["mode"], "experimental");
        assert_eq!(decoded.scope.extra["teams"], json!(["platform"]));
        assert_eq!(decoded.provenance.extra["collector"], "fixture-generator");
    }

    #[test]
    fn reports_migration_at_a_precise_source_location() {
        let source = fixture("01-current-policy.md")
            .replace("schema_version: 1\n", "")
            .replace("id: policy-current", "note_id: policy-current")
            .replace("type: policy", "kind: policy")
            .replace("sensitivity: internal", "visibility: internal");
        let parsed = parse_note("legacy.md", &source).expect("legacy note migrates");

        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].code, "front_matter_migrated");
        assert_eq!(parsed.diagnostics[0].location.file, "legacy.md");
        assert_eq!(parsed.diagnostics[0].location.line, 1);
        assert_eq!(parsed.note.schema_version, 1);
    }

    #[test]
    fn published_schema_tracks_required_contract_fields() {
        let schema_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/memory-note-v1.schema.json");
        let schema: Value =
            serde_json::from_str(&fs::read_to_string(schema_path).expect("schema is readable"))
                .expect("schema is valid JSON");
        let required = schema["required"].as_array().expect("required is an array");

        for field in [
            "schema_version",
            "id",
            "type",
            "scope",
            "provenance",
            "trust",
            "sensitivity",
        ] {
            assert!(required.contains(&Value::from(field)), "missing {field}");
        }
        assert_eq!(schema["additionalProperties"], true);
    }

    proptest! {
        #[test]
        fn arbitrary_unknown_scalar_fields_survive_round_trip(
            key in "x_[a-z]{1,16}",
            value in any::<i64>(),
        ) {
            let mut extra = BTreeMap::new();
            extra.insert(key.clone(), Value::from(value));
            let mut note = parse_note(
                "01-current-policy.md",
                &fixture("01-current-policy.md"),
            ).expect("fixture parses").note;
            note.extra = extra;

            let json = serde_json::to_string(&note).expect("serializes");
            let decoded: Note = serde_json::from_str(&json).expect("deserializes");
            prop_assert_eq!(decoded.extra[&key].clone(), Value::from(value));
        }

        #[test]
        fn arbitrary_malformed_input_never_panics(source in ".{0,4096}") {
            let limits = ParserLimits {
                max_file_bytes: 4096,
                ..ParserLimits::default()
            };
            let _ = parse_note_with_limits("fuzz.md", &source, &limits);
        }
    }

    #[test]
    fn hard_limits_reject_pathological_inputs_without_panicking() {
        let source = fixture("01-current-policy.md");
        let limits = ParserLimits {
            max_file_bytes: source.len() - 1,
            ..ParserLimits::default()
        };
        assert!(matches!(
            parse_note_with_limits("large.md", &source, &limits),
            Err(ParseError::ResourceLimit {
                resource: "file bytes",
                ..
            })
        ));

        let links = source.replace(
            "Deploy changes through a reviewed pull request.",
            "[[one]] [[two]]",
        );
        let limits = ParserLimits {
            max_links: 1,
            ..ParserLimits::default()
        };
        assert!(matches!(
            parse_note_with_limits("links.md", &links, &limits),
            Err(ParseError::ResourceLimit {
                resource: "links",
                ..
            })
        ));
    }

    #[test]
    fn seeded_malformed_corpus_returns_errors_without_crashing() {
        let repeated = "---\nschema_version: 1\n---\n# heading\n".repeat(200);
        let seeds = [
            "",
            "---",
            "---\n[\n---\n",
            "---\nschema_version: 999\n---\n",
            "---\nid: null\n---\n",
            "---\n{}\n---\n[[unterminated",
            "---\n- list\n---\nbody",
            "not front matter",
            "\0\0\0",
            repeated.as_str(),
        ];
        for (index, seed) in seeds.iter().enumerate() {
            let _ = parse_note(format!("seed-{index}.md"), seed);
        }
    }
}
