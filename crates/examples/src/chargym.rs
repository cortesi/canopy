//! Chargym: A Unicode width and wide character demo.

use canopy::{
    Binder, Canopy, Context, Loader, ReadContext, Widget, derive_commands,
    error::Result,
    event::{key, mouse},
    layout::Layout,
    render::Render,
};
use canopy_widgets::{CanvasWidth, Frame, Root, Text};
use unicode_width::UnicodeWidthStr;

/// Wrap width used for the text widget.
const WRAP_WIDTH: u32 = 120;
/// Width of the label column in the sample list.
const LABEL_WIDTH: usize = 12;
/// Separator between labels and sample text.
const LABEL_SEPARATOR: &str = " | ";
/// Width of the label + separator prefix before text content.
const RULER_PREFIX_LEN: usize = LABEL_WIDTH + LABEL_SEPARATOR.len();
/// Column count for the ruler lines, aligned with the sample text start.
const RULER_COLUMNS: usize = WRAP_WIDTH as usize - RULER_PREFIX_LEN;

/// ASCII-only sample text.
const SAMPLE_ASCII: &str =
    "The quick brown fox jumps over 13 lazy dogs, testing plain ASCII width.";
/// Sample text with combining marks.
const SAMPLE_COMBINING: &str = "e\u{301} o\u{308} a\u{30a} n\u{303} u\u{304} i\u{323}";
/// Sample text with CJK characters.
const SAMPLE_CJK: &str = concat!("\u{4e16}\u{754c} ", "\u{4e2d}\u{6587} ", "\u{6f22}\u{5b57}",);
/// Sample text with fullwidth Latin letters and digits.
const SAMPLE_FULLWIDTH: &str = concat!(
    "\u{ff26}\u{ff55}\u{ff4c}\u{ff4c}\u{ff57}\u{ff49}\u{ff44}\u{ff54}\u{ff48} ",
    "\u{ff11}\u{ff12}\u{ff13}\u{ff14}\u{ff15}",
);
/// Sample text with emoji characters.
const SAMPLE_EMOJI: &str = concat!(
    "\u{1f600}\u{1f603}\u{1f604}\u{1f601}\u{1f606} ",
    "\u{1f60d}\u{1f609}\u{1f642}",
);
/// Sample text with zero-width joiner sequences.
const SAMPLE_ZWJ: &str = concat!(
    "\u{1f469}\u{200d}\u{1f4bb} ",
    "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{200d}\u{1f466}",
);
/// Sample text that mixes narrow and wide characters.
const SAMPLE_MIXED: &str = concat!(
    "A\u{754c}B\u{1f642}C ",
    "cafe\u{301} ",
    "\u{65e5}\u{672c}\u{8a9e}",
);

/// Definition of a unicode sample line.
struct SampleSpec {
    /// Label used in the display.
    label: &'static str,
    /// Text content for the sample.
    text: &'static str,
    /// Short note describing the sample.
    note: &'static str,
}

/// Prepared sample with computed width metrics.
struct SampleLine {
    /// Label used in the display.
    label: &'static str,
    /// Text content for the sample.
    text: String,
    /// Unicode column width of the text.
    width: usize,
    /// Count of Unicode scalar values.
    chars: usize,
    /// UTF-8 byte length of the text.
    bytes: usize,
    /// Short note describing the sample.
    note: &'static str,
}

impl SampleLine {
    /// Build a sample line from a spec, computing width metrics.
    fn from_spec(spec: &SampleSpec) -> Self {
        let text = spec.text.to_string();
        let width = UnicodeWidthStr::width(text.as_str());
        let chars = text.chars().count();
        let bytes = text.len();
        Self {
            label: spec.label,
            text,
            width,
            chars,
            bytes,
            note: spec.note,
        }
    }
}

/// All unicode sample definitions used by the demo.
const SAMPLE_SPECS: &[SampleSpec] = &[
    SampleSpec {
        label: "ASCII",
        text: SAMPLE_ASCII,
        note: "Baseline ASCII.",
    },
    SampleSpec {
        label: "Combining",
        text: SAMPLE_COMBINING,
        note: "Combining marks (width 0 accents).",
    },
    SampleSpec {
        label: "CJK",
        text: SAMPLE_CJK,
        note: "Wide CJK characters.",
    },
    SampleSpec {
        label: "Fullwidth",
        text: SAMPLE_FULLWIDTH,
        note: "Fullwidth Latin + digits.",
    },
    SampleSpec {
        label: "Emoji",
        text: SAMPLE_EMOJI,
        note: "Emoji glyphs (often width 2).",
    },
    SampleSpec {
        label: "ZWJ",
        text: SAMPLE_ZWJ,
        note: "Zero-width joiner sequences.",
    },
    SampleSpec {
        label: "Mixed",
        text: SAMPLE_MIXED,
        note: "Mixed narrow + wide + combining.",
    },
];

/// Build the sample lines with computed metrics.
fn sample_lines() -> Vec<SampleLine> {
    SAMPLE_SPECS.iter().map(SampleLine::from_spec).collect()
}

/// Build ruler lines for column alignment.
fn build_ruler_lines(width: usize) -> (String, String) {
    let mut tens = String::with_capacity(width);
    let mut ones = String::with_capacity(width);

    for i in 0..width {
        let ones_digit = (i % 10) as u8;
        ones.push(char::from(b'0' + ones_digit));

        if i % 10 == 0 {
            let tens_digit = ((i / 10) % 10) as u8;
            tens.push(char::from(b'0' + tens_digit));
        } else {
            tens.push(' ');
        }
    }

    (tens, ones)
}

/// Build the full text content for the demo.
fn build_content() -> String {
    let samples = sample_lines();
    let mut lines = Vec::new();

    lines.push("chargym: unicode and wide character samples".to_string());
    lines.push("Use arrows/hjkl/PgUp/PgDn to scroll; q quits.".to_string());
    lines.push(String::new());
    lines.push(format!("Column ruler ({RULER_COLUMNS} cols):"));

    let (tens, ones) = build_ruler_lines(RULER_COLUMNS);
    let ruler_prefix = format!(
        "{: <label_width$}{sep}",
        "",
        label_width = LABEL_WIDTH,
        sep = LABEL_SEPARATOR
    );
    lines.push(format!("{ruler_prefix}{tens}"));
    lines.push(format!("{ruler_prefix}{ones}"));
    lines.push(String::new());

    for sample in samples {
        lines.push(format!(
            "{label: <label_width$}{sep}{text}",
            label = sample.label,
            text = sample.text,
            label_width = LABEL_WIDTH,
            sep = LABEL_SEPARATOR,
        ));
        lines.push(format!(
            "{: <label_width$}{sep}width={width} cols  chars={chars}  bytes={bytes}  {note}",
            "",
            label_width = LABEL_WIDTH,
            sep = LABEL_SEPARATOR,
            width = sample.width,
            chars = sample.chars,
            bytes = sample.bytes,
            note = sample.note,
        ));
        lines.push(String::new());
    }

    lines.push(
        "Note: ZWJ sequences may render as a single glyph or multiple cells depending on font."
            .to_string(),
    );

    lines.join("\n")
}

/// Root node for the chargym demo.
pub struct CharGym {
    /// Prebuilt content for the text widget.
    content: String,
}

impl Default for CharGym {
    fn default() -> Self {
        Self::new()
    }
}

#[derive_commands]
impl CharGym {
    /// Construct a new chargym demo.
    pub fn new() -> Self {
        Self {
            content: build_content(),
        }
    }
}

impl Widget for CharGym {
    fn accept_focus(&self, _ctx: &dyn ReadContext) -> bool {
        true
    }

    fn on_mount(&mut self, c: &mut dyn Context) -> Result<()> {
        let frame_id = c.add_child(Frame::new().with_title("chargym"))?;
        c.add_child_to(
            frame_id,
            Text::new(self.content.clone())
                .with_wrap_width(WRAP_WIDTH)
                .with_canvas_width(CanvasWidth::Intrinsic),
        )?;

        c.set_layout(Layout::fill())?;
        Ok(())
    }

    fn render(&mut self, _r: &mut Render, _ctx: &dyn ReadContext) -> Result<()> {
        Ok(())
    }
}

impl Loader for CharGym {
    fn load(c: &mut Canopy) -> Result<()> {
        c.add_commands::<Text>()?;
        Ok(())
    }
}

/// Install key bindings for the chargym demo.
pub fn setup_bindings(cnpy: &mut Canopy) {
    Binder::new(cnpy)
        .with_path("char_gym")
        .key_command('g', Text::cmd_scroll_to().call_with([0u32, 0u32]))
        .key_command('j', Text::cmd_scroll_down())
        .key_command(key::KeyCode::Down, Text::cmd_scroll_down())
        .mouse_command(mouse::Action::ScrollDown, Text::cmd_scroll_down())
        .key_command('k', Text::cmd_scroll_up())
        .key_command(key::KeyCode::Up, Text::cmd_scroll_up())
        .mouse_command(mouse::Action::ScrollUp, Text::cmd_scroll_up())
        .key_command('h', Text::cmd_scroll_left())
        .key_command(key::KeyCode::Left, Text::cmd_scroll_left())
        .key_command('l', Text::cmd_scroll_right())
        .key_command(key::KeyCode::Right, Text::cmd_scroll_right())
        .key_command(key::KeyCode::PageDown, Text::cmd_page_down())
        .key_command(' ', Text::cmd_page_down())
        .key_command(key::KeyCode::PageUp, Text::cmd_page_up())
        .with_path("root")
        .key_command('q', Root::cmd_quit());
}
