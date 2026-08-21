use chrono::{DateTime, Utc};
use memoryfs_core::{
    CompileRequest, LintContext, ParsedNote, compile_context, lint_vault, parse_note,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

#[derive(Debug, Clone, Deserialize)]
struct VaultFile {
    path: String,
    source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Graph {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GraphNode {
    id: String,
    title: String,
    note_type: String,
    sensitivity: String,
    trust: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GraphEdge {
    source: String,
    target: String,
    kind: String,
    line: usize,
}

#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    memoryfs_core::version().to_owned()
}

#[wasm_bindgen(js_name = parseNote)]
/// Parses one note and returns canonical JSON.
///
/// # Errors
///
/// Returns a JavaScript error when parsing or serialization fails.
pub fn parse_note_wasm(path: &str, source: &str) -> Result<String, JsValue> {
    parse_note_json(path, source).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen(js_name = lintVault)]
/// Lints an in-browser vault and returns a canonical JSON receipt.
///
/// # Errors
///
/// Returns a JavaScript error when input, parsing, or serialization fails.
pub fn lint_vault_wasm(vault_json: &str, now: &str) -> Result<String, JsValue> {
    lint_vault_json(vault_json, now).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen(js_name = graphVault)]
/// Builds the in-browser link graph as canonical JSON.
///
/// # Errors
///
/// Returns a JavaScript error when input, parsing, or serialization fails.
pub fn graph_vault_wasm(vault_json: &str) -> Result<String, JsValue> {
    graph_vault_json(vault_json).map_err(|error| JsValue::from_str(&error))
}

#[wasm_bindgen(js_name = compilePack)]
/// Compiles an in-browser context pack as canonical JSON.
///
/// # Errors
///
/// Returns a JavaScript error when input is invalid or evidence is refused.
pub fn compile_pack_wasm(
    vault_json: &str,
    request_json: &str,
    now: &str,
) -> Result<String, JsValue> {
    compile_pack_json(vault_json, request_json, now).map_err(|error| JsValue::from_str(&error))
}

/// Returns the same canonical parse response used by the WASM export.
///
/// # Errors
///
/// Returns a string error when the note cannot be parsed or serialized.
pub fn parse_note_json(path: &str, source: &str) -> Result<String, String> {
    let parsed = parse_note(path, source).map_err(|error| error.to_string())?;
    serde_json::to_string(&parsed).map_err(|error| error.to_string())
}

/// Returns the same canonical lint receipt used by the WASM export.
///
/// # Errors
///
/// Returns a string error when the vault, timestamp, notes, or response are invalid.
pub fn lint_vault_json(vault_json: &str, now: &str) -> Result<String, String> {
    let notes = parse_vault(vault_json)?;
    let evaluated_at = parse_timestamp(now)?;
    serde_json::to_string(&lint_vault(&notes, &LintContext::new(evaluated_at)))
        .map_err(|error| error.to_string())
}

/// Returns the same graph response used by the WASM export.
///
/// # Errors
///
/// Returns a string error when the vault, notes, or response are invalid.
pub fn graph_vault_json(vault_json: &str) -> Result<String, String> {
    let parsed_notes = parse_vault(vault_json)?;
    let mut nodes: Vec<_> = parsed_notes
        .iter()
        .map(|parsed| GraphNode {
            id: parsed.note.id.clone(),
            title: parsed
                .note
                .title
                .clone()
                .unwrap_or_else(|| parsed.note.id.clone()),
            note_type: enum_name(&parsed.note.note_type),
            sensitivity: enum_name(&parsed.note.sensitivity),
            trust: enum_name(&parsed.note.trust),
        })
        .collect();
    let mut edges: Vec<_> = parsed_notes
        .iter()
        .flat_map(|parsed| {
            parsed.links.iter().map(|link| GraphEdge {
                source: parsed.note.id.clone(),
                target: link
                    .target
                    .split(['#', '|'])
                    .next()
                    .unwrap_or(&link.target)
                    .trim_end_matches(".md")
                    .to_owned(),
                kind: enum_name(&link.kind),
                line: link.location.line,
            })
        })
        .collect();
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    edges.sort_by(|left, right| {
        (&left.source, &left.target, &left.kind, left.line).cmp(&(
            &right.source,
            &right.target,
            &right.kind,
            right.line,
        ))
    });
    serde_json::to_string(&Graph { nodes, edges }).map_err(|error| error.to_string())
}

/// Returns the same context pack response used by the WASM export.
///
/// # Errors
///
/// Returns a string error when any input is invalid or required evidence is refused.
pub fn compile_pack_json(
    vault_json: &str,
    request_json: &str,
    now: &str,
) -> Result<String, String> {
    let notes = parse_vault(vault_json)?;
    let request: CompileRequest =
        serde_json::from_str(request_json).map_err(|error| error.to_string())?;
    let evaluated_at = parse_timestamp(now)?;
    let pack =
        compile_context(&notes, &request, evaluated_at).map_err(|error| error.to_string())?;
    serde_json::to_string(&pack).map_err(|error| error.to_string())
}

fn parse_vault(vault_json: &str) -> Result<Vec<ParsedNote>, String> {
    let mut files: Vec<VaultFile> =
        serde_json::from_str(vault_json).map_err(|error| error.to_string())?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
        .into_iter()
        .map(|file| parse_note(file.path, &file.source).map_err(|error| error.to_string()))
        .collect()
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| error.to_string())
}

fn enum_name(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .expect("core enums serialize")
        .as_str()
        .expect("core enums serialize as strings")
        .to_owned()
}
