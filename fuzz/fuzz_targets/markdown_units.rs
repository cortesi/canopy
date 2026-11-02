//! Arbitrary text into the edit units of Markdown.
//!
//! The units must keep the invariants of
//! `canopy_markdown::testing::check_units` and match its slow reference, and a
//! replace of each unit's own text must keep them too. An input is the text,
//! which decodes as lossy UTF-8.

#![no_main]

use canopy_markdown::testing::check_units;
use libfuzzer_sys::fuzz_target;

/// The longest text of one input, in chars. Each unit's checks parse the
/// text again, so the cost of an input grows with its units and its length.
const MAX_TEXT: usize = 2048;

fuzz_target!(|data: &[u8]| {
    let text: String = String::from_utf8_lossy(data)
        .chars()
        .take(MAX_TEXT)
        .collect();
    if let Err(error) = check_units(&text) {
        panic!("{text:?}: {error}");
    }
});
