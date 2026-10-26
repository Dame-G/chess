//! Lichess's daily limit on games between bots: a BOT plays at most 100 games against other bots in any rolling day
//! (lila's `BotLimit`, rate-limit key `bot.vsBot.day`, counted when a game starts). Games against people are not
//! limited. Over the limit, Lichess refuses our challenges to bots and our acceptance of theirs; a bot that is over its
//! own limit cannot be challenged either.
//!
//! The bot counts the bot games it starts over the last 24 hours (kept by `play` across restarts) and stops looking
//! for bot games at 100 until the oldest ages out. Lichess is the judge: when it answers that we are limited, its
//! reset time wins, and the count is trimmed to agree with it. Times are milliseconds since the epoch, passed in.

use serde::{Deserialize, Serialize};

use crate::ids::parse_iso_ms;

/// Games against other bots Lichess allows in a rolling day.
pub const BOT_GAMES_PER_DAY: usize = 100;

pub const DAY_MS: i64 = 86_400_000;

/// When Lichess says we are limited without saying until when.
pub const UNKNOWN_RESET_MS: i64 = 3_600_000;

/// The bot games of the last day, and Lichess's word on when the limit lifts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    /// The bot games started in the last day, by Lichess game id, with when (ms since the epoch).
    #[serde(default)]
    pub games: Vec<(String, i64)>,
    /// When Lichess last said our limit lifts (ms since the epoch).
    #[serde(default)]
    pub lichess_until: Option<i64>,
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

/// How long until the limit a Lichess error describes lifts, in ms: its `ratelimit.seconds`, else a time in it (a
/// `ratelimit.until`, or the "please wait until <time>" of the message), else an hour.
pub fn retry_after_ms(body: &serde_json::Value, message: &str, now: i64) -> i64 {
    let limit = &body["ratelimit"];
    if let Some(s) = limit["seconds"].as_f64().filter(|s| *s >= 0.0) {
        return (s * 1000.0).round() as i64;
    }
    let until = match &limit["until"] {
        serde_json::Value::String(s) => parse_iso_ms(s),
        serde_json::Value::Number(n) => n.as_i64(),
        _ => None,
    }
    .or_else(|| message.split("until ").nth(1).and_then(|rest| parse_iso_ms(rest.split_whitespace().next()?)));
    match until {
        Some(t) => (t - now).max(0),
        None => UNKNOWN_RESET_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reset_is_read_from_seconds_or_a_time() {
        let now = parse_iso_ms("2026-10-25T12:00:00Z").unwrap();
        let body = serde_json::json!({ "ratelimit": { "key": "bot.vsBot.day", "seconds": 90 } });
        assert_eq!(retry_after_ms(&body, "", now), 90_000);
        let msg = "Gamma played 100 games against other bots today, please wait until 2026-10-25T13:00:00Z to challenge them.";
        assert_eq!(retry_after_ms(&serde_json::Value::Null, msg, now), 3_600_000);
        assert_eq!(retry_after_ms(&serde_json::Value::Null, "slow down", now), UNKNOWN_RESET_MS);
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
