//! A frame as styled text, for renderers outside the terminal.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use super::TermBuf;
use crate::{
    geom::Point,
    style::{AttrSet, Color, ResolvedStyle},
};

/// The cells of a terminal buffer as styled text: each row is a list of runs
/// of text in one style. A renderer outside the terminal, such as a
/// screenshot, draws a frame from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenCapture {
    /// Width in cells.
    pub width: u32,
    /// Height in cells.
    pub height: u32,
    /// The distinct styles of the frame. A run names its style by index.
    pub styles: Vec<CaptureStyle>,
    /// The runs of each row, top to bottom. The runs of a row fill its
    /// width.
    pub rows: Vec<Vec<CaptureRun>>,
}

/// One style of a [`ScreenCapture`], with resolved colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureStyle {
    /// Foreground color.
    #[serde(serialize_with = "hex", deserialize_with = "unhex")]
    pub fg: [u8; 3],
    /// Background color.
    #[serde(serialize_with = "hex", deserialize_with = "unhex")]
    pub bg: [u8; 3],
    /// Text attributes, as a list of the lowercase names of those that are
    /// on.
    #[serde(
        default,
        skip_serializing_if = "no_attrs",
        serialize_with = "attr_names",
        deserialize_with = "parse_attrs"
    )]
    pub attrs: AttrSet,
}

/// Text in one style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureRun {
    /// The text.
    pub text: String,
    /// Index of the style in [`ScreenCapture::styles`], from 0.
    pub style: usize,
    /// Cells that the run covers. A wide grapheme covers two, so a renderer
    /// places the next run without measuring the text.
    pub cells: u32,
}

impl CaptureStyle {
    /// Return the capture style of a resolved style.
    fn of(style: ResolvedStyle) -> Self {
        let rgb = |color: Color| {
            let (r, g, b) = color.rgb();
            [r, g, b]
        };
        Self {
            fg: rgb(style.fg),
            bg: rgb(style.bg),
            attrs: style.attrs,
        }
    }
}

impl TermBuf {
    /// Return the cells as styled text.
    pub fn capture(&self) -> ScreenCapture {
        let mut styles: Vec<CaptureStyle> = Vec::new();
        let mut rows = Vec::with_capacity(self.size.h as usize);
        for y in 0..self.size.h {
            let mut runs: Vec<CaptureRun> = Vec::new();
            for x in 0..self.size.w {
                let cell = self
                    .get(Point { x, y })
                    .expect("buffer coordinates should always be valid");
                // The base cell of a wide grapheme holds its text and style,
                // and the run that holds it covers the continuation too.
                if cell.continuation {
                    if let Some(run) = runs.last_mut() {
                        run.cells += 1;
                    }
                    continue;
                }
                let style = CaptureStyle::of(cell.style);
                let index = styles.iter().position(|known| *known == style);
                let index = index.unwrap_or_else(|| {
                    styles.push(style);
                    styles.len() - 1
                });
                match runs.last_mut() {
                    Some(run) if run.style == index => {
                        cell.push_text(&mut run.text);
                        run.cells += 1;
                    }
                    _ => {
                        let mut text = String::new();
                        cell.push_text(&mut text);
                        runs.push(CaptureRun {
                            text,
                            style: index,
                            cells: 1,
                        });
                    }
                }
            }
            rows.push(runs);
        }
        ScreenCapture {
            width: self.size.w,
            height: self.size.h,
            styles,
            rows,
        }
    }
}

/// Serialize a color as `#rrggbb`.
fn hex<S: Serializer>(rgb: &[u8; 3], serializer: S) -> Result<S::Ok, S::Error> {
    let [r, g, b] = rgb;
    serializer.serialize_str(&format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Return whether no attribute is on.
fn no_attrs(attrs: &AttrSet) -> bool {
    !attrs.named().iter().any(|(_, on)| *on)
}

/// Serialize attributes as the names of those that are on.
fn attr_names<S: Serializer>(attrs: &AttrSet, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(
        attrs
            .named()
            .into_iter()
            .filter(|(_, on)| *on)
            .map(|(name, _)| name),
    )
}

/// Deserialize attributes from the names of those that are on.
fn parse_attrs<'de, D: Deserializer<'de>>(deserializer: D) -> Result<AttrSet, D::Error> {
    let mut attrs = AttrSet::default();
    for name in Vec::<String>::deserialize(deserializer)? {
        if !attrs.set_named(&name) {
            return Err(D::Error::custom(format!("unknown attribute `{name}`")));
        }
    }
    Ok(attrs)
}

/// Deserialize a color from `#rrggbb`.
fn unhex<'de, D: Deserializer<'de>>(deserializer: D) -> Result<[u8; 3], D::Error> {
    let text = String::deserialize(deserializer)?;
    let digits = text
        .strip_prefix('#')
        .filter(|digits| digits.len() == 6 && digits.is_ascii())
        .ok_or_else(|| D::Error::custom(format!("color `{text}` is not #rrggbb")))?;
    let channel = |at: usize| {
        u8::from_str_radix(&digits[at..at + 2], 16)
            .map_err(|_| D::Error::custom(format!("color `{text}` is not #rrggbb")))
    };
    Ok([channel(0)?, channel(2)?, channel(4)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::{Line, Size};

    #[test]
    fn a_capture_groups_cells_into_runs_and_round_trips() {
        let plain = ResolvedStyle::new(Color::White, Color::Black, AttrSet::default());
        let bold = ResolvedStyle::new(
            Color::Rgb { r: 1, g: 2, b: 3 },
            Color::Black,
            AttrSet {
                bold: true,
                ..AttrSet::default()
            },
        );
        let mut buf = TermBuf::new(Size::new(6, 2), ' ', plain).expect("buffer");
        buf.text(&bold, Line::new(1, 0, 4), "ab界").expect("text");
        let capture = buf.capture();
        assert_eq!(capture.styles.len(), 2);
        let texts: Vec<_> = capture.rows[0]
            .iter()
            .map(|run| (run.text.as_str(), run.style, run.cells))
            .collect();
        // The wide grapheme covers two cells of its run.
        assert_eq!(texts, [(" ", 0, 1), ("ab界", 1, 4), (" ", 0, 1)]);
        assert_eq!(capture.rows[1].len(), 1);
        assert_eq!(capture.rows[1][0].text, "      ");

        let json = serde_json::to_value(&capture).expect("json");
        assert_eq!(json["styles"][1]["fg"], "#010203");
        assert_eq!(json["styles"][1]["attrs"], serde_json::json!(["bold"]));
        assert!(json["styles"][0].get("attrs").is_none());
        let back: ScreenCapture = serde_json::from_value(json.clone()).expect("parse");
        assert_eq!(back, capture);
        let mut unknown = json;
        unknown["styles"][1]["attrs"] = serde_json::json!(["Bold"]);
        assert!(serde_json::from_value::<ScreenCapture>(unknown).is_err());
    }
}
