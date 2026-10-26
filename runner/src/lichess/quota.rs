//! Lichess's daily limit on games between bots: a BOT plays at most 100 games against other bots in any rolling day
//! (lila's `BotLimit`, rate-limit key `bot.vsBot.day`, counted when a game starts). Games against people are not
//! limited. Over the limit, Lichess refuses our challenges to bots and our acceptance of theirs; a bot that is over its
//! own limit cannot be challenged either.
//!
//! The bot counts the bot games it starts over the last 24 hours, kept in the `meta` table so a restart remembers them,
//! and stops looking for bot games at 100 until the oldest ages out. Lichess is the judge: when it answers that we are
//! limited, its reset time wins, and the count is trimmed to agree with it.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{Bot, meta_get, meta_set};
use crate::api::AppState;

/// Games against other bots Lichess allows in a rolling day.
pub const BOT_GAMES_PER_DAY: usize = 100;

pub(super) const DAY_MS: i64 = 86_400_000;

/// When Lichess says we are limited without saying until when.
const UNKNOWN_RESET: Duration = Duration::from_secs(3600);

pub(super) const QUOTA_KEY: &str = "lichess.bot_games";

/// Milliseconds since the epoch, by the wall clock.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct Quota {
    /// The bot games started in the last day, by Lichess game id, with when (ms since the epoch).
    #[serde(default)]
    pub(super) games: Vec<(String, i64)>,
    /// When Lichess last said our limit lifts (ms since the epoch).
    #[serde(default)]
    pub(super) lichess_until: Option<i64>,
}

impl Quota {
    /// When each game of the last day started, oldest first.
    fn recent(&self, now: i64) -> Vec<i64> {
        let mut at: Vec<i64> = self.games.iter().map(|(_, t)| *t).filter(|t| *t > now - DAY_MS).collect();
        at.sort_unstable();
        at
    }

    /// Bot games started in the last 24 hours.
    pub fn today(&self, now: i64) -> u32 {
        self.recent(now).len() as u32
    }

    /// When our count drops below the limit, while it is at it.
    fn count_until(&self, now: i64) -> Option<i64> {
        let at = self.recent(now);
        (at.len() >= BOT_GAMES_PER_DAY).then(|| at[at.len() - BOT_GAMES_PER_DAY] + DAY_MS)
    }

    /// Until when no game against a bot can start, if that is now.
    pub fn until(&self, now: i64) -> Option<i64> {
        let lichess = self.lichess_until.filter(|t| *t > now);
        lichess.max(self.count_until(now))
    }

    /// Counts game `id` started at `now`; `false` when it was counted already.
    pub fn add(&mut self, id: &str, now: i64) -> bool {
        self.games.retain(|(_, t)| *t > now - DAY_MS);
        if self.games.iter().any(|(g, _)| g == id) {
            return false;
        }
        self.games.push((id.to_owned(), now));
        true
    }

    /// Lichess says we are limited until `until`: that is the reset, and the count forgets the games that would keep
    /// it limited for longer.
    pub fn limited(&mut self, until: i64, now: i64) {
        self.lichess_until = Some(until);
        self.games.retain(|(_, t)| *t > now - DAY_MS);
        while self.count_until(now).is_some_and(|t| t > until) {
            let Some(oldest) = self.games.iter().enumerate().min_by_key(|(_, (_, t))| *t).map(|(i, _)| i) else {
                break;
            };
            self.games.remove(oldest);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Times as text
// ---------------------------------------------------------------------------------------------------------------------

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

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { yoe + era * 400 + 1 } else { yoe + era * 400 }, m, d)
}

/// `2026-10-25T14:32:00Z`.
pub fn iso(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s % 3600 / 60, s % 60)
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

/// How long until the limit a Lichess error describes lifts: its `ratelimit.seconds`, else a time in it (a
/// `ratelimit.until`, or the "please wait until <time>" of the message), else an hour.
pub fn retry_after(body: &serde_json::Value, message: &str, now: i64) -> Duration {
    let limit = &body["ratelimit"];
    if let Some(s) = limit["seconds"].as_f64().filter(|s| *s >= 0.0) {
        return Duration::from_secs_f64(s);
    }
    let until = match &limit["until"] {
        serde_json::Value::String(s) => parse_iso(s),
        serde_json::Value::Number(n) => n.as_i64(),
        _ => None,
    }
    .or_else(|| message.split("until ").nth(1).and_then(|rest| parse_iso(rest.split_whitespace().next()?)));
    match until {
        Some(t) => Duration::from_millis((t - now).max(0) as u64),
        None => UNKNOWN_RESET,
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The bot's count
// ---------------------------------------------------------------------------------------------------------------------

impl Bot {
    /// Whether a game against a bot cannot start now.
    pub(super) fn bots_limited(&self) -> bool {
        self.inner.lock().unwrap().quota.until(now_ms()).is_some()
    }
}

/// Reads the count kept by the last run.
pub(super) async fn load(state: &AppState) -> anyhow::Result<()> {
    if let Some(json) = meta_get(state, QUOTA_KEY).await? {
        state.lichess.inner.lock().unwrap().quota = serde_json::from_str(&json).unwrap_or_default();
    }
    Ok(())
}

pub(super) async fn save(state: &AppState) {
    let json = serde_json::to_string(&state.lichess.inner.lock().unwrap().quota).unwrap_or_default();
    if let Err(e) = meta_set(state, QUOTA_KEY, json).await {
        tracing::warn!("lichess: could not store the bot-game count: {e:#}");
    }
}

/// Game `id` against a bot has started: it counts, once.
pub(super) async fn bot_game_started(state: &AppState, id: &str) {
    let added = state.lichess.inner.lock().unwrap().quota.add(id, now_ms());
    if added {
        save(state).await;
        state.lichess.announce(state);
    }
}

/// Lichess says we have played our bot games for the day: none until `after` has passed.
pub(super) async fn limited(state: &AppState, after: Duration) {
    {
        let now = now_ms();
        state.lichess.inner.lock().unwrap().quota.limited(now + after.as_millis() as i64, now);
    }
    save(state).await;
    state.lichess.announce(state);
}

/// `14:32`: the local time of day at `ms`, as the machine's time zone has it.
pub(super) async fn local_clock(state: &AppState, ms: i64) -> String {
    let fallback = iso(ms);
    state
        .store
        .call(move |c| {
            Ok(c.query_row("SELECT strftime('%H:%M', ?1 / 1000, 'unixepoch', 'localtime')", [ms], |r| r.get(0))?)
        })
        .await
        .unwrap_or_else(|_: anyhow::Error| fallback[11..16].to_owned() + " UTC")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_times_go_both_ways() {
        let t = parse_iso("2026-10-25T14:32:07Z").unwrap();
        assert_eq!(iso(t), "2026-10-25T14:32:07Z");
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("2000-03-01T00:00:00.250Z"), Some(951_868_800_250));
        assert_eq!(parse_iso("2026-10-25T14:32:07.5Z."), Some(t + 500));
        assert_eq!(parse_iso("tomorrow"), None);
    }

    #[test]
    fn the_reset_is_read_from_seconds_or_a_time() {
        let now = parse_iso("2026-10-25T12:00:00Z").unwrap();
        let body = serde_json::json!({ "ratelimit": { "key": "bot.vsBot.day", "seconds": 90 } });
        assert_eq!(retry_after(&body, "", now), Duration::from_secs(90));
        let msg = "Gamma played 100 games against other bots today, please wait until 2026-10-25T13:00:00Z to challenge them.";
        assert_eq!(retry_after(&serde_json::Value::Null, msg, now), Duration::from_secs(3600));
        assert_eq!(retry_after(&serde_json::Value::Null, "slow down", now), UNKNOWN_RESET);
    }

    #[test]
    fn counts_a_rolling_day_and_follows_lichess() {
        let now = 10 * DAY_MS;
        let mut q = Quota::default();
        // 100 games, one a minute, the first 23 hours ago.
        let first = now - 23 * 3_600_000;
        for i in 0..100 {
            assert!(q.add(&format!("g{i}"), first + i * 60_000));
        }
        assert!(!q.add("g5", now), "a game counts once");
        assert_eq!(q.today(now), 100);
        assert_eq!(q.until(now), Some(first + DAY_MS));
        // An hour on, the first has aged out.
        assert_eq!(q.today(first + DAY_MS + 1), 99);
        assert_eq!(q.until(first + DAY_MS + 1), None);
        // Lichess says sooner: it wins, and the count is trimmed to agree.
        q.limited(now + 60_000, now);
        assert_eq!(q.until(now), Some(now + 60_000));
        assert_eq!(q.until(now + 60_001), None);
        // Lichess says limited with fewer games counted: limited all the same.
        let mut q = Quota::default();
        q.add("a", now);
        q.limited(now + 5_000, now);
        assert_eq!((q.today(now), q.until(now)), (1, Some(now + 5_000)));
    }
}
