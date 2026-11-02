//! Arbitrary text, a unit, and a new text, through the edit of a block.
//!
//! The edit goes through the own-text rules and the splice of
//! `canopy_markdown::units`, and through `Markdown::apply_edit` in a reader:
//! its refusals, its new text, the edit it records, and the block cursor's
//! new place. The units of the result must keep their invariants. One
//! reader takes the text of each run as a new document, which gives the
//! results of a fresh reader without the cost of one.
//!
//! An input is a flags byte, two position bytes, the text, a zero byte, and
//! the new text, so seeds are easy to write by hand. The texts decode as
//! lossy UTF-8. The low bits of the flags choose the edit:
//!
//! - 0: the whole unit that the first position byte picks.
//! - 1: that unit's own text, by its rules.
//! - 2 and 3: the bytes between the two position bytes, scaled to the text,
//!   which can be reversed or off character boundaries. With bit 2 set, the
//!   range runs past the end.

#![no_main]

use std::cell::RefCell;

use canopy_markdown::{
    testing::{EditCheck, check_edit},
    units::{parse_units, replace_own_text},
};
use libfuzzer_sys::fuzz_target;

/// The longest text of one input, in chars. Each input lays the text out in
/// a reader twice, so long texts cost the fuzzer its pace.
const MAX_TEXT: usize = 1024;

/// The longest new text of one input, in chars.
const MAX_NEW: usize = 256;

thread_local! {
    /// One reader takes the text of every run, as a new document each time.
    static READER: RefCell<EditCheck> =
        RefCell::new(EditCheck::new().expect("a reader builds"));
}

/// Return the first `max` chars of `bytes`, decoded as lossy UTF-8.
fn text(bytes: &[u8], max: usize) -> String {
    String::from_utf8_lossy(bytes).chars().take(max).collect()
}

fuzz_target!(|data: &[u8]| {
    let [flags, first, second, rest @ ..] = data else {
        return;
    };
    let split = rest
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(rest.len());
    let source = text(&rest[..split], MAX_TEXT);
    let new = text(rest.get(split + 1..).unwrap_or_default(), MAX_NEW);
    let units = parse_units(&source);
    let unit = (!units.is_empty()).then(|| usize::from(*first) % units.len());
    let (from, to, replacement) = match (flags & 3, unit) {
        (0, Some(index)) => {
            if let Err(error) = check_edit(&source, index, &new, false) {
                panic!("{source:?}, unit {index} to {new:?}: {error}");
            }
            let range = units[index].range.clone();
            (range.start, range.end, new)
        }
        (1, Some(index)) => {
            if let Err(error) = check_edit(&source, index, &new, true) {
                panic!("{source:?}, own text of unit {index} to {new:?}: {error}");
            }
            match replace_own_text(&source, &units[index], &new) {
                Some(splice) => (splice.range.start, splice.range.end, splice.text),
                None => return,
            }
        }
        _ => {
            let scale = |byte: u8| usize::from(byte) * (source.len() + 1) / 256;
            let to = if flags & 4 == 0 {
                scale(*second)
            } else {
                source.len() + 1 + usize::from(*second)
            };
            (scale(*first), to, new)
        }
    };
    let checked = READER.with(|reader| reader.borrow_mut().check(&source, from, to, &replacement));
    if let Err(error) = checked {
        panic!("{source:?}, {from}..{to} to {replacement:?}: {error}");
    }
});
