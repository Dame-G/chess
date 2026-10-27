//! Looking for games. A BOT account cannot use Lichess's lobby or pools: it plays only when challenged. So, to find a
//! game, the bot challenges another bot that is online, picked at random among those near its rating at a clock from
//! the settings, and tries another when one declines or does not answer. `once` stops at the first game that starts;
//! `always` goes on after every game, and comes back after a restart.
//!
//! [`Seeking`] is every decision of the search as a state machine; `play` runs the task around it, makes the HTTP
//! calls, and passes in the time (milliseconds since the epoch) and the randomness.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{Ratings, bot::SeekClock, quota};
use crate::time::Speed;

/// How long a bot that declined or ignored a challenge is left alone.
pub const SHUN_MS: i64 = 3_600_000;

/// Whether, and how, the bot is looking for games.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SeekMode {
    /// Not looking.
    #[default]
    Off,
    /// Looking until one game starts.
    Once,
    /// Looking again after every game, across restarts, until stopped.
    Always,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SeekOpponent {
    pub name: String,
    pub title: Option<String>,
    /// Their rating at the challenge's speed.
    pub rating: u32,
}

/// The challenge the bot has sent and is waiting on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SeekChallenge {
    /// The challenge id, which becomes the game id if it is accepted.
    pub id: String,
    pub url: String,
    pub opponent: SeekOpponent,
    pub clock: SeekClock,
    pub rated: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SeekStatus {
    pub mode: SeekMode,
    /// The challenge sent and not answered yet. There is never more than one.
    pub pending: Option<SeekChallenge>,
    /// A short sentence on how the search last went, for people to read.
    pub note: Option<String>,
}

/// The pauses the search takes, in ms. Tests shorten them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Between a game's end, a decline or a challenge left unanswered and the next challenge.
    pub polite: i64,
    /// How long a challenge may go unanswered before it is withdrawn.
    pub answer: i64,
    /// After Lichess answers 429.
    pub rate_limited: i64,
    /// When there is nobody to challenge, or Lichess cannot be reached.
    pub retry: i64,
    /// How long an accepted challenge whose game has not shown up still counts as a game.
    pub accepted_for: i64,
}

impl Default for Timing {
    fn default() -> Self {
        Self { polite: 7_000, answer: 25_000, rate_limited: 61_000, retry: 60_000, accepted_for: 60_000 }
    }
}

/// The challenge waiting for an answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub challenge: SeekChallenge,
    /// The opponent's lower-case id.
    pub opponent: String,
    /// When it is withdrawn unanswered.
    pub expires: i64,
}

/// What became of a challenge of ours, as the event stream tells it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// With Lichess's decline key.
    Declined(Option<String>),
    /// Withdrawn by Lichess.
    Canceled,
    Started,
}

/// Why no challenge went out, and when to try again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Retry {
    pub note: String,
    /// Ms until the next try.
    pub after: i64,
    /// A bot (lower-case id) not to try again for a while.
    pub shun: Option<String>,
    /// A bot that has played its bot games for the day, and ms until it may play again.
    pub rest: Option<(String, i64)>,
}

/// What the search should do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// The search is off: the task ends.
    Stop,
    /// Until woken, or until this time.
    Wait(Option<i64>),
    /// The pending challenge went unanswered: withdraw it with [`Seeking::expire`].
    Expired(Pending),
    /// The daily limit on bot games is reached until this time, and the note does not say so yet: tell of it with
    /// [`Seeking::note_limit`].
    Limited(i64),
    /// The daily limit has lifted: the status changed.
    Lifted,
    /// Challenge someone now.
    Go,
}

/// What the step needs to know about the bot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Situation {
    /// Online by the user's switch.
    pub online: bool,
    /// The event stream is connected.
    pub connected: bool,
    /// Games running or accepted and about to start.
    pub busy: usize,
    pub max_games: usize,
    /// While at the daily bot-game limit: until when.
    pub limit_until: Option<i64>,
}

/// A bot online now, as `GET /api/bot/online` lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OnlineBot {
    pub name: String,
    pub title: Option<String>,
    pub ratings: Ratings,
}

/// Everything the search decides on.
#[derive(Clone, Debug, Default)]
pub struct Seeking {
    pub mode: SeekMode,
    pub pending: Option<Pending>,
    pub note: Option<String>,
    /// Bots that declined or ignored us, by lower-case id, and when.
    shunned: HashMap<String, i64>,
    /// Bots that have played their bot games for the day, by lower-case id, and until when.
    resting: HashMap<String, i64>,
    /// The end of the daily limit the note tells of, once it does.
    limit_noted: Option<i64>,
    /// Our account's ratings, until a game ends and changes them.
    pub ratings: Option<Ratings>,
    /// No challenge before this.
    next_at: Option<i64>,
    /// Outcomes that arrived before the answer to the challenge they belong to.
    early: VecDeque<(String, Outcome)>,
    pub timing: Timing,
}

fn rated_word(rated: bool) -> &'static str {
    if rated { "rated" } else { "casual" }
}

/// Lichess's decline keys, as a few words.
pub fn decline_phrase(key: &str) -> Option<&'static str> {
    Some(match key {
        "later" => "busy",
        "tooFast" => "too fast",
        "tooSlow" => "too slow",
        "timeControl" => "time control",
        "rated" => "wants rated",
        "casual" => "wants casual",
        "standard" | "variant" => "variant",
        "noBot" => "no bots",
        "onlyBot" => "bots only",
        _ => return None,
    })
}

impl Seeking {
    pub fn status(&self) -> SeekStatus {
        SeekStatus {
            mode: self.mode,
            pending: self.pending.as_ref().map(|p| p.challenge.clone()),
            note: self.note.clone(),
        }
    }

    /// The id of the challenge waiting for an answer.
    pub fn pending_id(&self) -> Option<&str> {
        self.pending.as_ref().map(|p| p.challenge.id.as_str())
    }

    /// Turns the search to `mode`. Turning it off answers the pending challenge, to withdraw.
    pub fn set_mode(&mut self, mode: SeekMode) -> Option<SeekChallenge> {
        self.mode = mode;
        if mode == SeekMode::Off {
            self.early.clear();
            return self.pending.take().map(|p| p.challenge);
        }
        None
    }

    /// Applies what became of challenge `id`, if it is the pending one: `true` then. An outcome for a challenge not
    /// pending yet is kept for when it is.
    pub fn settle(&mut self, id: &str, outcome: Outcome, now: i64) -> bool {
        if self.pending.as_ref().is_none_or(|p| p.challenge.id != id) {
            if self.mode != SeekMode::Off {
                self.early.push_back((id.to_owned(), outcome));
                while self.early.len() > 16 {
                    self.early.pop_front();
                }
            }
            return false;
        }
        let Some(p) = self.pending.take() else { return false };
        let name = &p.challenge.opponent.name;
        match outcome {
            Outcome::Declined(reason) => {
                self.shunned.insert(p.opponent.clone(), now);
                let why = reason.as_deref().and_then(decline_phrase).map(|r| format!(" ({r})")).unwrap_or_default();
                self.note = Some(format!("{name} declined{why}. Trying another bot."));
                self.next_at = Some(now + self.timing.polite);
            }
            Outcome::Canceled => {
                self.note = Some(format!("The challenge to {name} was withdrawn. Trying another bot."));
                self.next_at = Some(now + self.timing.polite);
            }
            Outcome::Started => {
                let c = &p.challenge;
                self.note = Some(format!("{name} accepted: playing {} {}.", c.clock, rated_word(c.rated)));
                if self.mode == SeekMode::Once {
                    self.mode = SeekMode::Off;
                }
            }
        }
        true
    }

    /// What to do now.
    pub fn step(&mut self, now: i64, s: Situation) -> Step {
        if self.mode == SeekMode::Off || !s.online {
            self.mode = SeekMode::Off;
            return Step::Stop;
        }
        if let Some(p) = &self.pending {
            return if now >= p.expires { Step::Expired(p.clone()) } else { Step::Wait(Some(p.expires)) };
        }
        match s.limit_until {
            Some(until) if self.limit_noted != Some(until) => return Step::Limited(until),
            // The search stays on, and goes on when the limit lifts.
            Some(until) => return Step::Wait(Some(until.max(now + 1))),
            None if self.limit_noted.take().is_some() => {
                self.note = Some("Looking for a bot to play again.".into());
                return Step::Lifted;
            }
            None => {}
        }
        if !s.connected {
            return Step::Wait(None);
        }
        if s.busy >= s.max_games {
            // A game's end wakes the search; an accepted challenge whose game never showed stops counting by itself.
            return Step::Wait(Some(now + self.timing.accepted_for));
        }
        match self.next_at {
            Some(at) if now < at => Step::Wait(Some(at)),
            _ => Step::Go,
        }
    }

    /// Tells of the daily limit, until `until`; `at` is that time as people read it ("14:32").
    pub fn note_limit(&mut self, until: i64, at: &str) {
        self.limit_noted = Some(until);
        self.note =
            Some(format!("Played {} games against bots today; looking again at {at}.", quota::BOT_GAMES_PER_DAY));
    }

    /// The pending challenge `id` went unanswered: `true` when it was still pending (then withdraw it at Lichess).
    pub fn expire(&mut self, id: &str, now: i64) -> bool {
        if self.pending.as_ref().is_none_or(|p| p.challenge.id != id) {
            return false;
        }
        let Some(p) = self.pending.take() else { return false };
        self.shunned.insert(p.opponent, now);
        self.note = Some(format!("{} did not answer. Trying another bot.", p.challenge.opponent.name));
        self.next_at = Some(now + self.timing.polite);
        true
    }

    /// A game ended: the next challenge waits a moment, and our ratings have changed.
    pub fn game_ended(&mut self, now: i64) {
        self.ratings = None;
        self.next_at = Some(now + self.timing.polite);
    }

    /// An incoming challenge from `challenger` was accepted: if that fills every slot, the challenge we are waiting
    /// on is answered, to withdraw.
    pub fn make_room(&mut self, challenger: &str, busy: usize, max_games: usize) -> Option<SeekChallenge> {
        if busy < max_games {
            return None;
        }
        let p = self.pending.take()?;
        self.note =
            Some(format!("Took a challenge from {challenger}; withdrew ours to {}.", p.challenge.opponent.name));
        Some(p.challenge)
    }

    /// The bots not to challenge now (lower-case ids): those that declined or ignored us in the last hour, and those
    /// resting from their own daily limit.
    pub fn avoided(&mut self, now: i64) -> HashSet<String> {
        self.shunned.retain(|_, at| now - *at < SHUN_MS);
        self.resting.retain(|_, until| now < *until);
        self.shunned.keys().chain(self.resting.keys()).cloned().collect()
    }

    /// A challenge went out. Answers its id when the search was stopped while it was on its way: withdraw it.
    pub fn sent(&mut self, challenge: SeekChallenge, opponent: &str, now: i64) -> Option<String> {
        if self.mode == SeekMode::Off {
            return Some(challenge.id);
        }
        let c = &challenge;
        self.note = Some(format!(
            "Challenged {} ({}) to {} {}.",
            c.opponent.name,
            c.opponent.rating,
            c.clock,
            rated_word(c.rated)
        ));
        let id = c.id.clone();
        self.pending =
            Some(Pending { challenge, opponent: opponent.to_lowercase(), expires: now + self.timing.answer });
        if let Some(i) = self.early.iter().position(|(e, _)| *e == id)
            && let Some((_, outcome)) = self.early.remove(i)
        {
            self.settle(&id, outcome, now);
        }
        None
    }

    /// No challenge went out: remember why, and when to try again.
    pub fn failed(&mut self, retry: Retry, now: i64) {
        if let Some(id) = retry.shun {
            self.shunned.insert(id, now);
        }
        if let Some((id, after)) = retry.rest {
            self.resting.insert(id, now + after);
        }
        if self.mode != SeekMode::Off {
            self.note = Some(retry.note);
        }
        self.next_at = Some(now + retry.after);
    }
}

/// Picks a clock among the settings' with `roll(n)` choosing among `n`.
pub fn pick_clock(clocks: &[SeekClock], roll: &mut dyn FnMut(usize) -> usize) -> Option<SeekClock> {
    (!clocks.is_empty()).then(|| clocks[roll(clocks.len()).min(clocks.len() - 1)])
}

/// Picks a bot to challenge at `speed`: never `me` (our lower-case id), one in `avoided` or one without a rating at
/// that speed. One within `range` of `ours` if there is any, else within twice that, else anyone; `roll(n)` picks
/// among `n`. Answers the bot and its rating.
pub fn pick<'a>(
    bots: &'a [OnlineBot],
    me: &str,
    speed: Speed,
    ours: u32,
    range: u32,
    avoided: &HashSet<String>,
    roll: &mut dyn FnMut(usize) -> usize,
) -> Option<(&'a OnlineBot, u32)> {
    let candidates: Vec<(&OnlineBot, u32)> = bots
        .iter()
        .filter(|b| {
            let id = b.name.to_lowercase();
            id != me && !avoided.contains(&id)
        })
        .filter_map(|b| Some((b, b.ratings.get(speed)?.rating)))
        .collect();
    for window in [Some(range), Some(range.saturating_mul(2)), None] {
        let tier: Vec<_> = candidates.iter().filter(|(_, r)| window.is_none_or(|w| r.abs_diff(ours) <= w)).collect();
        if !tier.is_empty() {
            return Some(*tier[roll(tier.len()).min(tier.len() - 1)]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lichess::Rating;

    fn bot(name: &str, blitz: Option<u32>) -> OnlineBot {
        let blitz = blitz.map(|rating| Rating { rating, games: None, provisional: false });
        OnlineBot { name: name.into(), title: Some("BOT".into()), ratings: Ratings { blitz, ..Ratings::default() } }
    }

    fn names(bots: &[OnlineBot], avoided: &[&str], ours: u32) -> Vec<String> {
        let avoided: HashSet<String> = avoided.iter().map(|s| s.to_string()).collect();
        // Every index the roll can give, to see the whole tier.
        let mut seen = Vec::new();
        for i in 0..bots.len() {
            if let Some((b, _)) = pick(bots, "recklessbot", Speed::Blitz, ours, 300, &avoided, &mut |_| i)
                && !seen.contains(&b.name)
            {
                seen.push(b.name.clone());
            }
        }
        seen.sort();
        seen
    }

    #[test]
    fn picks_within_the_window_then_twice_it_then_anyone() {
        let bots = [
            bot("RecklessBot", Some(1500)),
            bot("Near", Some(1650)),
            bot("Nearer", Some(1400)),
            bot("Farther", Some(1950)),
            bot("Far", Some(2600)),
            bot("Unrated", None),
        ];
        assert_eq!(names(&bots, &[], 1500), ["Near", "Nearer"]);
        assert_eq!(names(&bots, &["near", "nearer"], 1500), ["Farther"]);
        assert_eq!(names(&bots, &["near", "nearer", "farther"], 1500), ["Far"]);
        // Never ourselves, nor a bot without a rating at the speed.
        assert!(names(&bots, &["near", "nearer", "farther", "far"], 1500).is_empty());
        let clocks = [SeekClock { initial: 180, increment: 0 }, SeekClock { initial: 180, increment: 2 }];
        assert_eq!(pick_clock(&clocks, &mut |_| 1), Some(clocks[1]));
        assert_eq!(pick_clock(&[], &mut |_| 0), None);
    }

    fn challenge(id: &str) -> SeekChallenge {
        SeekChallenge {
            id: id.into(),
            url: String::new(),
            opponent: SeekOpponent { name: "Other".into(), title: None, rating: 1500 },
            clock: SeekClock { initial: 180, increment: 2 },
            rated: true,
        }
    }

    #[test]
    fn an_early_outcome_waits_for_its_challenge() {
        let mut s = Seeking { mode: SeekMode::Once, ..Seeking::default() };
        assert!(!s.settle("x", Outcome::Declined(Some("tooFast".into())), 0));
        assert_eq!(s.early.len(), 1);
        assert_eq!(s.sent(challenge("x"), "Other", 10), None);
        assert!(s.pending.is_none());
        assert_eq!(s.note.as_deref(), Some("Other declined (too fast). Trying another bot."));
        assert!(s.avoided(10).contains("other"));
        // An hour on, it may be asked again.
        assert!(!s.avoided(10 + SHUN_MS).contains("other"));
    }

    fn situation() -> Situation {
        Situation { online: true, connected: true, busy: 0, max_games: 1, limit_until: None }
    }

    #[test]
    fn steps_through_a_search() {
        let mut s = Seeking { mode: SeekMode::Once, ..Seeking::default() };
        assert_eq!(s.step(0, situation()), Step::Go);
        assert_eq!(s.step(0, Situation { connected: false, ..situation() }), Step::Wait(None));
        assert_eq!(s.step(0, Situation { busy: 1, ..situation() }), Step::Wait(Some(60_000)));
        // A challenge out waits for its answer, then expires.
        s.sent(challenge("c1"), "Other", 0);
        assert_eq!(s.step(1_000, situation()), Step::Wait(Some(25_000)));
        let Step::Expired(p) = s.step(25_000, situation()) else { panic!("not expired") };
        assert!(s.expire(&p.challenge.id, 25_000));
        assert_eq!(s.note.as_deref(), Some("Other did not answer. Trying another bot."));
        assert_eq!(s.step(26_000, situation()), Step::Wait(Some(32_000)));
        // The daily limit is told of once, waited out, and its end announced.
        let limited = Situation { limit_until: Some(100_000), ..situation() };
        assert_eq!(s.step(40_000, limited), Step::Limited(100_000));
        s.note_limit(100_000, "14:32");
        assert_eq!(s.note.as_deref(), Some("Played 100 games against bots today; looking again at 14:32."));
        assert_eq!(s.step(40_000, limited), Step::Wait(Some(100_000)));
        assert_eq!(s.step(100_000, situation()), Step::Lifted);
        assert_eq!(s.step(100_000, situation()), Step::Go);
        // A start ends a `once` search.
        s.sent(challenge("c2"), "Other", 100_000);
        assert!(s.settle("c2", Outcome::Started, 101_000));
        assert_eq!(s.mode, SeekMode::Off);
        assert_eq!(s.step(101_000, situation()), Step::Stop);
    }

    #[test]
    fn stopping_or_filling_up_withdraws_the_pending_challenge() {
        let mut s = Seeking { mode: SeekMode::Always, ..Seeking::default() };
        s.sent(challenge("c1"), "Other", 0);
        assert_eq!(s.make_room("Someone", 0, 1), None);
        assert_eq!(s.make_room("Someone", 1, 1).map(|c| c.id), Some("c1".into()));
        assert_eq!(s.note.as_deref(), Some("Took a challenge from Someone; withdrew ours to Other."));
        s.sent(challenge("c2"), "Other", 0);
        assert_eq!(s.set_mode(SeekMode::Off).map(|c| c.id), Some("c2".into()));
        // A challenge that arrives after the stop is withdrawn at once.
        assert_eq!(s.sent(challenge("c3"), "Other", 0), Some("c3".into()));
        s.mode = SeekMode::Always;
        s.failed(Retry { note: "busy".into(), after: 5, shun: Some("x".into()), rest: Some(("y".into(), 10)) }, 0);
        assert_eq!(s.avoided(1), HashSet::from(["x".to_owned(), "y".to_owned()]));
        assert!(!s.avoided(11).contains("y"));
    }
}
