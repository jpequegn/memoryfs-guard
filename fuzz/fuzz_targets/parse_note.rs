#![no_main]

use libfuzzer_sys::fuzz_target;
use memoryfs_core::{ParserLimits, parse_note_with_limits};

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let limits = ParserLimits {
            max_file_bytes: 64 * 1024,
            max_front_matter_bytes: 16 * 1024,
            max_body_bytes: 48 * 1024,
            max_links: 128,
            max_attachments: 32,
            max_heading_depth: 6,
        };
        let _ = parse_note_with_limits("fuzz.md", source, &limits);
    }
});
