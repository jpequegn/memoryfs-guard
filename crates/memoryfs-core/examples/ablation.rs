use std::{fs, path::PathBuf};

use chrono::{TimeZone, Utc};
use memoryfs_core::{build_default_eval_cases, parse_note, render_ablation_markdown, run_ablation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let vault = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vault");
    let mut paths: Vec<_> = fs::read_dir(vault)?
        .map(|entry| entry.map(|item| item.path()))
        .collect::<Result<_, _>>()?;
    paths.retain(|path| path.extension().is_some_and(|extension| extension == "md"));
    paths.sort();
    let notes = paths
        .iter()
        .map(|path| {
            let source = fs::read_to_string(path)?;
            parse_note(path.display().to_string(), &source)
                .map_err(|error| std::io::Error::other(error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let cases = build_default_eval_cases(&notes)?;
    let now = Utc
        .with_ymd_and_hms(2026, 8, 21, 12, 0, 0)
        .single()
        .ok_or("invalid evaluation timestamp")?;
    let report = run_ablation(&cases, now)?;

    print!("{}", render_ablation_markdown(&report));
    Ok(())
}
