//! Tests of the display formats.

use std::io;

use super::*;

/// Return `format` of an age of `seconds`.
fn age(format: AgeFormat, seconds: u64) -> String {
    format.format(Duration::from_secs(seconds))
}

#[test]
fn compact_ages_step_through_minutes_days_months_and_years() {
    let ago = |seconds| age(AgeFormat::COMPACT, seconds);
    assert_eq!(ago(0), "now");
    assert_eq!(ago(59), "now");
    assert_eq!(ago(60), "1m");
    assert_eq!(ago(59 * 60), "59m");
    assert_eq!(ago(HOUR), "1h");
    assert_eq!(ago(DAY), "1d");
    assert_eq!(ago(8 * DAY), "8d", "short units have no weeks");
    assert_eq!(ago(29 * DAY), "29d");
    assert_eq!(ago(30 * DAY), "1mo");
    assert_eq!(ago(364 * DAY), "12mo");
    assert_eq!(ago(365 * DAY), "1y");
    assert_eq!(ago(3 * YEAR), "3y");
}

#[test]
fn a_suffix_follows_every_age_but_now() {
    let ago = |seconds| age(AgeFormat::COMPACT.with_suffix(" ago"), seconds);
    assert_eq!(ago(10), "now");
    assert_eq!(ago(3 * DAY), "3d ago");
    assert_eq!(ago(5 * MONTH), "5mo ago");
}

#[test]
fn notice_ages_show_seconds() {
    let ago = |seconds| age(AgeFormat::SECONDS, seconds);
    assert_eq!(ago(0), "0s");
    assert_eq!(ago(59), "59s");
    assert_eq!(ago(61), "1m");
    assert_eq!(ago(3_599), "59m");
    assert_eq!(ago(HOUR), "1h");
    assert_eq!(ago(90_000), "1d");
    assert_eq!(ago(4 * MONTH), "4mo");
}

#[test]
fn prose_ages_read_as_words() {
    let ago = |seconds| age(AgeFormat::PROSE, seconds);
    assert_eq!(ago(5), "just now");
    assert_eq!(ago(MINUTE), "1 min ago");
    assert_eq!(ago(12 * MINUTE + 30), "12 min ago");
    assert_eq!(ago(HOUR), "1 hour ago");
    assert_eq!(ago(5 * HOUR), "5 hours ago");
    assert_eq!(ago(DAY + HOUR), "1 day ago");
    assert_eq!(ago(3 * DAY), "3 days ago");
    assert_eq!(ago(2 * WEEK), "2 weeks ago");
    assert_eq!(ago(4 * MONTH), "4 months ago");
    assert_eq!(ago(2 * YEAR), "2 years ago");
    let seconds = AgeFormat {
        seconds: true,
        ..AgeFormat::PROSE
    };
    assert_eq!(age(seconds, 1), "1 second ago");
    assert_eq!(age(seconds, 45), "45 seconds ago");
}

#[test]
fn the_next_change_is_the_next_count_or_the_next_unit() {
    let next = |format: AgeFormat, age: Duration| format.next_change(age);
    let secs = Duration::from_secs;
    let millis = Duration::from_millis;
    assert_eq!(next(AgeFormat::SECONDS, millis(250)), millis(750));
    assert_eq!(next(AgeFormat::SECONDS, millis(1_200)), millis(800));
    assert_eq!(next(AgeFormat::SECONDS, secs(61)), secs(59));
    assert_eq!(next(AgeFormat::SECONDS, secs(59)), secs(1));
    assert_eq!(next(AgeFormat::SECONDS, secs(90)), secs(30));
    assert_eq!(next(AgeFormat::SECONDS, secs(HOUR + 10)), secs(HOUR - 10));
    // Without seconds, `now` holds until the first minute.
    assert_eq!(next(AgeFormat::COMPACT, secs(15)), secs(45));
    // Weeks do not divide a month, so the month starts before the next week.
    assert_eq!(
        next(AgeFormat::PROSE, secs(4 * WEEK)),
        secs(MONTH - 4 * WEEK)
    );
    // Months do not divide a year.
    assert_eq!(
        next(AgeFormat::COMPACT, secs(12 * MONTH)),
        secs(YEAR - 12 * MONTH)
    );
    assert_eq!(next(AgeFormat::COMPACT, secs(YEAR + 5)), secs(YEAR - 5));
    // Each change lands on a new text.
    for format in [AgeFormat::SECONDS, AgeFormat::COMPACT, AgeFormat::PROSE] {
        for seconds in [0, 59, 61, 3_599, 86_399, 29 * DAY, 31 * DAY, 364 * DAY] {
            let at = secs(seconds) + millis(300);
            let change = at + next(format, at);
            assert_ne!(
                format.format(at),
                format.format(change),
                "{format:?} {seconds}"
            );
            assert_eq!(
                format.format(at),
                format.format(change - millis(1)),
                "{format:?} {seconds}"
            );
        }
    }
}

#[test]
fn a_count_has_three_significant_digits() {
    let cases = [
        (0, "0"),
        (987, "987"),
        (999, "999"),
        (1_000, "1.00k"),
        (1_234, "1.23k"),
        (9_994, "9.99k"),
        (9_995, "10.0k"),
        (12_345, "12.3k"),
        (99_949, "99.9k"),
        (99_950, "100k"),
        (123_456, "123k"),
        (999_499, "999k"),
        (999_500, "1.00M"),
        (1_250_000, "1.25M"),
        (4_270_000, "4.27M"),
        (272_000, "272k"),
        (3_084_112_000, "3.08B"),
        (u64::MAX, "18446744T"),
    ];
    for (n, text) in cases {
        assert_eq!(count(n), text, "{n}");
    }
}

#[test]
fn a_grouped_count_groups_digits_in_threes() {
    assert_eq!(grouped(0), "0");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1_000), "1,000");
    assert_eq!(grouped(3_084_112), "3,084,112");
    assert_eq!(grouped(u64::MAX), "18,446,744,073,709,551,615");
}

#[test]
fn sizes_count_binary_units_with_one_decimal_below_ten() {
    assert_eq!(bytes(0), "0");
    assert_eq!(bytes(812), "812");
    assert_eq!(bytes(1023), "1023");
    assert_eq!(bytes(1024), "1.0k");
    assert_eq!(bytes(3_481), "3.4k");
    assert_eq!(bytes(12_288), "12k");
    assert_eq!(bytes(1024 * 1024), "1.0m");
    assert_eq!(bytes(2 * 1024 * 1024 * 1024), "2.0g");
    assert_eq!(bytes(u64::MAX), "16e");
}

#[test]
fn a_duration_names_its_largest_units() {
    let cases = [
        (0, "0 ms"),
        (850, "850 ms"),
        (999, "999 ms"),
        (1_000, "1.0 s"),
        (7_249, "7.2 s"),
        (9_949, "9.9 s"),
        (9_950, "10 s"),
        (48_000, "48 s"),
        (59_499, "59 s"),
        (59_500, "1 min"),
        (102_000, "1 min 42 s"),
        (3_599_000, "59 min 59 s"),
        (3_600_000, "1 h"),
        (4_320_000, "1 h 12 min"),
        (90_000_000, "25 h"),
        (u64::MAX, "5124095576030 h 26 min"),
    ];
    for (millis, text) in cases {
        assert_eq!(duration(Duration::from_millis(millis)), text, "{millis}");
    }
}

#[test]
fn a_path_below_home_starts_with_a_tilde() {
    let home = Home::new("/home/ana");
    assert_eq!(
        home.path(Path::new("/home/ana/.verber/db.sql")),
        "~/.verber/db.sql"
    );
    assert_eq!(home.path(Path::new("/home/ana")), "~");
    assert_eq!(home.path(Path::new("/home/anabel/x")), "/home/anabel/x");
    assert_eq!(home.path(Path::new("/srv/x")), "/srv/x");
}

#[test]
fn the_root_and_relative_paths_are_no_home() {
    for home in [Home::new("/"), Home::new("relative"), Home::new("")] {
        assert_eq!(home.path(Path::new("/srv/x")), "/srv/x");
        assert_eq!(home.path(Path::new("relative/x")), "relative/x");
        assert_eq!(home.text("/srv/x and relative/x"), "/srv/x and relative/x");
    }
}

#[cfg(unix)]
#[test]
fn a_symlinked_home_matches_both_forms() -> io::Result<()> {
    use std::{fs, os::unix::fs::symlink};

    let dir = tempfile::tempdir()?;
    let real = dir.path().join("real");
    fs::create_dir(&real)?;
    let link = dir.path().join("link");
    symlink(&real, &link)?;
    let home = Home::new(&link);
    let real = real.canonicalize()?;
    assert_eq!(home.path(&link.join("src")), "~/src");
    assert_eq!(home.path(&real.join("src")), "~/src");
    assert_eq!(
        home.text(&format!(
            "{} and {}",
            link.display(),
            real.join("a").display()
        )),
        "~ and ~/a"
    );
    Ok(())
}

#[test]
fn paths_of_home_in_text_start_with_a_tilde() {
    let home = Home::new("/home/ana");
    let shown = |message: &str| home.text(message);
    assert_eq!(
        shown("cannot open /home/ana/.verber/db.sql: denied"),
        "cannot open ~/.verber/db.sql: denied"
    );
    assert_eq!(shown("root `/home/ana/work`"), "root `~/work`");
    assert_eq!(shown("in /home/ana."), "in ~.");
    assert_eq!(shown("/home/ana/a and /home/ana/b"), "~/a and ~/b");
    // A longer name, or a path that only contains the home, stays.
    assert_eq!(shown("/home/anabel/a"), "/home/anabel/a");
    assert_eq!(shown("/srv/home/ana/a"), "/srv/home/ana/a");
    assert_eq!(shown("/home/ana.old/a"), "/home/ana.old/a");
    assert_eq!(shown("/home/ana/home/ana"), "~/home/ana");
    assert_eq!(shown("no path"), "no path");
}

#[test]
fn colored_text_keeps_its_styles_and_path_boundaries() {
    let home = Home::new("/home/ana");
    assert_eq!(
        home.text("error: \u{1b}[31m/home/ana/file\u{1b}[0m"),
        "error: \u{1b}[31m~/file\u{1b}[0m"
    );
    assert_eq!(
        home.text("\u{1b}[31m/home/ana\u{1b}[0m: denied"),
        "\u{1b}[31m~\u{1b}[0m: denied"
    );
    assert_eq!(
        home.text("prefix\u{1b}[31m/home/ana/file"),
        "prefix\u{1b}[31m/home/ana/file"
    );
    assert_eq!(
        home.text("/home/ana\u{1b}[31m.old/file"),
        "/home/ana\u{1b}[31m.old/file"
    );
}
