//! Convert the bitmap faces of `BigText` into the module that
//! `canopy-widgets` compiles.
//!
//! The sources are the Tamzen BDF files, unchanged, and Canopy's compact
//! font. The converter validates each file, moves the VT100 line set from the
//! slots below 32 to its code points, takes a glyph that one weight lacks
//! from the other weight of the same size, and derives the symbols that
//! Tamzen lacks from the glyphs of each face. Every face then has the same
//! repertoire.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

/// The font sources, relative to the workspace root.
const FONTS: &str = "crates/canopy-widgets/assets/fonts";
/// The generated module, relative to the workspace root.
const OUTPUT: &str = "crates/canopy-widgets/src/big_text/faces.rs";
/// The Tamzen sizes, smallest first.
const SIZES: [&str; 7] = ["5x9", "6x12", "7x13", "7x14", "8x15", "8x16", "10x20"];
/// The Tamzen weights: the file suffix and the name in the module.
const WEIGHTS: [(&str, &str); 2] = [("b", "BOLD"), ("r", "REGULAR")];
/// The VT100 line set in the slots below 32, with the code point of each.
/// The other slots below 32 hold duplicates or nothing, and go.
const LINE_SET: [(u32, char); 12] = [
    (2, '▒'),
    (11, '┘'),
    (12, '┐'),
    (13, '┌'),
    (14, '└'),
    (15, '┼'),
    (18, '─'),
    (21, '├'),
    (22, '┤'),
    (23, '┴'),
    (24, '┬'),
    (25, '│'),
];
/// The symbols derived from the glyphs of each Tamzen face.
const DERIVED: [char; 12] = ['≤', '≥', '±', '—', '·', '…', 'µ', '€', '↑', '↓', '▲', '▼'];
/// Rows of each glyph of the compact font.
const COMPACT_ROWS: usize = 5;

/// Write the module, or with `check`, fail when the module on disk differs
/// from what the sources give.
pub fn run(workspace_root: &Path, check: bool) -> bool {
    let result = generate(workspace_root).and_then(|module| {
        let path = workspace_root.join(OUTPUT);
        if check {
            let current = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {OUTPUT}: {error}"))?;
            if current != module {
                return Err(format!("{OUTPUT} is stale: run `cargo xtask big-text`"));
            }
            println!("{OUTPUT} is current");
        } else {
            fs::write(&path, module).map_err(|error| format!("cannot write {OUTPUT}: {error}"))?;
            println!("wrote {OUTPUT}");
        }
        Ok(())
    });
    match result {
        Ok(()) => true,
        Err(error) => {
            eprintln!("{error}");
            false
        }
    }
}

/// One glyph: a bitmap placed against the pen and the baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Glyph {
    /// Pixel columns from the pen to the left edge of the bitmap.
    x: i32,
    /// Pixel rows from the baseline up to the top edge of the bitmap.
    top: i32,
    /// Advance of the pen in pixels.
    advance: i32,
    /// Pixels, top row first.
    rows: Vec<Vec<bool>>,
}

impl Glyph {
    /// Build a glyph from ink pixels, each a column from the pen and a row
    /// above the baseline (0 is the row on the baseline).
    fn from_pixels(pixels: &BTreeSet<(i32, i32)>, advance: i32) -> Self {
        let Some(left) = pixels.iter().map(|p| p.0).min() else {
            return Self {
                x: 0,
                top: 0,
                advance,
                rows: Vec::new(),
            };
        };
        let right = pixels.iter().map(|p| p.0).max().unwrap_or(left);
        let low = pixels.iter().map(|p| p.1).min().unwrap_or(0);
        let high = pixels.iter().map(|p| p.1).max().unwrap_or(0);
        let rows = (low..=high)
            .rev()
            .map(|row| {
                (left..=right)
                    .map(|column| pixels.contains(&(column, row)))
                    .collect()
            })
            .collect();
        Self {
            x: left,
            top: high + 1,
            advance,
            rows,
        }
    }

    /// Return the ink pixels, each a column from the pen and a row above the
    /// baseline.
    fn pixels(&self) -> BTreeSet<(i32, i32)> {
        let mut pixels = BTreeSet::new();
        for (index, row) in self.rows.iter().enumerate() {
            for (column, ink) in row.iter().enumerate() {
                if *ink {
                    pixels.insert((self.x + column as i32, self.top - 1 - index as i32));
                }
            }
        }
        pixels
    }

    /// Return the glyph cropped to its ink.
    fn cropped(&self) -> Self {
        Self::from_pixels(&self.pixels(), self.advance)
    }

    /// Return the ink bounds: the left and right columns, and the lowest and
    /// highest rows, all inclusive.
    fn ink(&self) -> Option<Ink> {
        let pixels = self.pixels();
        Some(Ink {
            left: pixels.iter().map(|p| p.0).min()?,
            right: pixels.iter().map(|p| p.0).max()?,
            low: pixels.iter().map(|p| p.1).min()?,
            high: pixels.iter().map(|p| p.1).max()?,
        })
    }
}

/// The ink bounds of a glyph, inclusive.
#[derive(Debug, Clone, Copy)]
struct Ink {
    /// Leftmost ink column.
    left: i32,
    /// Rightmost ink column.
    right: i32,
    /// Lowest ink row above the baseline.
    low: i32,
    /// Highest ink row above the baseline.
    high: i32,
}

impl Ink {
    /// Ink width in pixels.
    fn width(self) -> i32 {
        self.right - self.left + 1
    }

    /// Ink height in pixels.
    fn height(self) -> i32 {
        self.high - self.low + 1
    }
}

/// Glyphs of one face, by character.
type Glyphs = BTreeMap<char, Glyph>;

/// Build the module from the sources.
fn generate(workspace_root: &Path) -> Result<String, String> {
    let fonts = workspace_root.join(FONTS);
    let mut faces: Vec<(String, Glyphs)> = Vec::new();
    for size in SIZES {
        let mut pair = Vec::new();
        for (suffix, weight) in WEIGHTS {
            let file = format!("tamzen/Tamzen{size}{suffix}.bdf");
            let text = fs::read_to_string(fonts.join(&file))
                .map_err(|error| format!("cannot read {FONTS}/{file}: {error}"))?;
            let glyphs = parse_bdf(&text).map_err(|error| format!("{file}: {error}"))?;
            let name = format!("TAMZEN_{}_{weight}", size.to_uppercase());
            pair.push((name, glyphs));
        }
        repair(&mut pair);
        faces.extend(pair);
    }
    let repertoire = faces[0].1.keys().copied().collect::<BTreeSet<_>>();
    for (name, glyphs) in &faces {
        let chars = glyphs.keys().copied().collect::<BTreeSet<_>>();
        if chars != repertoire {
            return Err(format!(
                "{name} differs from the repertoire: {:?}",
                chars.symmetric_difference(&repertoire).collect::<Vec<_>>()
            ));
        }
    }
    for (name, glyphs) in &mut faces {
        set_baseline(glyphs).map_err(|error| format!("{name}: {error}"))?;
        derive_symbols(glyphs).map_err(|error| format!("{name}: {error}"))?;
    }
    let compact_text = fs::read_to_string(fonts.join("compact.txt"))
        .map_err(|error| format!("cannot read {FONTS}/compact.txt: {error}"))?;
    let compact = parse_compact(&compact_text).map_err(|error| format!("compact.txt: {error}"))?;
    let mut sources = vec![("compact.txt".to_owned(), compact_text)];
    for size in SIZES {
        for (suffix, _) in WEIGHTS {
            let file = format!("tamzen/Tamzen{size}{suffix}.bdf");
            let text = fs::read_to_string(fonts.join(&file))
                .map_err(|error| format!("cannot read {FONTS}/{file}: {error}"))?;
            sources.push((file, text));
        }
    }
    Ok(module(&sources, &compact, &faces))
}

/// Give each face of one size the glyphs that only the other weight has.
fn repair(pair: &mut [(String, Glyphs)]) {
    let union = pair
        .iter()
        .flat_map(|(_, glyphs)| glyphs.keys().copied())
        .collect::<BTreeSet<_>>();
    let donors = pair
        .iter()
        .map(|(_, glyphs)| glyphs.clone())
        .collect::<Vec<_>>();
    for (index, (_, glyphs)) in pair.iter_mut().enumerate() {
        for ch in &union {
            if glyphs.contains_key(ch) {
                continue;
            }
            if let Some(glyph) = donors
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .find_map(|(_, donor)| donor.get(ch))
            {
                glyphs.insert(*ch, glyph.clone());
            }
        }
    }
}

/// Parse a BDF file into its glyphs, by character, with the line set at its
/// code points.
fn parse_bdf(text: &str) -> Result<Glyphs, String> {
    let mut glyphs = Glyphs::new();
    let mut declared = None;
    let mut parsed = 0;
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("CHARS") => declared = words.next().and_then(|n| n.parse::<usize>().ok()),
            Some("STARTCHAR") => {
                let (encoding, glyph) = parse_char(&mut lines)?;
                parsed += 1;
                if let Some(ch) = char_for(encoding) {
                    glyphs.insert(ch, glyph.cropped());
                }
            }
            _ => {}
        }
    }
    match declared {
        Some(count) if count == parsed => Ok(glyphs),
        Some(count) => Err(format!("declares {count} glyphs, has {parsed}")),
        None => Err("has no CHARS line".to_owned()),
    }
}

/// Parse one glyph, from after its `STARTCHAR` line to its `ENDCHAR` line.
fn parse_char<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Result<(u32, Glyph), String> {
    let mut encoding = None;
    let mut advance = None;
    let mut bbx = None;
    for line in lines.by_ref() {
        let words = line.split_whitespace().collect::<Vec<_>>();
        let number = |index: usize| -> Result<i32, String> {
            words
                .get(index)
                .and_then(|word| word.parse().ok())
                .ok_or_else(|| format!("bad line: {line}"))
        };
        match words.first().copied() {
            Some("ENCODING") => encoding = Some(number(1)?),
            Some("DWIDTH") => advance = Some(number(1)?),
            Some("BBX") => bbx = Some([number(1)?, number(2)?, number(3)?, number(4)?]),
            Some("BITMAP") => break,
            _ => {}
        }
    }
    let encoding = encoding.ok_or("a glyph has no ENCODING")?;
    let advance = advance.ok_or_else(|| format!("glyph {encoding} has no DWIDTH"))?;
    let [width, height, x, bottom] = bbx.ok_or_else(|| format!("glyph {encoding} has no BBX"))?;
    let digits = ((width + 7) / 8 * 2) as usize;
    let mut rows = Vec::new();
    for line in lines.by_ref() {
        let line = line.trim();
        if line == "ENDCHAR" {
            break;
        }
        if line.len() != digits {
            return Err(format!(
                "glyph {encoding} has a row of {} digits, not {digits}",
                line.len()
            ));
        }
        let bits = u64::from_str_radix(line, 16)
            .map_err(|_| format!("glyph {encoding} has a bad row: {line}"))?;
        let span = digits as i32 * 4;
        rows.push(
            (0..width)
                .map(|column| bits >> (span - 1 - column) & 1 == 1)
                .collect(),
        );
    }
    if rows.len() != height as usize {
        return Err(format!(
            "glyph {encoding} has {} rows, not {height}",
            rows.len()
        ));
    }
    let encoding = u32::try_from(encoding).map_err(|_| format!("bad encoding {encoding}"))?;
    Ok((
        encoding,
        Glyph {
            x,
            top: bottom + height,
            advance,
            rows,
        },
    ))
}

/// Return the character of a BDF encoding, or `None` for a slot that goes.
fn char_for(encoding: u32) -> Option<char> {
    match encoding {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(encoding),
        _ => LINE_SET
            .iter()
            .find(|(slot, _)| *slot == encoding)
            .map(|(_, ch)| *ch),
    }
}

/// Parse the compact font: a code point line, then five rows of `.` and `#`.
/// A line that starts with `//` is a comment.
fn parse_compact(text: &str) -> Result<Glyphs, String> {
    let mut glyphs = Glyphs::new();
    let mut lines = text
        .lines()
        .filter(|line| !line.starts_with("//") && !line.trim().is_empty());
    while let Some(header) = lines.next() {
        let code = header
            .split_whitespace()
            .next()
            .and_then(|word| word.strip_prefix("U+"))
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .and_then(char::from_u32)
            .ok_or_else(|| format!("bad glyph header: {header}"))?;
        let rows = lines.by_ref().take(COMPACT_ROWS).collect::<Vec<_>>();
        let width = rows.first().map_or(0, |row| row.len());
        if rows.len() != COMPACT_ROWS || rows.iter().any(|row| row.len() != width) {
            return Err(format!(
                "glyph {header} needs {COMPACT_ROWS} rows of one width"
            ));
        }
        if rows
            .iter()
            .any(|row| row.chars().any(|c| c != '.' && c != '#'))
        {
            return Err(format!("glyph {header} has a pixel that is not . or #"));
        }
        let glyph = Glyph {
            x: 0,
            top: COMPACT_ROWS as i32,
            advance: width as i32 + 1,
            rows: rows
                .iter()
                .map(|row| row.chars().map(|c| c == '#').collect())
                .collect(),
        };
        if glyphs.insert(code, glyph.cropped()).is_some() {
            return Err(format!("glyph {header} appears twice"));
        }
    }
    Ok(glyphs)
}

/// Move the baseline of a face to the bottom of its capitals. Tamzen draws
/// its capitals one row above the baseline of the BDF file, and the compact
/// font draws them on it, so the two faces share a baseline only after this.
fn set_baseline(glyphs: &mut Glyphs) -> Result<(), String> {
    let low = glyphs
        .get(&'H')
        .and_then(Glyph::ink)
        .ok_or("no H to find the baseline")?
        .low;
    for glyph in glyphs.values_mut() {
        if !glyph.rows.is_empty() {
            glyph.top -= low;
        }
    }
    Ok(())
}

/// Add the symbols that Tamzen lacks, each derived from glyphs of the face,
/// so it takes the stroke weight and proportions of the face.
fn derive_symbols(glyphs: &mut Glyphs) -> Result<(), String> {
    let metrics = Metrics::of(glyphs)?;
    // Derived glyphs read only the source glyphs, so they join the face at
    // the end.
    let mut derived = barred(glyphs, &metrics)?;
    derived.extend(dots_and_dashes(glyphs, &metrics)?);
    derived.push(micro(glyphs)?);
    derived.push(euro(glyphs, &metrics)?);
    derived.extend(arrows(&metrics));
    derived.extend(triangles(&metrics));
    glyphs.extend(derived);
    for ch in DERIVED {
        if !glyphs.contains_key(&ch) {
            return Err(format!("{ch} was not derived"));
        }
    }
    Ok(())
}

/// Return a glyph of a face and its ink.
fn source(glyphs: &Glyphs, ch: char) -> Result<(Glyph, Ink), String> {
    let glyph = glyphs
        .get(&ch)
        .ok_or_else(|| format!("no {ch} to derive from"))?;
    let ink = glyph.ink().ok_or_else(|| format!("{ch} has no ink"))?;
    Ok((glyph.clone(), ink))
}

/// The measures of a face that derived symbols take.
struct Metrics {
    /// Ink of the bar of the hyphen.
    bar: Ink,
    /// Advance of the hyphen.
    dash_advance: i32,
    /// Ink of the digit zero.
    digit: Ink,
    /// Advance of the digits.
    advance: i32,
    /// Ink of the capital H.
    cap: Ink,
    /// Ink of the vertical bar, whose width is the stem of the face.
    stem: Ink,
}

impl Metrics {
    /// Measure a face.
    fn of(glyphs: &Glyphs) -> Result<Self, String> {
        let (dash, bar) = source(glyphs, '-')?;
        let (zero, digit) = source(glyphs, '0')?;
        Ok(Self {
            bar,
            dash_advance: dash.advance,
            digit,
            advance: zero.advance,
            cap: source(glyphs, 'H')?.1,
            stem: source(glyphs, '|')?.1,
        })
    }

    /// Return the row at the middle of the bar of the hyphen.
    fn center(&self) -> i32 {
        (self.bar.low + self.bar.high) / 2
    }
}

/// Return `≤`, `≥`, and `±`: a glyph over a bar, one blank row apart. The
/// glyph shrinks to fit above the bar within the cap height, so a bound such
/// as `≥12K` fits like the digits beside it.
fn barred(glyphs: &Glyphs, metrics: &Metrics) -> Result<Vec<(char, Glyph)>, String> {
    let bar = metrics.bar;
    let mut derived = Vec::new();
    for (ch, base) in [('≤', '<'), ('≥', '>'), ('±', '+')] {
        let (glyph, _) = source(glyphs, base)?;
        let glyph = resample(&glyph.cropped(), metrics.cap.height() - bar.height() - 1);
        let ink = glyph.ink().ok_or_else(|| format!("{base} has no ink"))?;
        let lift = bar.height() + 1 - ink.low;
        let mut pixels = glyph
            .pixels()
            .into_iter()
            .map(|(column, row)| (column, row + lift))
            .collect::<BTreeSet<_>>();
        for row in 0..bar.height() {
            for column in ink.left..=ink.right {
                pixels.insert((column, row));
            }
        }
        derived.push((ch, Glyph::from_pixels(&pixels, glyph.advance)));
    }
    Ok(derived)
}

/// Return `—`, the bar of the hyphen across its advance; `·`, the period on
/// the bar of the hyphen; and `…`, three periods one blank column apart.
fn dots_and_dashes(glyphs: &Glyphs, metrics: &Metrics) -> Result<Vec<(char, Glyph)>, String> {
    let bar = metrics.bar;
    let mut dash = BTreeSet::new();
    for row in bar.low..=bar.high {
        for column in 0..metrics.dash_advance {
            dash.insert((column, row));
        }
    }
    let (period, dot) = source(glyphs, '.')?;
    let lift = metrics.center() - (dot.low + dot.high) / 2;
    let middle = period
        .pixels()
        .into_iter()
        .map(|(column, row)| (column, row + lift))
        .collect();
    let mut ellipsis = BTreeSet::new();
    for index in 0..3 {
        let shift = index * (dot.width() + 1);
        for (column, row) in period.pixels() {
            ellipsis.insert((column - dot.left + metrics.digit.left + shift, row));
        }
    }
    let width = 3 * dot.width() + 2;
    let ellipsis_advance = metrics.advance.max(metrics.digit.left + width + 1);
    Ok(vec![
        ('—', Glyph::from_pixels(&dash, metrics.dash_advance)),
        ('·', Glyph::from_pixels(&middle, period.advance)),
        ('…', Glyph::from_pixels(&ellipsis, ellipsis_advance)),
    ])
}

/// Return `µ`: u with its left stem down to the descender of p.
fn micro(glyphs: &Glyphs) -> Result<(char, Glyph), String> {
    let (u, u_ink) = source(glyphs, 'u')?;
    let (_, p_ink) = source(glyphs, 'p')?;
    let mut pixels = u.pixels();
    let stem_width = (u_ink.left..=u_ink.right)
        .take_while(|column| pixels.contains(&(*column, u_ink.high)))
        .count() as i32;
    for column in u_ink.left..u_ink.left + stem_width.max(1) {
        for row in p_ink.low..u_ink.low {
            pixels.insert((column, row));
        }
    }
    Ok(('µ', Glyph::from_pixels(&pixels, u.advance)))
}

/// Return `€`: C with two bars across its left side.
fn euro(glyphs: &Glyphs, metrics: &Metrics) -> Result<(char, Glyph), String> {
    let (c, c_ink) = source(glyphs, 'C')?;
    let mut pixels = c.pixels();
    let span = c_ink.height() - metrics.bar.height();
    for row in [c_ink.low + span / 3, c_ink.low + span * 2 / 3] {
        for column in c_ink.left - 1..c_ink.left + c_ink.width() * 2 / 3 {
            for thick in 0..metrics.bar.height() {
                pixels.insert((column, row + thick));
            }
        }
    }
    Ok(('€', Glyph::from_pixels(&pixels, c.advance)))
}

/// Return `↑` and `↓`, as tall as the capitals. An arrow shares the parity
/// of the stem, so the stem has a center: an odd width around a one-pixel
/// stem, an even width around a two-pixel one.
fn arrows(metrics: &Metrics) -> Vec<(char, Glyph)> {
    let (digit, stem) = (metrics.digit, metrics.stem);
    let parity = stem.width() % 2;
    let width = if digit.width() % 2 == parity {
        digit.width()
    } else {
        digit.width() - 1
    };
    let height = metrics.cap.high + 1;
    let head = (width + 1) / 2;
    let stem_left = digit.left + (width - stem.width()) / 2;
    [('↑', true), ('↓', false)]
        .into_iter()
        .map(|(ch, up)| {
            let mut pixels = BTreeSet::new();
            for step in 0..head {
                let row = if up { height - 1 - step } else { step };
                // Each row of the head is two pixels wider than the one
                // before, centered on the stem.
                let span = (stem.width() + 2 * step).min(width);
                let left = digit.left + (width - span) / 2;
                for column in left..left + span {
                    pixels.insert((column, row));
                }
            }
            let stems = if up { 0..height - head } else { head..height };
            for row in stems {
                for column in stem_left..stem_left + stem.width() {
                    pixels.insert((column, row));
                }
            }
            (ch, Glyph::from_pixels(&pixels, metrics.advance))
        })
        .collect()
}

/// Return `▲` and `▼`, centered on the bar of the hyphen. A triangle takes
/// an odd width, so it has a point.
fn triangles(metrics: &Metrics) -> Vec<(char, Glyph)> {
    let digit = metrics.digit;
    let width = if digit.width() % 2 == 0 {
        digit.width() - 1
    } else {
        digit.width()
    };
    let middle = digit.left + width / 2;
    let head = (width + 1) / 2;
    let base = metrics.center() - head / 2;
    [('▲', true), ('▼', false)]
        .into_iter()
        .map(|(ch, up)| {
            let mut pixels = BTreeSet::new();
            for step in 0..head {
                let row = if up {
                    base + head - 1 - step
                } else {
                    base + step
                };
                for column in middle - step..=middle + step {
                    pixels.insert((column, row));
                }
            }
            (ch, Glyph::from_pixels(&pixels, metrics.advance))
        })
        .collect()
}

/// Return a glyph resampled to `rows` rows, keeping its first and last rows
/// and its width. The result sits on the baseline.
fn resample(glyph: &Glyph, rows: i32) -> Glyph {
    let height = glyph.rows.len() as i32;
    let rows = rows.clamp(1, height.max(1));
    let picked = (0..rows)
        .map(|index| {
            let source = if rows == 1 {
                0
            } else {
                (index * (height - 1) + (rows - 1) / 2) / (rows - 1)
            };
            glyph.rows[source as usize].clone()
        })
        .collect();
    Glyph {
        x: glyph.x,
        top: rows,
        advance: glyph.advance,
        rows: picked,
    }
}

/// Return the module text.
fn module(sources: &[(String, String)], compact: &Glyphs, faces: &[(String, Glyphs)]) -> String {
    let mut out = String::new();
    out.push_str("//! The bitmap faces of `BigText`, generated by `cargo xtask big-text` from\n");
    out.push_str("//! `assets/fonts`. Do not edit this file: change the sources, and run the\n");
    out.push_str("//! task again.\n//!\n//! Sources, with their FNV-1a hashes:\n//!\n");
    for (name, text) in sources {
        out.push_str(&format!("//! - `{name}`: {:016x}\n", fnv(text.as_bytes())));
    }
    out.push_str("\nuse super::face::{Face, Glyph};\n");
    write_face(&mut out, "COMPACT", compact);
    for (name, glyphs) in faces {
        write_face(&mut out, name, glyphs);
    }
    out
}

/// Append one face to the module.
fn write_face(out: &mut String, name: &str, glyphs: &Glyphs) {
    let cap = glyphs
        .get(&'H')
        .or_else(|| glyphs.get(&'0'))
        .and_then(Glyph::ink)
        .map_or(0, |ink| ink.high + 1);
    let doc = if name == "COMPACT" {
        "Canopy's compact numerals and symbols.".to_owned()
    } else {
        let mut words = name.split('_').skip(1);
        let size = words.next().unwrap_or_default().to_lowercase();
        let weight = words.next().unwrap_or_default().to_lowercase();
        format!("Tamzen {size}, {weight}.")
    };
    out.push_str(&format!(
        "\n/// {doc}\npub(super) static {name}: Face = Face {{\n    cap: {cap},\n    glyphs: &[\n"
    ));
    for (ch, glyph) in glyphs {
        let width = glyph.rows.first().map_or(0, Vec::len);
        let rows = glyph
            .rows
            .iter()
            .map(|row| {
                let bits = row
                    .iter()
                    .fold(0_u32, |bits, ink| bits << 1 | u32::from(*ink));
                format!("{bits:#x}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "        ({ch:?}, Glyph {{ x: {}, top: {}, width: {width}, advance: {}, rows: &[{rows}] }}),\n",
            glyph.x, glyph.top, glyph.advance
        ));
    }
    out.push_str("    ],\n};\n");
}

/// Return the 64-bit FNV-1a hash of `bytes`, which marks the sources that a
/// module came from.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A BDF file with one glyph: a 2x2 square on the baseline, and one
    /// glyph in a slot that goes.
    const BDF: &str = "STARTFONT 2.1\nCHARS 2\n\
        STARTCHAR A\nENCODING 65\nDWIDTH 4 0\nBBX 2 2 1 0\nBITMAP\nC0\nC0\nENDCHAR\n\
        STARTCHAR x\nENCODING 7\nDWIDTH 4 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\nENDFONT\n";

    #[test]
    fn a_bdf_glyph_keeps_its_place_against_the_pen_and_the_baseline() {
        let glyphs = parse_bdf(BDF).expect("parses");
        assert_eq!(glyphs.len(), 1, "slot 7 is a duplicate and goes");
        let glyph = &glyphs[&'A'];
        assert_eq!((glyph.x, glyph.top, glyph.advance), (1, 2, 4));
        assert_eq!(glyph.rows, vec![vec![true, true], vec![true, true]]);
    }

    #[test]
    fn a_bdf_file_with_a_wrong_count_or_row_fails() {
        assert!(parse_bdf(&BDF.replace("CHARS 2", "CHARS 3")).is_err());
        assert!(parse_bdf(&BDF.replace("C0\nC0", "C0")).is_err());
        assert!(parse_bdf(&BDF.replace("C0\nC0", "C00\nC0")).is_err());
    }

    #[test]
    fn the_line_set_moves_to_its_code_points() {
        assert_eq!(char_for(18), Some('─'));
        assert_eq!(char_for(25), Some('│'));
        assert_eq!(char_for(7), None);
        assert_eq!(char_for(0x41), Some('A'));
    }

    #[test]
    fn a_compact_glyph_sits_on_the_baseline_with_one_blank_column() {
        let glyphs = parse_compact("U+0031 1\n.#.\n##.\n.#.\n.#.\n###\n").expect("parses");
        let glyph = &glyphs[&'1'];
        assert_eq!((glyph.x, glyph.top, glyph.advance), (0, 5, 4));
        assert!(parse_compact("U+0031 1\n.#.\n##\n").is_err());
    }

    #[test]
    fn the_converted_module_is_current() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace");
        let module = generate(root).expect("the sources convert");
        let current = fs::read_to_string(root.join(OUTPUT)).expect("the module exists");
        assert!(
            current == module,
            "{OUTPUT} is stale: run `cargo xtask big-text`"
        );
    }
}
