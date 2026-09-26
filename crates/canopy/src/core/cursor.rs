//! Soft cursors: roles, looks, and the declarations widgets make while they
//! render.
//!
//! A widget names what its cursor means with a role, such as
//! `cursor/vi/insert`. The theme decides how each role looks: a shape that
//! Canopy draws exactly on every cell, a color, and a motion.

use std::{
    borrow::Cow,
    collections::HashMap,
    time::{Duration, Instant},
};

use crate::{
    NodeId,
    geom::Point,
    style::{Color, Mix, animation::ease_in_out, themes::Palette},
};

/// The root cursor role, which every other role falls back to.
pub const CURSOR: &str = "cursor";
/// The cursor of editable text outside vi modes.
pub const TEXT: &str = "cursor/text";
/// The cursor of vi insert mode.
pub const VI_INSERT: &str = "cursor/vi/insert";
/// The cursor of vi normal mode.
pub const VI_NORMAL: &str = "cursor/vi/normal";
/// The cursor of vi visual mode.
pub const VI_VISUAL: &str = "cursor/vi/visual";
/// Where typing lands in a field that is active without focus.
pub const INACTIVE: &str = "cursor/inactive";
/// The cursor of a program that runs in an embedded terminal.
pub const TERMINAL: &str = "cursor/terminal";

/// How long a blinking cursor shows.
pub const BLINK_ON: Duration = Duration::from_millis(600);
/// How long a blinking cursor hides.
pub const BLINK_OFF: Duration = Duration::from_millis(400);
/// One period of a pulsing cursor.
pub const PULSE_PERIOD: Duration = Duration::from_millis(1200);

/// How much of the foreground an inactive cursor mixes into the background.
const INACTIVE_MIX: f32 = 0.35;

/// A cursor shape that Canopy draws exactly on every cell.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum CursorShape {
    /// The whole cell takes the cursor color, and the grapheme takes a
    /// contrasting color.
    Block,
    /// The grapheme and its underline take the cursor color.
    Underline,
}

/// How a cursor changes over time.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum CursorMotion {
    /// Always shown.
    Steady,
    /// Shown for `on`, then the cell below for `off`.
    Blink {
        /// Time the cursor shows.
        on: Duration,
        /// Time the cell below shows.
        off: Duration,
    },
    /// Fades between the cursor color and the cell below over one period.
    Pulse {
        /// Time of one fade out and back.
        period: Duration,
    },
}

impl CursorMotion {
    /// The default blink.
    pub const BLINK: Self = Self::Blink {
        on: BLINK_ON,
        off: BLINK_OFF,
    };

    /// The default pulse.
    pub const PULSE: Self = Self::Pulse {
        period: PULSE_PERIOD,
    };

    /// Return the phase `elapsed` after the motion starts: 0.0 shows the
    /// cursor, and 1.0 shows the cell below it.
    pub(crate) fn phase(self, elapsed: Duration) -> f32 {
        match self {
            Self::Steady => 0.0,
            Self::Blink { on, off } => {
                let cycle = (on + off).as_nanos();
                if cycle == 0 || elapsed.as_nanos() % cycle < on.as_nanos() {
                    0.0
                } else {
                    1.0
                }
            }
            Self::Pulse { period } => {
                let period = period.as_nanos();
                if period < 2 {
                    return 0.0;
                }
                let half = period / 2;
                let local = elapsed.as_nanos() % period;
                let run = if local <= half { local } else { period - local };
                ease_in_out(run as f32 / half as f32)
            }
        }
    }

    /// Return the next time the phase changes, for a motion that started at
    /// `start`.
    pub(crate) fn next_change(
        self,
        start: Instant,
        now: Instant,
        sample: Duration,
    ) -> Option<Instant> {
        match self {
            Self::Steady => None,
            Self::Blink { on, off } => {
                let cycle = (on + off).as_nanos();
                if cycle == 0 {
                    return None;
                }
                let elapsed = now.saturating_duration_since(start).as_nanos();
                let run = elapsed - elapsed % cycle;
                let local = elapsed % cycle;
                let next = if local < on.as_nanos() {
                    run + on.as_nanos()
                } else {
                    run + cycle
                };
                Some(start + Duration::from_nanos(u64::try_from(next).unwrap_or(u64::MAX)))
            }
            Self::Pulse { .. } => Some(now + sample),
        }
    }
}

/// How a cursor role looks.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct CursorLook {
    /// Shape of the cursor.
    pub shape: CursorShape,
    /// Color of the cursor.
    pub color: Color,
    /// Change of the cursor over time.
    pub motion: CursorMotion,
}

impl CursorLook {
    /// Construct a look.
    pub const fn new(shape: CursorShape, color: Color, motion: CursorMotion) -> Self {
        Self {
            shape,
            color,
            motion,
        }
    }

    /// A blinking block in `color`.
    pub const fn blinking_block(color: Color) -> Self {
        Self::new(CursorShape::Block, color, CursorMotion::BLINK)
    }

    /// A steady block in `color`.
    pub const fn steady_block(color: Color) -> Self {
        Self::new(CursorShape::Block, color, CursorMotion::Steady)
    }
}

/// A cursor that a widget declares while it renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorRequest {
    /// Role of the cursor.
    pub role: Cow<'static, str>,
    /// A color that replaces the color of the role, such as the cursor color
    /// that a child terminal program sets.
    pub color: Option<Color>,
    /// A motion that replaces the motion of the role.
    pub motion: Option<CursorMotion>,
    /// A shape that replaces the shape of the role.
    pub shape: Option<CursorShape>,
}

impl CursorRequest {
    /// Request a cursor in `role`, with the look of the role.
    pub fn new(role: impl Into<Cow<'static, str>>) -> Self {
        Self {
            role: role.into(),
            color: None,
            motion: None,
            shape: None,
        }
    }

    /// Replace the color of the role.
    #[must_use]
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Replace the motion of the role.
    #[must_use]
    pub fn with_motion(mut self, motion: CursorMotion) -> Self {
        self.motion = Some(motion);
        self
    }

    /// Replace the shape of the role.
    #[must_use]
    pub fn with_shape(mut self, shape: CursorShape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Apply the replacements of this request to the look of its role.
    pub(crate) fn apply(&self, look: CursorLook) -> CursorLook {
        CursorLook {
            shape: self.shape.unwrap_or(look.shape),
            color: self.color.unwrap_or(look.color),
            motion: self.motion.unwrap_or(look.motion),
        }
    }
}

/// A replacement for parts of a role's look, kept across theme switches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CursorLookPatch {
    /// Replacement shape.
    pub shape: Option<CursorShape>,
    /// Replacement color.
    pub color: Option<Color>,
    /// Replacement motion.
    pub motion: Option<CursorMotion>,
}

impl CursorLookPatch {
    /// Apply this patch to a look.
    fn apply(self, look: CursorLook) -> CursorLook {
        CursorLook {
            shape: self.shape.unwrap_or(look.shape),
            color: self.color.unwrap_or(look.color),
            motion: self.motion.unwrap_or(look.motion),
        }
    }
}

/// The look of each cursor role.
///
/// A role resolves with the fallback of style paths: `cursor/vi/insert` falls
/// back to `cursor/vi`, then to `cursor`.
#[derive(Debug, Clone, PartialEq)]
pub struct CursorLooks {
    /// Looks keyed by canonical role.
    looks: HashMap<String, CursorLook>,
}

impl Default for CursorLooks {
    fn default() -> Self {
        let mut looks = Self {
            looks: HashMap::new(),
        };
        looks.set(CURSOR, CursorLook::blinking_block(Color::White));
        looks
    }
}

impl CursorLooks {
    /// Return the built-in looks for a palette.
    pub fn for_palette(p: &Palette) -> Self {
        let mut looks = Self::default();
        looks.set(CURSOR, CursorLook::blinking_block(p.fg));
        looks.set(TEXT, CursorLook::blinking_block(p.fg));
        looks.set(VI_INSERT, CursorLook::blinking_block(p.green));
        looks.set(VI_NORMAL, CursorLook::steady_block(p.blue));
        looks.set(VI_VISUAL, CursorLook::steady_block(p.magenta));
        looks.set(
            INACTIVE,
            CursorLook::steady_block(p.bg.mix(p.fg, INACTIVE_MIX, Mix::Oklab)),
        );
        looks.set(TERMINAL, CursorLook::blinking_block(p.fg));
        looks
    }

    /// Set the look of a role.
    pub fn set(&mut self, role: &str, look: CursorLook) {
        self.looks.insert(canonical_role(role), look);
    }

    /// Return the look set for exactly this role, without fallback.
    pub fn get(&self, role: &str) -> Option<CursorLook> {
        self.looks.get(&canonical_role(role)).copied()
    }

    /// Resolve the look of a role, falling back to its parent roles.
    pub fn resolve(&self, role: &str) -> CursorLook {
        let mut role = canonical_role(role);
        loop {
            if let Some(look) = self.looks.get(&role) {
                return *look;
            }
            match role.rfind('/') {
                Some(end) => role.truncate(end),
                None if role != CURSOR => role = CURSOR.to_string(),
                None => return CursorLook::blinking_block(Color::White),
            }
        }
    }

    /// Apply a patch over the resolved look of a role.
    pub(crate) fn patch(&mut self, role: &str, patch: CursorLookPatch) {
        let look = patch.apply(self.resolve(role));
        self.set(role, look);
    }
}

/// Return the canonical key of a role: non-empty components joined by `/`.
fn canonical_role(role: &str) -> String {
    role.split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Rule sets reapplied to the cursor looks after every theme switch.
pub(crate) type CursorRules = Box<dyn Fn(&Palette, &mut CursorLooks)>;

/// One cursor in a published frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorSnapshot {
    /// Node that declared the cursor.
    pub node: NodeId,
    /// Screen location of the cursor.
    pub location: Point,
    /// Role of the cursor.
    pub role: String,
    /// Look the frame painted, after request replacements and style effects.
    pub look: CursorLook,
    /// Whether this is the primary cursor: the declaration of the deepest node
    /// on the focus path.
    pub primary: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_fall_back_to_their_parents() {
        let mut looks = CursorLooks::default();
        let vi = CursorLook::steady_block(Color::Blue);
        looks.set("cursor/vi", vi);
        assert_eq!(looks.resolve("cursor/vi/insert"), vi);
        assert_eq!(looks.resolve("/cursor//vi/"), vi);
        assert_eq!(
            looks.resolve("cursor/text"),
            CursorLook::blinking_block(Color::White)
        );
        assert_eq!(
            looks.resolve("elsewhere"),
            CursorLook::blinking_block(Color::White)
        );
    }

    #[test]
    fn patches_replace_parts_of_the_resolved_look() {
        let mut looks = CursorLooks::default();
        looks.patch(
            VI_INSERT,
            CursorLookPatch {
                color: Some(Color::Green),
                ..CursorLookPatch::default()
            },
        );
        assert_eq!(
            looks.get(VI_INSERT),
            Some(CursorLook::blinking_block(Color::Green))
        );
    }

    #[test]
    fn a_blink_shows_then_hides_and_restarts_from_its_start() {
        let blink = CursorMotion::BLINK;
        let ms = Duration::from_millis;
        assert!(blink.phase(ms(0)) < f32::EPSILON);
        assert!(blink.phase(ms(599)) < f32::EPSILON);
        assert!((blink.phase(ms(600)) - 1.0).abs() < f32::EPSILON);
        assert!(blink.phase(ms(1000)) < f32::EPSILON);
        let t0 = Instant::now();
        let sample = ms(33);
        assert_eq!(blink.next_change(t0, t0, sample), Some(t0 + ms(600)));
        assert_eq!(
            blink.next_change(t0, t0 + ms(600), sample),
            Some(t0 + ms(1000))
        );
        assert_eq!(CursorMotion::Steady.next_change(t0, t0, sample), None);
    }

    #[test]
    fn a_pulse_fades_out_and_back() {
        let pulse = CursorMotion::PULSE;
        let ms = Duration::from_millis;
        assert!(pulse.phase(ms(0)) < f32::EPSILON);
        assert!((pulse.phase(ms(600)) - 1.0).abs() < f32::EPSILON);
        assert!(pulse.phase(ms(1200)) < f32::EPSILON);
        let quarter = pulse.phase(ms(300));
        assert!((0.4..0.6).contains(&quarter), "quarter phase {quarter}");
    }

    #[test]
    fn requests_replace_parts_of_the_look() {
        let look = CursorLook::blinking_block(Color::White);
        let request = CursorRequest::new(TERMINAL)
            .with_color(Color::Red)
            .with_motion(CursorMotion::Steady);
        assert_eq!(request.apply(look), CursorLook::steady_block(Color::Red));
    }
}
