use serde_json::{Map, Value};
use thiserror::Error;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("front matter must be an object")]
    NotAnObject,
    #[error("schema version {0} is newer than this compiler supports")]
    FutureVersion(u64),
}

/// Migrates front matter from a supported historical schema to the current version.
///
/// # Errors
///
/// Returns an error when the value is not an object or declares a schema version
/// newer than this library supports.
pub fn migrate_front_matter(value: Value) -> Result<Value, MigrationError> {
    let Value::Object(mut map) = value else {
        return Err(MigrationError::NotAnObject);
    };

    let version = map
        .get("schema_version")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if version > u64::from(CURRENT_SCHEMA_VERSION) {
        return Err(MigrationError::FutureVersion(version));
    }

    if version == 0 {
        rename_if_absent(&mut map, "note_id", "id");
        rename_if_absent(&mut map, "kind", "type");
        rename_if_absent(&mut map, "visibility", "sensitivity");
        map.insert("schema_version".into(), Value::from(CURRENT_SCHEMA_VERSION));
    }

    Ok(Value::Object(map))
}

fn rename_if_absent(map: &mut Map<String, Value>, old: &str, new: &str) {
    if !map.contains_key(new)
        && let Some(value) = map.remove(old)
    {
        map.insert(new.to_owned(), value);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn migrates_legacy_names_without_losing_unknown_fields() {
        let migrated = migrate_front_matter(json!({
            "note_id": "legacy",
            "kind": "fact",
            "visibility": "internal",
            "future_key": {"enabled": true}
        }))
        .expect("migration succeeds");

        assert_eq!(migrated["schema_version"], 1);
        assert_eq!(migrated["id"], "legacy");
        assert_eq!(migrated["type"], "fact");
        assert_eq!(migrated["future_key"]["enabled"], true);
    }
}
