//! Built-in themes: role palettes and the one rule set they share.
//!
//! A theme is a [`Palette`] of role colours. [`Palette::style_map`] builds the
//! rule set every built-in theme shares, so adding a rule here adds it to every
//! theme at once. Applications style their own paths from the palette through
//! `Setup::widget_styles`, which keeps those rules across theme switches.

use super::{Attr, AttrSet, Color, Mix, PartialStyle, StyleMap};
use crate::rgb;

/// How far a scrollbar thumb leans from the theme's base toward the accent.
const THUMB_ACCENT: f32 = 0.4;
/// How far a search match that is not current fades from yellow toward the
/// ground, so the current match stands out in the same hue.
const MATCH_FADE: f32 = 0.5;

/// The role colours a theme assigns.
///
/// Each field names the role a colour plays, not the colour itself, so the same
/// rule set can render a light theme, a dark theme, or any other palette.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Default foreground.
    pub fg: Color,
    /// Default background, and the foreground drawn on top of `accent`.
    pub bg: Color,
    /// Inactive frame borders.
    pub frame: Color,
    /// Border of the frame that holds focus.
    pub frame_focused: Color,
    /// Base colour of scrollbar thumbs, tinted toward the accent.
    pub frame_thumb: Color,
    /// Frame title text.
    pub frame_title: Color,
    /// Primary accent: focus and selection.
    pub accent: Color,
    /// Foreground on panel backgrounds, one step away from `fg`.
    pub muted_fg: Color,
    /// Quiet foreground a step below `muted_fg`: placeholders, gutters, and
    /// hints.
    pub faint_fg: Color,
    /// Background of panels such as the help overlay and prompt.
    pub panel_bg: Color,
    /// Background of raised elements set on a panel, such as inactive tabs.
    pub element_bg: Color,
    /// Selection background: editor selections and the active tab.
    pub selection_bg: Color,
    /// Editor line-number gutter.
    pub line_number: Color,
    /// Key names in the help overlay.
    pub key: Color,
    /// Named blue.
    pub blue: Color,
    /// Named red.
    pub red: Color,
    /// Named magenta.
    pub magenta: Color,
    /// Named violet.
    pub violet: Color,
    /// Named cyan.
    pub cyan: Color,
    /// Named green.
    pub green: Color,
    /// Named yellow, also the hue of search matches.
    pub yellow: Color,
    /// Named orange.
    pub orange: Color,
}

impl Palette {
    /// Build the shared rule set for this palette.
    pub fn style_map(&self) -> StyleMap {
        let p = self;
        let mut c = StyleMap::new();
        // The thumb is chrome, but the position is worth seeing, so it leans
        // toward the accent without becoming a highlight. A drag holds the full
        // accent, which reads as the thumb waking up.
        let thumb = p.frame_thumb.mix(p.accent, THUMB_ACCENT, Mix::Rgb);
        c.rules()
            .style(
                "/",
                PartialStyle::new()
                    .fg(p.fg)
                    .bg(p.bg)
                    .attrs(AttrSet::default()),
            )
            .fg("/frame", p.frame)
            .fg("/frame/focused", p.frame_focused)
            .fg("/frame/thumb", thumb)
            .fg("/frame/thumb/active", p.accent)
            .fg("/frame/title", p.frame_title)
            .fg("/columns/divider", p.frame)
            .fg("/columns/thumb", thumb)
            .fg("/columns/thumb/active", p.accent)
            // A diff reads changed rows by their colour, and its chrome stays
            // quiet: gaps, the empty half of a one-sided change, and the divider.
            .fg("/diff_view/context", p.fg)
            .fg("/diff_view/added", p.green)
            .fg("/diff_view/removed", p.red)
            .fg("/diff_view/header", p.accent)
            .fg("/diff_view/gap", p.muted_fg)
            .fg("/diff_view/missing", p.muted_fg)
            .fg("/diff_view/separator", p.frame)
            .fg("/diff_view/message", p.faint_fg)
            .fg("/diff_view/loading", p.faint_fg)
            .fg("/blue", p.blue)
            .fg("/red", p.red)
            .fg("/magenta", p.magenta)
            .fg("/violet", p.violet)
            .fg("/cyan", p.cyan)
            .fg("/green", p.green)
            .fg("/yellow", p.yellow)
            .fg("/orange", p.orange)
            .attr("/text/bold", Attr::Bold)
            .attr("/text/italic", Attr::Italic)
            .attr("/text/underline", Attr::Underline)
            // A button's accelerator shares the help overlay's key colour, so one
            // letter names the key without the label repeating it. The button keeps
            // whatever ground it sits on.
            .style(
                "/button/key",
                PartialStyle::new().fg(p.key).attrs(AttrSet::new(Attr::Bold)),
            )
            .fg("/button/focused/border", p.frame_focused)
            .fg("/button/disabled/border", p.muted_fg)
            .fg("/button/disabled/text", p.muted_fg)
            .fg("/button/disabled/key", p.muted_fg)
            .fg("/selector", p.fg)
            .fg("/selector/chosen", p.accent)
            .fg("/dropdown", p.fg)
            .fg("/dropdown/chosen", p.accent)
            .style(
                "/tabs/bar",
                PartialStyle::new().fg(p.muted_fg).bg(p.panel_bg),
            )
            .style(
                "/tabs/tab",
                PartialStyle::new().fg(p.muted_fg).bg(p.element_bg),
            )
            .style(
                "/tabs/tab/active",
                PartialStyle::new()
                    .fg(p.accent)
                    .bg(p.selection_bg)
                    .attrs(AttrSet::new(Attr::Bold)),
            )
            .style(
                "/tabs/tab/active/focused",
                PartialStyle::new()
                    .fg(p.bg)
                    .bg(p.accent)
                    .attrs(AttrSet::new(Attr::Bold)),
            )
            .style("/editor/text", PartialStyle::new().fg(p.fg).bg(p.bg))
            .style(
                "/editor/selection",
                PartialStyle::new().fg(p.fg).bg(p.selection_bg),
            )
            // Every match shares one hue, and lightness tells them apart: the
            // other matches fade toward the ground under the ordinary text,
            // and the current match takes the full yellow in bold.
            .style(
                "/editor/search/match",
                PartialStyle::new()
                    .fg(p.fg)
                    .bg(p.yellow.mix(p.bg, MATCH_FADE, Mix::Rgb)),
            )
            // Scrollbar marks take the full yellow as a foreground: a mark
            // under the thumb keeps its style but takes the thumb glyph, and a
            // block thumb glyph hides the background, so the text style's dark
            // foreground would turn the mark dark just when it slides under.
            .fg("/editor/search/mark", p.yellow)
            .style(
                "/editor/search/current",
                PartialStyle::new()
                    .fg(p.bg)
                    .bg(p.yellow)
                    .attrs(AttrSet::new(Attr::Bold)),
            )
            .fg("/editor/line-number", p.line_number)
            .fg("/editor/line-number/current", p.accent)
            .style(
                "/editor/prompt",
                PartialStyle::new().fg(p.fg).bg(p.panel_bg),
            )
            .style(
                "/help/panel",
                PartialStyle::new().fg(p.fg).bg(p.panel_bg),
            )
            // The status bar is chrome on the panel ground: a quiet label and an
            // accented key that names what the bar can do.
            .style(
                "/status_bar",
                PartialStyle::new().fg(p.muted_fg).bg(p.panel_bg),
            )
            // A notice reports a failure the application survived, so it takes
            // the error colour on the chrome ground of the row it covers.
            .style(
                "/root/notice",
                PartialStyle::new().fg(p.red).bg(p.panel_bg),
            )
            .style(
                "/status_bar/key",
                PartialStyle::new()
                    .fg(p.key)
                    .attrs(AttrSet::new(Attr::Bold)),
            )
            .style(
                "/help/key",
                PartialStyle::new()
                    .fg(p.key)
                    .bg(p.panel_bg)
                    .attrs(AttrSet::new(Attr::Bold)),
            )
            // The comma between keys reads as punctuation, not as part of a key.
            // The empty attribute set stops it inheriting the key's bold.
            .style(
                "/help/key/separator",
                PartialStyle::new()
                    .fg(p.fg)
                    .bg(p.panel_bg)
                    .attrs(AttrSet::default()),
            )
            .style_all(
                &["/help/label"],
                PartialStyle::new().fg(p.muted_fg).bg(p.panel_bg),
            )
            .style_all(
                &["/picker/background", "/picker/text"],
                PartialStyle::new().fg(p.fg).bg(p.panel_bg),
            )
            .style_all(
                &["/selection", "/picker/selection"],
                PartialStyle::new().fg(p.bg).bg(p.accent),
            )
            // While the filter takes keys the list is not what the keyboard drives,
            // so its selection holds its place without claiming the eye.
            .style_all(
                &["/selection/dimmed", "/picker/selection/dimmed"],
                PartialStyle::new().fg(p.fg).bg(p.selection_bg),
            )
            .style_all(
                &["/picker/placeholder", "/picker/muted"],
                PartialStyle::new().fg(p.muted_fg).bg(p.panel_bg),
            )
            // A field beside results, such as a picker's filter, takes the
            // element ground to set it apart from the rows. A field that has
            // been given stays legible without competing with the list that
            // the keyboard has gone back to.
            .style_all(
                &["/input/background", "/input/text", "/input/prompt"],
                PartialStyle::new().fg(p.muted_fg).bg(p.element_bg),
            )
            // Taking keys lights the field up, because it is what typing reaches.
            .style_all(
                &["/input/focused/background", "/input/focused/text"],
                PartialStyle::new().fg(p.fg).bg(p.selection_bg),
            )
            .style_all(
                &["/input/focused/prompt"],
                PartialStyle::new()
                    .fg(p.key)
                    .bg(p.selection_bg)
                    .attrs(AttrSet::new(Attr::Bold)),
            )
            // Every dialog is one panel surface: its ground, its frame, and
            // its buttons take the panel rather than the view behind it.
            .style_all(
                &["/dialog/background", "/confirm/message"],
                PartialStyle::new().fg(p.fg).bg(p.panel_bg),
            )
            .style_all(
                &[
                    "/dialog/frame",
                    "/dialog/frame/focused",
                    "/dialog/frame/thumb",
                    "/dialog/frame/thumb/active",
                    "/dialog/frame/title",
                ],
                PartialStyle::new().bg(p.panel_bg),
            )
            .style_all(
                &["/dialog/button/border", "/dialog/button/text"],
                PartialStyle::new().fg(p.fg).bg(p.panel_bg),
            )
            .style(
                "/dialog/button/focused/border",
                PartialStyle::new().fg(p.frame_focused).bg(p.panel_bg),
            )
            .style(
                "/dialog/button/key",
                PartialStyle::new()
                    .fg(p.key)
                    .bg(p.panel_bg)
                    .attrs(AttrSet::new(Attr::Bold)),
            )
            .apply();
        c
    }
}

/// Colours of the default theme.
mod default {
    use super::{Color, rgb};

    /// Default background.
    pub(super) const BG: Color = rgb!("#0a0a0a");
    /// Panel background: header and status bars, overlays.
    pub(super) const PANEL: Color = rgb!("#1a1a1a");
    /// Highlight background: raised elements and selections without focus.
    pub(super) const HIGHLIGHT: Color = rgb!("#282828");
    /// Dividers, faint rules, and the selection background.
    pub(super) const BORDER_SUBTLE: Color = rgb!("#3c3c3c");
    /// Frame borders.
    pub(super) const BORDER: Color = rgb!("#484848");
    /// Borders of the active frame.
    pub(super) const BORDER_ACTIVE: Color = rgb!("#606060");
    /// Muted text: comments, gutters, hints.
    pub(super) const MUTED: Color = rgb!("#808080");
    /// Secondary text: labels and bars.
    pub(super) const SUBTEXT: Color = rgb!("#b4b4b4");
    /// Default text.
    pub(super) const TEXT: Color = rgb!("#eeeeee");
    /// Signature accent: focus and selection.
    pub(super) const ACCENT: Color = rgb!("#8aadf4");
    /// Blue.
    pub(super) const BLUE: Color = rgb!("#5c9cf5");
    /// Violet.
    pub(super) const VIOLET: Color = rgb!("#bb9af7");
    /// Magenta.
    pub(super) const MAGENTA: Color = rgb!("#e58fd6");
    /// Cyan.
    pub(super) const CYAN: Color = rgb!("#56b6c2");
    /// Green.
    pub(super) const GREEN: Color = rgb!("#7fd88f");
    /// Yellow.
    pub(super) const YELLOW: Color = rgb!("#e5c07b");
    /// Orange.
    pub(super) const ORANGE: Color = rgb!("#f5a742");
    /// Red.
    pub(super) const RED: Color = rgb!("#e06c75");
}

/// The default theme: a neutral near-black ground, grey chrome, and a single
/// accent.
///
/// The grey ramp follows opencode's default theme, and the named colours come
/// from the One Dark and TokyoNight families that opencode and Grok Build draw
/// on.
pub fn default_dark() -> Palette {
    Palette {
        fg: default::TEXT,
        bg: default::BG,
        frame: default::BORDER,
        frame_focused: default::MUTED,
        frame_thumb: default::BORDER_ACTIVE,
        frame_title: default::TEXT,
        accent: default::ACCENT,
        muted_fg: default::SUBTEXT,
        faint_fg: default::MUTED,
        panel_bg: default::PANEL,
        element_bg: default::HIGHLIGHT,
        selection_bg: default::BORDER_SUBTLE,
        line_number: default::BORDER_ACTIVE,
        key: default::ACCENT,
        blue: default::BLUE,
        red: default::RED,
        magenta: default::MAGENTA,
        violet: default::VIOLET,
        cyan: default::CYAN,
        green: default::GREEN,
        yellow: default::YELLOW,
        orange: default::ORANGE,
    }
}

/// Colours of the dracula theme.
mod dracula {
    use super::{Color, rgb};

    /// Background.
    pub(super) const BACKGROUND: Color = rgb!("#282a36");
    /// Current line / selection background.
    pub(super) const CURRENT_LINE: Color = rgb!("#44475a");
    /// Selection.
    pub(super) const SELECTION: Color = rgb!("#44475a");
    /// Foreground.
    pub(super) const FOREGROUND: Color = rgb!("#f8f8f2");
    /// Comment color (also used for subtle elements).
    pub(super) const COMMENT: Color = rgb!("#6272a4");
    /// Red.
    pub(super) const RED: Color = rgb!("#ff5555");
    /// Orange.
    pub(super) const ORANGE: Color = rgb!("#ffb86c");
    /// Yellow.
    pub(super) const YELLOW: Color = rgb!("#f1fa8c");
    /// Green.
    pub(super) const GREEN: Color = rgb!("#50fa7b");
    /// Cyan.
    pub(super) const CYAN: Color = rgb!("#8be9fd");
    /// Purple.
    pub(super) const PURPLE: Color = rgb!("#bd93f9");
    /// Pink.
    pub(super) const PINK: Color = rgb!("#ff79c6");
}

/// The Dracula theme: <https://draculatheme.com>.
pub fn dracula() -> Palette {
    Palette {
        fg: dracula::FOREGROUND,
        bg: dracula::BACKGROUND,
        frame: dracula::COMMENT,
        frame_focused: dracula::PURPLE,
        frame_thumb: dracula::CYAN,
        frame_title: dracula::FOREGROUND,
        accent: dracula::PURPLE,
        muted_fg: dracula::FOREGROUND,
        faint_fg: dracula::COMMENT,
        panel_bg: dracula::CURRENT_LINE,
        element_bg: dracula::BACKGROUND,
        selection_bg: dracula::SELECTION,
        line_number: dracula::COMMENT,
        key: dracula::CYAN,
        blue: dracula::CYAN,
        red: dracula::RED,
        magenta: dracula::PINK,
        violet: dracula::PURPLE,
        cyan: dracula::CYAN,
        green: dracula::GREEN,
        yellow: dracula::YELLOW,
        orange: dracula::ORANGE,
    }
}

/// Colours of the gruvbox theme.
mod gruvbox {
    use super::{Color, rgb};

    /// Dark background (default).
    pub(super) const DARK0: Color = rgb!("#282828");
    /// Dark background 1.
    pub(super) const DARK1: Color = rgb!("#3c3836");
    /// Dark background 2.
    pub(super) const DARK2: Color = rgb!("#504945");
    /// Dark background 4.
    pub(super) const DARK4: Color = rgb!("#7c6f64");
    /// Light foreground 0.
    pub(super) const LIGHT0: Color = rgb!("#fbf1c7");
    /// Light foreground 1.
    pub(super) const LIGHT1: Color = rgb!("#ebdbb2");
    /// Light foreground 3.
    pub(super) const LIGHT3: Color = rgb!("#bdae93");
    /// Gray.
    pub(super) const GRAY: Color = rgb!("#928374");
    /// Bright red.
    pub(super) const RED: Color = rgb!("#fb4934");
    /// Bright green.
    pub(super) const GREEN: Color = rgb!("#b8bb26");
    /// Bright yellow.
    pub(super) const YELLOW: Color = rgb!("#fabd2f");
    /// Bright blue.
    pub(super) const BLUE: Color = rgb!("#83a598");
    /// Bright purple.
    pub(super) const PURPLE: Color = rgb!("#d3869b");
    /// Bright aqua/cyan.
    pub(super) const AQUA: Color = rgb!("#8ec07c");
    /// Bright orange.
    pub(super) const ORANGE: Color = rgb!("#fe8019");
}

/// The dark gruvbox theme by morhetz: <https://github.com/morhetz/gruvbox>.
pub fn gruvbox_dark() -> Palette {
    Palette {
        fg: gruvbox::LIGHT1,
        bg: gruvbox::DARK0,
        frame: gruvbox::DARK4,
        frame_focused: gruvbox::BLUE,
        frame_thumb: gruvbox::LIGHT3,
        frame_title: gruvbox::LIGHT0,
        accent: gruvbox::BLUE,
        muted_fg: gruvbox::LIGHT3,
        faint_fg: gruvbox::GRAY,
        panel_bg: gruvbox::DARK1,
        element_bg: gruvbox::DARK0,
        selection_bg: gruvbox::DARK2,
        line_number: gruvbox::GRAY,
        key: gruvbox::AQUA,
        blue: gruvbox::BLUE,
        red: gruvbox::RED,
        magenta: gruvbox::PURPLE,
        violet: gruvbox::PURPLE,
        cyan: gruvbox::AQUA,
        green: gruvbox::GREEN,
        yellow: gruvbox::YELLOW,
        orange: gruvbox::ORANGE,
    }
}

/// Colours of the solarized theme.
mod solarized {
    use super::{Color, rgb};

    /// Solarized base03.
    pub(super) const BASE03: Color = rgb!("#002b36");
    /// Solarized base02.
    pub(super) const BASE02: Color = rgb!("#073642");
    /// Solarized base01.
    pub(super) const BASE01: Color = rgb!("#586e75");
    /// Solarized base00.
    pub(super) const BASE00: Color = rgb!("#657b83");
    /// Solarized base0.
    pub(super) const BASE0: Color = rgb!("#839496");
    /// Solarized base1.
    pub(super) const BASE1: Color = rgb!("#93a1a1");
    /// Solarized base2.
    pub(super) const BASE2: Color = rgb!("#eee8d5");
    /// Solarized base3.
    pub(super) const BASE3: Color = rgb!("#fdf6e3");
    /// Solarized yellow.
    pub(super) const YELLOW: Color = rgb!("#b58900");
    /// Solarized orange.
    pub(super) const ORANGE: Color = rgb!("#cb4b16");
    /// Solarized red.
    pub(super) const RED: Color = rgb!("#dc322f");
    /// Solarized magenta.
    pub(super) const MAGENTA: Color = rgb!("#d33682");
    /// Solarized violet.
    pub(super) const VIOLET: Color = rgb!("#6c71c4");
    /// Solarized blue.
    pub(super) const BLUE: Color = rgb!("#268bd2");
    /// Solarized cyan.
    pub(super) const CYAN: Color = rgb!("#2aa198");
    /// Solarized green.
    pub(super) const GREEN: Color = rgb!("#859900");
}

/// The dark Solarized theme.
pub fn solarized_dark() -> Palette {
    Palette {
        fg: solarized::BASE0,
        bg: solarized::BASE03,
        frame: solarized::BASE01,
        frame_focused: solarized::BLUE,
        frame_thumb: solarized::BASE1,
        frame_title: solarized::BASE3,
        accent: solarized::BLUE,
        muted_fg: solarized::BASE1,
        faint_fg: solarized::BASE01,
        panel_bg: solarized::BASE02,
        element_bg: solarized::BASE03,
        selection_bg: solarized::BASE02,
        line_number: solarized::BASE01,
        key: solarized::CYAN,
        blue: solarized::BLUE,
        red: solarized::RED,
        magenta: solarized::MAGENTA,
        violet: solarized::VIOLET,
        cyan: solarized::CYAN,
        green: solarized::GREEN,
        yellow: solarized::YELLOW,
        orange: solarized::ORANGE,
    }
}

/// The light Solarized theme.
pub fn solarized_light() -> Palette {
    Palette {
        fg: solarized::BASE00,
        bg: solarized::BASE3,
        frame: solarized::BASE1,
        frame_focused: solarized::BLUE,
        frame_thumb: solarized::BASE01,
        frame_title: solarized::BASE03,
        accent: solarized::BLUE,
        muted_fg: solarized::BASE01,
        faint_fg: solarized::BASE1,
        panel_bg: solarized::BASE2,
        element_bg: solarized::BASE3,
        selection_bg: solarized::BASE2,
        line_number: solarized::BASE1,
        key: solarized::CYAN,
        blue: solarized::BLUE,
        red: solarized::RED,
        magenta: solarized::MAGENTA,
        violet: solarized::VIOLET,
        cyan: solarized::CYAN,
        green: solarized::GREEN,
        yellow: solarized::YELLOW,
        orange: solarized::ORANGE,
    }
}
