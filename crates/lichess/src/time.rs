//! Lichess's times: milliseconds since 1970, and the ISO 8601 strings some error messages carry.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the epoch, by the wall clock.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A UTC time as Lichess writes it (`2026-10-25T14:32:00Z`, `2026-10-25T14:32:00.123Z`), in ms since the epoch.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim().trim_end_matches(['.', ',']);
    let s = s.strip_suffix('Z').or_else(|| s.strip_suffix("+00:00"))?;
    let (date, time) = s.split_once('T')?;
    let mut date = date.split('-').map(str::parse::<i64>);
    let (y, m, d) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (time, frac) = time.split_once('.').unwrap_or((time, ""));
    let mut time = time.split(':').map(str::parse::<i64>);
    let (h, mi, sec) = (time.next()?.ok()?, time.next()?.ok()?, time.next().unwrap_or(Ok(0)).ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let ms: i64 = format!("{frac:0<3}").get(..3).and_then(|f| f.parse().ok()).unwrap_or(0);
    Some(((days_from_civil(y, m, d) * 86_400) + h * 3600 + mi * 60 + sec) * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_lichess_times() {
        assert_eq!(parse_iso("1970-01-01T01:00:00Z"), Some(3_600_000));
        assert_eq!(parse_iso("2026-10-25T14:32:00.123Z"), Some(1_792_938_720_123));
        assert_eq!(parse_iso("2026-10-25T14:32:00+00:00."), Some(1_792_938_720_000));
        assert_eq!(parse_iso("2026-13-25T14:32:00Z"), None);
        assert_eq!(parse_iso("yesterday"), None);
    }
}
