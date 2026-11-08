//! Times as people read them: "2 h ago", "Yesterday 21:40", "Sep 3".

use chrono::{DateTime, Datelike, Local, TimeZone, Utc};

pub fn now() -> i64 {
    Utc::now().timestamp()
}

fn local(unix: i64) -> DateTime<Local> {
    Local
        .timestamp_opt(unix, 0)
        .single()
        .unwrap_or_else(|| Local.timestamp_opt(0, 0).unwrap())
}

/// The start of the local day that contains `unix`.
pub fn local_day_start(unix: i64) -> i64 {
    let date = local(unix).date_naive();
    date.and_hms_opt(0, 0, 0)
        .and_then(|t| Local.from_local_datetime(&t).earliest())
        .map_or(unix - unix.rem_euclid(86_400), |t| t.timestamp())
}

/// A short time for list rows: "21:40" today, "Yesterday", "Tue", "Sep 3",
/// "Sep 3 2024".
pub fn short(unix: i64, now: i64) -> String {
    let then = local(unix);
    let today = local_day_start(now);
    if unix >= today {
        then.format("%H:%M").to_string()
    } else if unix >= today - 86_400 {
        format!("Yesterday {}", then.format("%H:%M"))
    } else if unix >= today - 6 * 86_400 {
        then.format("%a %H:%M").to_string()
    } else if then.year() == local(now).year() {
        then.format("%b %-d").to_string()
    } else {
        then.format("%b %-d %Y").to_string()
    }
}

/// "September 3, 2026 at 16:12".
pub fn long(unix: i64) -> String {
    local(unix).format("%B %-d, %Y at %H:%M").to_string()
}

/// "Sep 3" for a week's label.
pub fn day_label(unix: i64) -> String {
    local(unix).format("%b %-d").to_string()
}

/// "2 min ago", "3 h ago", "5 days ago".
pub fn ago(unix: i64, now: i64) -> String {
    let seconds = (now - unix).max(0);
    match seconds {
        0..=59 => "just now".to_owned(),
        60..=3_599 => format!("{} min ago", seconds / 60),
        3_600..=86_399 => format!("{} h ago", seconds / 3_600),
        86_400..=172_799 => "yesterday".to_owned(),
        _ if seconds < 60 * 86_400 => format!("{} days ago", seconds / 86_400),
        _ if seconds < 730 * 86_400 => format!("{} months ago", seconds / (30 * 86_400)),
        _ => format!("{} years ago", seconds / (365 * 86_400)),
    }
}

/// "3:07" for seconds.
pub fn duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as i64;
    if total >= 3_600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3_600,
            (total / 60) % 60,
            total % 60
        )
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

/// The Monday 00:00 local of the week that contains `unix`.
pub fn week_start(unix: i64) -> i64 {
    let day = local_day_start(unix);
    let weekday = local(day).weekday().num_days_from_monday() as i64;
    local_day_start(day - weekday * 86_400 + 3_600)
}

/// Parses Slippi's `startAt` ("2026-11-08T01:59:32Z", sometimes without
/// the zone or with fractions).
pub fn parse_start_at(text: &str) -> Option<i64> {
    if let Ok(t) = DateTime::parse_from_rfc3339(text) {
        return Some(t.timestamp());
    }
    let trimmed = text.trim_end_matches('Z');
    chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|t| Utc.from_utc_datetime(&t).timestamp())
}

/// The time in a replay's file name (`Game_20261107T203043.slp`, local).
pub fn from_file_name(name: &str) -> Option<i64> {
    let stamp = name.strip_prefix("Game_")?.get(..15)?;
    let naive = chrono::NaiveDateTime::parse_from_str(stamp, "%Y%m%dT%H%M%S").ok()?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|t| t.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slippi_start_times_parse() {
        assert_eq!(parse_start_at("2026-11-08T01:59:32Z"), Some(1_794_103_172));
        assert_eq!(
            parse_start_at("2026-11-08T01:59:32.5Z"),
            Some(1_794_103_172)
        );
        assert_eq!(parse_start_at("2026-11-08T01:59:32"), Some(1_794_103_172));
        assert_eq!(parse_start_at("nonsense"), None);
    }

    #[test]
    fn durations_read_like_the_game_clock() {
        assert_eq!(duration(187.0), "3:07");
        assert_eq!(duration(5.4), "0:05");
        assert_eq!(duration(3_725.0), "1:02:05");
    }

    #[test]
    fn file_names_carry_a_local_time() {
        assert!(from_file_name("Game_20261107T203043.slp").is_some());
        assert!(from_file_name("replay.slp").is_none());
    }
}
