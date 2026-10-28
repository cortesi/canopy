//! Formats of values for display: ages, counts, sizes, durations, and paths
//! below a home directory.
//!
//! Each function is pure and holds no widget state. A month is 30 days and a
//! year is 365 days.

use std::{
    env, iter,
    path::{Path, PathBuf},
    time::Duration,
};

/// Seconds in one minute.
const MINUTE: u64 = 60;
/// Seconds in one hour.
const HOUR: u64 = 60 * MINUTE;
/// Seconds in one day.
const DAY: u64 = 24 * HOUR;
/// Seconds in one week.
const WEEK: u64 = 7 * DAY;
/// Seconds in one month.
const MONTH: u64 = 30 * DAY;
/// Seconds in one year.
const YEAR: u64 = 365 * DAY;

/// Unit names of an age.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgeUnits {
    /// `45s`, `5m`, `3h`, `2d`, `4mo`, `1y`.
    Short,
    /// `45 seconds`, `12 min`, `1 hour`, `3 days`, `2 weeks`, `4 months`,
    /// `1 year`. Every unit except `min` takes a plural `s`.
    Long,
}

impl AgeUnits {
    /// Return `count` of the unit `unit` seconds long.
    fn name(self, count: u64, unit: u64) -> String {
        match self {
            Self::Short => {
                let name = match unit {
                    1 => "s",
                    MINUTE => "m",
                    HOUR => "h",
                    DAY => "d",
                    MONTH => "mo",
                    _ => "y",
                };
                format!("{count}{name}")
            }
            Self::Long => {
                let name = match unit {
                    1 => "second",
                    MINUTE => return format!("{count} min"),
                    HOUR => "hour",
                    DAY => "day",
                    WEEK => "week",
                    MONTH => "month",
                    _ => "year",
                };
                let plural = if count == 1 { "" } else { "s" };
                format!("{count} {name}{plural}")
            }
        }
    }
}

/// How an age reads. Each field is public, so a host builds the form it
/// needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgeFormat {
    /// Unit names.
    pub units: AgeUnits,
    /// Show seconds below one minute, such as `45s`. Else an age below one
    /// minute is `now`.
    pub seconds: bool,
    /// Use weeks between one week and one month. Only `Long` units have
    /// weeks.
    pub weeks: bool,
    /// Text for an age below one minute when `seconds` is off.
    pub now: &'static str,
    /// Text after every age that is not `now`, such as ` ago`.
    pub suffix: &'static str,
}

impl AgeFormat {
    /// `now`, `5m`, `3h`, `2d`, `4mo`, `1y`: the ages of file listings.
    pub const COMPACT: Self = Self {
        units: AgeUnits::Short,
        seconds: false,
        weeks: false,
        now: "now",
        suffix: "",
    };
    /// `59s`, `1m`, `1h`, `1d`, `4mo`: the ages of the notice log.
    pub const SECONDS: Self = Self {
        seconds: true,
        ..Self::COMPACT
    };
    /// `just now`, `12 min ago`, `1 hour ago`, `2 weeks ago`: ages in prose.
    pub const PROSE: Self = Self {
        units: AgeUnits::Long,
        seconds: false,
        weeks: true,
        now: "just now",
        suffix: " ago",
    };

    /// Return this form with `suffix`, such as
    /// `AgeFormat::COMPACT.with_suffix(" ago")`.
    #[must_use]
    pub const fn with_suffix(self, suffix: &'static str) -> Self {
        Self { suffix, ..self }
    }

    /// Format `age`.
    #[must_use]
    pub fn format(&self, age: Duration) -> String {
        let seconds = age.as_secs();
        if seconds < MINUTE && !self.seconds {
            return self.now.to_owned();
        }
        let (unit, _) = self.step(seconds);
        format!("{}{}", self.units.name(seconds / unit, unit), self.suffix)
    }

    /// Return the time until the text of `age` changes, so a view repaints
    /// only then.
    #[must_use]
    pub fn next_change(&self, age: Duration) -> Duration {
        let (unit, end) = self.step(age.as_secs());
        let unit = Duration::from_secs(unit);
        let into = u64::try_from(age.as_nanos() % unit.as_nanos()).unwrap_or(0);
        let next_count = unit - Duration::from_nanos(into);
        let next_step = Duration::from_secs(end).saturating_sub(age);
        next_count.min(next_step)
    }

    /// Return the unit an age of `seconds` counts in, and the age in seconds
    /// where the next unit starts.
    fn step(&self, seconds: u64) -> (u64, u64) {
        let weeks = self.weeks && self.units == AgeUnits::Long;
        match seconds {
            0..MINUTE if self.seconds => (1, MINUTE),
            0..MINUTE => (MINUTE, MINUTE),
            MINUTE..HOUR => (MINUTE, HOUR),
            HOUR..DAY => (HOUR, DAY),
            DAY..WEEK if weeks => (DAY, WEEK),
            WEEK..MONTH if weeks => (WEEK, MONTH),
            DAY..MONTH => (DAY, MONTH),
            MONTH..YEAR => (MONTH, YEAR),
            _ => (YEAR, u64::MAX),
        }
    }
}

/// Units of a count above one thousand, each a thousand times the one
/// before.
const COUNT_UNITS: [&str; 4] = ["k", "M", "B", "T"];

/// Return a count with three significant digits, in units of 1000: `987`,
/// `1.23k`, `12.3k`, `123k`, `1.00M`.
#[must_use]
pub fn count(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count shows three significant digits"
    )]
    let mut value = n as f64;
    for unit in COUNT_UNITS {
        value /= 1_000.0;
        // A value that rounds to 1,000 in this unit shows in the next one.
        if value < 999.5 || unit == COUNT_UNITS[COUNT_UNITS.len() - 1] {
            return if value < 9.995 {
                format!("{value:.2}{unit}")
            } else if value < 99.95 {
                format!("{value:.1}{unit}")
            } else {
                format!("{value:.0}{unit}")
            };
        }
    }
    unreachable!("the last unit always returns")
}

/// Return every digit of `n`, in groups of three: `3,084,112`.
#[must_use]
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}

/// Unit suffixes of a size, each 1024 times the one before.
const SIZE_UNITS: [&str; 7] = ["", "k", "m", "g", "t", "p", "e"];

/// Return a size in units of 1024, with one decimal below ten: `812`, `3.4k`,
/// `12k`, `1.0m`, `2.0g`. The text never exceeds five columns.
#[must_use]
pub fn bytes(n: u64) -> String {
    let mut unit = 0;
    while unit + 1 < SIZE_UNITS.len() && n >= 1_u64 << (10 * (unit + 1)) {
        unit += 1;
    }
    if unit == 0 {
        return n.to_string();
    }
    let base = 1_u64 << (10 * unit);
    let suffix = SIZE_UNITS[unit];
    let whole = n / base;
    let remainder = n % base;
    if whole < 10 {
        let tenths = (remainder * 10 + base / 2) / base;
        let (whole, tenths) = if tenths == 10 {
            (whole + 1, 0)
        } else {
            (whole, tenths)
        };
        format!("{whole}.{tenths}{suffix}")
    } else {
        let rounded = whole + u64::from(remainder >= base / 2);
        format!("{rounded}{suffix}")
    }
}

/// Return a duration in its largest units: `850 ms`, `7.2 s`, `48 s`,
/// `1 min 42 s`, `1 h 12 min`.
#[must_use]
pub fn duration(d: Duration) -> String {
    let millis = u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
    if millis < 1_000 {
        return format!("{millis} ms");
    }
    if millis < 9_950 {
        // Tenths of a second, rounded.
        let tenths = (millis + 50) / 100;
        return format!("{}.{} s", tenths / 10, tenths % 10);
    }
    // Whole seconds, rounded without overflowing the millisecond count.
    let seconds = millis / 1_000 + u64::from(millis % 1_000 >= 500);
    if seconds < MINUTE {
        return format!("{seconds} s");
    }
    if seconds < HOUR {
        let (minutes, rest) = (seconds / MINUTE, seconds % MINUTE);
        return if rest == 0 {
            format!("{minutes} min")
        } else {
            format!("{minutes} min {rest} s")
        };
    }
    let minutes = (seconds + 30) / MINUTE;
    let (hours, rest) = (minutes / 60, minutes % 60);
    if rest == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {rest} min")
    }
}

/// A home directory, for showing paths below it with `~`.
///
/// A home matches both its path as given and its canonical form, so a path
/// through a symbolic link to the home shows with `~` too.
#[derive(Clone, Debug)]
pub struct Home {
    /// The path as given and, when it differs, its canonical form. Empty for
    /// a home that matches nothing.
    forms: Vec<PathBuf>,
}

impl Home {
    /// Use `path` as the home. It resolves the canonical form once, here. A
    /// relative path, or `/`, gives a home that matches nothing.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let usable = |path: &Path| path.is_absolute() && path.parent().is_some();
        if !usable(&path) {
            return Self { forms: Vec::new() };
        }
        let canonical = path
            .canonicalize()
            .ok()
            .filter(|canonical| *canonical != path && usable(canonical));
        Self {
            forms: iter::once(path).chain(canonical).collect(),
        }
    }

    /// The home of the `HOME` variable, or a home that matches nothing.
    #[must_use]
    pub fn from_env() -> Self {
        Self::new(env::var_os("HOME").unwrap_or_default())
    }

    /// Return `path` with the home as `~`, such as `~/src/fh`.
    #[must_use]
    pub fn path(&self, path: &Path) -> String {
        for home in &self.forms {
            if let Ok(rest) = path.strip_prefix(home) {
                return if rest.as_os_str().is_empty() {
                    "~".to_owned()
                } else {
                    format!("~/{}", rest.display())
                };
            }
        }
        path.display().to_string()
    }

    /// Return `text` with each path below the home shown with `~`, at path
    /// boundaries only.
    ///
    /// A path of the home starts at the start of the text or after a
    /// character that ends a path, and it continues with `/` or ends. ANSI
    /// control sequences, as in colored command-line output, count as no
    /// characters and stay in the text.
    #[must_use]
    pub fn text(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for home in &self.forms {
            text = replace_home(&text, &home.display().to_string());
        }
        text
    }
}

/// Replace each path of `home` in `text` with `~`.
fn replace_home(text: &str, home: &str) -> String {
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find(home) {
        let (before, after) = rest.split_at(index);
        let after = &after[home.len()..];
        let starts = visible_chars(before)
            .last()
            .or_else(|| visible_chars(&out).last())
            .is_none_or(ends_path);
        let mut following = visible_chars(after);
        let ends = match following.next() {
            None | Some('/') => true,
            Some('.') => following.next().is_none_or(char::is_whitespace),
            Some(next) => ends_path(next),
        };
        out.push_str(before);
        out.push_str(if starts && ends { "~" } else { home });
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Yield the visible characters of `text`, skipping ANSI CSI sequences.
fn visible_chars(text: &str) -> impl Iterator<Item = char> + '_ {
    let mut chars = text.chars().peekable();
    iter::from_fn(move || {
        loop {
            let character = chars.next()?;
            if character == '\u{1b}' && chars.peek() == Some(&'[') {
                let _ = chars.next();
                for code in chars.by_ref() {
                    if ('@'..='~').contains(&code) {
                        break;
                    }
                }
                continue;
            }
            return Some(character);
        }
    })
}

/// Return whether `character` ends a path in text: space, a quote, a
/// bracket, or punctuation that no path in a message holds.
fn ends_path(character: char) -> bool {
    character.is_whitespace()
        || matches!(
            character,
            '"' | '\''
                | '`'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '<'
                | '>'
                | ','
                | ';'
                | ':'
                | '='
        )
}

#[cfg(test)]
mod tests;
