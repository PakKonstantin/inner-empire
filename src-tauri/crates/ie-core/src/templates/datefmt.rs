//! A small date-format language.
//!
//! Users write `YYYY-MM-DD` for a daily-note filename, not `%Y-%m-%d`. The
//! tokens below are the familiar ones from spreadsheet and note-taking tools,
//! implemented directly so the format string in settings means exactly what it
//! looks like and nothing else is reachable from it.
//!
//! ```text
//! YYYY  2026    MMMM  September   dddd  Thursday    HH  14
//! YY    26      MMM   Sep         ddd   Thu         mm  05
//! MM    09      DD    17          ww    38          ss  09
//! ```

use time::OffsetDateTime;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// Expand a format string against an instant.
///
/// Anything in single quotes is literal, so `'Week' ww` yields `Week 38`
/// rather than `Wednesdayeek 38`.
pub fn format(pattern: &str, at: OffsetDateTime) -> String {
    let mut out = String::with_capacity(pattern.len() + 8);
    let chars: Vec<char> = pattern.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '\'' {
            // Literal run. `''` is an escaped quote.
            index += 1;
            while index < chars.len() && chars[index] != '\'' {
                out.push(chars[index]);
                index += 1;
            }
            index += 1;
            continue;
        }

        let run_char = chars[index];
        let mut run = 0;
        while index + run < chars.len() && chars[index + run] == run_char {
            run += 1;
        }

        match (run_char, run) {
            ('Y', 4) => out.push_str(&format!("{:04}", at.year())),
            ('Y', 2) => out.push_str(&format!("{:02}", at.year() % 100)),
            ('M', 4) => out.push_str(MONTHS[at.month() as usize - 1]),
            ('M', 3) => out.push_str(&MONTHS[at.month() as usize - 1][..3]),
            ('M', 2) => out.push_str(&format!("{:02}", at.month() as u8)),
            ('M', 1) => out.push_str(&format!("{}", at.month() as u8)),
            ('D', 2) => out.push_str(&format!("{:02}", at.day())),
            ('D', 1) => out.push_str(&format!("{}", at.day())),
            ('d', 4) => out.push_str(WEEKDAYS[at.weekday().number_days_from_monday() as usize]),
            ('d', 3) => {
                out.push_str(&WEEKDAYS[at.weekday().number_days_from_monday() as usize][..3])
            }
            ('H', 2) => out.push_str(&format!("{:02}", at.hour())),
            ('H', 1) => out.push_str(&format!("{}", at.hour())),
            ('m', 2) => out.push_str(&format!("{:02}", at.minute())),
            ('m', 1) => out.push_str(&format!("{}", at.minute())),
            ('s', 2) => out.push_str(&format!("{:02}", at.second())),
            ('s', 1) => out.push_str(&format!("{}", at.second())),
            ('w', 2) => out.push_str(&format!("{:02}", at.iso_week())),
            _ => {
                for _ in 0..run {
                    out.push(run_char);
                }
            }
        }
        index += run;
    }

    out
}

/// Build an instant from epoch milliseconds and a UTC offset in seconds.
pub fn at(now_ms: i64, offset_seconds: i32) -> OffsetDateTime {
    let utc = OffsetDateTime::from_unix_timestamp_nanos((now_ms as i128) * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    let offset = time::UtcOffset::from_whole_seconds(offset_seconds)
        .unwrap_or(time::UtcOffset::UTC);
    utc.to_offset(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-17T14:05:09Z, a Thursday in ISO week 38.
    fn sample() -> OffsetDateTime {
        at(1_789_653_909_000, 0)
    }

    #[test]
    fn the_sample_instant_is_what_the_other_tests_assume() {
        let t = sample();
        assert_eq!((t.year(), t.month() as u8, t.day()), (2026, 9, 17));
        assert_eq!((t.hour(), t.minute(), t.second()), (14, 5, 9));
    }

    #[test]
    fn the_default_daily_note_format_produces_an_iso_date() {
        assert_eq!(format("YYYY-MM-DD", sample()), "2026-09-17");
    }

    #[test]
    fn every_token_expands() {
        let t = sample();
        assert_eq!(format("YYYY", t), "2026");
        assert_eq!(format("YY", t), "26");
        assert_eq!(format("MMMM", t), "September");
        assert_eq!(format("MMM", t), "Sep");
        assert_eq!(format("MM", t), "09");
        assert_eq!(format("M", t), "9");
        assert_eq!(format("DD", t), "17");
        assert_eq!(format("D", t), "17");
        assert_eq!(format("dddd", t), "Thursday");
        assert_eq!(format("ddd", t), "Thu");
        assert_eq!(format("HH:mm:ss", t), "14:05:09");
        assert_eq!(format("ww", t), "38");
    }

    #[test]
    fn separators_and_unknown_characters_pass_through() {
        assert_eq!(format("YYYY/MM/DD", sample()), "2026/09/17");
        assert_eq!(format("[YYYY]", sample()), "[2026]");
    }

    #[test]
    fn quoted_text_is_literal() {
        assert_eq!(format("'Week' ww', 'YYYY", sample()), "Week 38, 2026");
        assert_eq!(
            format("'Daily note for 'dddd", sample()),
            "Daily note for Thursday"
        );
    }

    #[test]
    fn a_local_offset_shifts_the_rendered_date() {
        // 14:05 UTC is the next day in UTC+11.
        let local = at(1_789_653_909_000, 11 * 3600);
        assert_eq!(format("YYYY-MM-DD HH:mm", local), "2026-09-18 01:05");
    }

    #[test]
    fn a_nonsensical_timestamp_does_not_panic() {
        let t = at(i64::MAX, 0);
        assert!(!format("YYYY-MM-DD", t).is_empty());
    }
}
