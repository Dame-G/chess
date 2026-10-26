//! Looking for games. A BOT account cannot use Lichess's lobby or pools: it plays only when challenged. So, to find a
//! game, the bot challenges another bot that is online, picked at random among those near its rating at a clock from
//! the settings, and tries another when one declines or does not answer. `once` stops at the first game that starts;
//! `always` goes on after every game, and comes back after a restart.
//!
//! One task runs the search while it is on. Everything it decides on is in [`Seeking`], under the bot's lock; the event
//! stream and the game tasks tell it what happened (a decline, a game that started or ended) and wake it.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::Instant;
use utoipa::ToSchema;

use super::{
    Bot, BotState, LichessStatus, OnlineBot, Ratings,
    client::{CallError, Lichess},
    meta_set, need_client, read_bot,
    rules::{SeekClock, Speed},
};
use crate::api::{ApiError, AppState};

pub(super) const SEEK_KEY: &str = "lichess.seek";

/// A rating Lichess calls provisional, or no rating at all, counts as this.
const UNRATED: u32 = 1500;

/// How long a bot that declined or ignored a challenge is left alone.
const SHUN: Duration = Duration::from_secs(3600);

/// Whether, and how, the bot is looking for games.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SeekMode {
    /// Not looking.
    #[default]
    Off,
    /// Looking until one game starts.
    Once,
    /// Looking again after every game, across restarts, until stopped.
    Always,
}

#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct SeekOpponent {
    pub name: String,
    pub title: Option<String>,
    /// Their rating at the challenge's speed.
    pub rating: u32,
}

/// The challenge the bot has sent and is waiting on.
#[derive(Clone, Debug, PartialEq, Serialize, ToSchema)]
pub struct SeekChallenge {
    /// The challenge id, which becomes the game id if it is accepted.
    pub id: String,
    pub url: String,
    pub opponent: SeekOpponent,
    pub clock: SeekClock,
    pub rated: bool,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SeekStatus {
    pub mode: SeekMode,
    /// The challenge sent and not answered yet. There is never more than one.
    pub pending: Option<SeekChallenge>,
    /// A short sentence on how the search last went, for people to read.
    pub note: Option<String>,
}

/// The pauses the search takes. Tests shorten them.
#[derive(Clone, Copy, Debug)]
pub(super) struct Timing {
    /// Between a game's end, a decline or a challenge left unanswered and the next challenge.
    pub polite: Duration,
    /// How long a challenge may go unanswered before it is withdrawn.
    pub answer: Duration,
    /// After Lichess answers 429.
    pub rate_limited: Duration,
    /// When there is nobody to challenge, or Lichess cannot be reached.
    pub retry: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            polite: Duration::from_secs(7),
            answer: Duration::from_secs(25),
            rate_limited: Duration::from_secs(61),
            retry: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Pending {
    challenge: SeekChallenge,
    /// The opponent's lower-case id.
    opponent: String,
    expires: Instant,
}

/// What became of a challenge of ours, as the event stream tells it.
#[derive(Clone, Debug)]
pub(super) enum Outcome {
    /// With Lichess's decline key.
    Declined(Option<String>),
    /// Withdrawn by Lichess.
    Canceled,
    Started,
}

#[derive(Default)]
pub(super) struct Seeking {
    mode: SeekMode,
    pending: Option<Pending>,
    note: Option<String>,
    /// Bots that declined or ignored us, by lower-case id, and when.
    shunned: HashMap<String, Instant>,
    /// Our account's `perfs`, until a game ends and changes them.
    perfs: Option<Value>,
    /// No challenge before this.
    next_at: Option<Instant>,
    /// Outcomes that arrived before the answer to the challenge they belong to.
    early: VecDeque<(String, Outcome)>,
    /// Whether the search task runs.
    task: bool,
    pub timing: Timing,
}

impl Seeking {
    pub fn status(&self) -> SeekStatus {
        SeekStatus {
            mode: self.mode,
            pending: self.pending.as_ref().map(|p| p.challenge.clone()),
            note: self.note.clone(),
        }
    }

    /// Applies what became of challenge `id`, if it is the pending one: `true` then. An outcome for a challenge not
    /// pending yet is kept for when it is.
    fn settle(&mut self, id: &str, outcome: Outcome) -> bool {
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
        let now = Instant::now();
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
}

fn rated_word(rated: bool) -> &'static str {
    if rated { "rated" } else { "casual" }
}

/// Lichess's decline keys, as a few words.
fn decline_phrase(key: &str) -> Option<&'static str> {
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

// ---------------------------------------------------------------------------------------------------------------------
// Picking an opponent
// ---------------------------------------------------------------------------------------------------------------------

fn speed_key(speed: Speed) -> &'static str {
    match speed {
        Speed::Bullet => "bullet",
        Speed::Blitz => "blitz",
        Speed::Rapid => "rapid",
        Speed::Classical => "classical",
    }
}

fn rating_at(r: &Ratings, speed: Speed) -> Option<u32> {
    match speed {
        Speed::Bullet => r.bullet,
        Speed::Blitz => r.blitz,
        Speed::Rapid => r.rapid,
        Speed::Classical => r.classical,
    }
}

/// Our rating at `speed` from the account's `perfs`: a provisional or missing one counts as 1500.
pub fn our_rating(perfs: &Value, speed: Speed) -> u32 {
    let perf = &perfs[speed_key(speed)];
    match perf["rating"].as_u64() {
        Some(r) if perf["prov"].as_bool() != Some(true) => r as u32,
        _ => UNRATED,
    }
}

/// Picks a bot to challenge at `speed`: never `me` (our lower-case id), one in `shunned` or one without a rating at
/// that speed. One within `range` of `ours` if there is any, else within twice that, else anyone; `roll(n)` picks
/// among `n`. Answers the bot and its rating.
pub fn pick<'a>(
    bots: &'a [OnlineBot],
    me: &str,
    speed: Speed,
    ours: u32,
    range: u32,
    shunned: &HashSet<String>,
    roll: &mut dyn FnMut(usize) -> usize,
) -> Option<(&'a OnlineBot, u32)> {
    let candidates: Vec<(&OnlineBot, u32)> = bots
        .iter()
        .filter(|b| {
            let id = b.name.to_lowercase();
            id != me && !shunned.contains(&id)
        })
        .filter_map(|b| Some((b, rating_at(&b.ratings, speed)?)))
        .collect();
    for window in [Some(range), Some(range.saturating_mul(2)), None] {
        let tier: Vec<_> = candidates.iter().filter(|(_, r)| window.is_none_or(|w| r.abs_diff(ours) <= w)).collect();
        if !tier.is_empty() {
            return Some(*tier[roll(tier.len()).min(tier.len() - 1)]);
        }
    }
    None
}

/// A random index below `n`.
fn roll(n: usize) -> usize {
    let mut bytes = [0u8; 8];
    let _ = getrandom::fill(&mut bytes);
    (u64::from_le_bytes(bytes) % n.max(1) as u64) as usize
}

// ---------------------------------------------------------------------------------------------------------------------
// Turning it on and off
// ---------------------------------------------------------------------------------------------------------------------

/// Starts looking for games (`once` or `always`; `off` stops). 409 unless the bot is online.
pub async fn start(state: &Arc<AppState>, mode: SeekMode) -> Result<LichessStatus, ApiError> {
    if mode == SeekMode::Off {
        return stop(state).await;
    }
    need_client(state)?;
    if !state.lichess.inner.lock().unwrap().online {
        return Err(ApiError::conflict("the bot is offline: start it first (`chess lichess start`)"));
    }
    let persisted = if mode == SeekMode::Always { "always" } else { "off" };
    meta_set(state, SEEK_KEY, persisted.into()).await?;
    {
        let mut inner = state.lichess.inner.lock().unwrap();
        let seek = &mut inner.seek;
        if seek.mode == SeekMode::Off {
            seek.note = Some("Looking for a bot to play.".into());
            seek.next_at = None;
            seek.early.clear();
        }
        seek.mode = mode;
    }
    ensure_task(state);
    state.lichess.announce(state);
    Ok(state.lichess.status())
}

/// Stops looking for games, and withdraws the challenge waiting for an answer.
pub async fn stop(state: &Arc<AppState>) -> Result<LichessStatus, ApiError> {
    meta_set(state, SEEK_KEY, "off".into()).await?;
    halt(state).await;
    state.lichess.announce(state);
    Ok(state.lichess.status())
}

/// Turns the search off and withdraws the pending challenge, without storing anything.
pub(super) async fn halt(state: &AppState) {
    let pending = {
        let mut inner = state.lichess.inner.lock().unwrap();
        let seek = &mut inner.seek;
        seek.mode = SeekMode::Off;
        seek.note = None;
        seek.early.clear();
        seek.pending.take()
    };
    if let Some(p) = pending
        && let Some(client) = state.lichess.client()
    {
        withdraw(&client, &p.challenge.id).await;
    }
    state.lichess.seek_wake.notify_one();
}

/// Starts the search task unless it runs, and wakes it.
pub(super) fn ensure_task(state: &Arc<AppState>) {
    let spawn = {
        let mut inner = state.lichess.inner.lock().unwrap();
        !std::mem::replace(&mut inner.seek.task, true)
    };
    if spawn {
        tokio::spawn(run(state.clone()));
    }
    state.lichess.seek_wake.notify_one();
}

async fn withdraw(client: &Lichess, id: &str) {
    if let Err(e) = client.post(&format!("/api/challenge/{id}/cancel"), &[]).await {
        tracing::info!("lichess: could not withdraw challenge {id}: {e}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// What the event stream and the games tell the search
// ---------------------------------------------------------------------------------------------------------------------

impl Bot {
    /// Something became of one of our challenges.
    pub(super) fn seek_outcome(&self, state: &AppState, id: &str, outcome: Outcome) {
        let settled = self.inner.lock().unwrap().seek.settle(id, outcome);
        if settled {
            self.announce(state);
        }
    }

    /// A game ended: the next challenge waits a moment, and our ratings have changed.
    pub(super) fn seek_game_ended(&self) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.seek.perfs = None;
            inner.seek.next_at = Some(Instant::now() + inner.seek.timing.polite);
        }
        self.seek_wake.notify_one();
    }
}

/// An incoming challenge was accepted: if that fills every slot, the challenge we are waiting on is withdrawn.
pub(super) async fn make_room(state: &AppState, challenger: &str) {
    let pending = {
        let mut inner = state.lichess.inner.lock().unwrap();
        let busy = inner.games.len() + inner.accepted.len();
        if busy < inner.settings.max_games as usize {
            return;
        }
        let pending = inner.seek.pending.take();
        if let Some(p) = &pending {
            inner.seek.note =
                Some(format!("Took a challenge from {challenger}; withdrew ours to {}.", p.challenge.opponent.name));
        }
        pending
    };
    let Some(p) = pending else { return };
    if let Some(client) = state.lichess.client() {
        withdraw(&client, &p.challenge.id).await;
    }
    state.lichess.announce(state);
}

// ---------------------------------------------------------------------------------------------------------------------
// The search
// ---------------------------------------------------------------------------------------------------------------------

enum Step {
    Stop,
    /// Until woken, or until the time given.
    Wait(Option<Instant>),
    /// The pending challenge went unanswered.
    Expired(Pending),
    Go,
}

impl Bot {
    fn seek_step(&self) -> Step {
        let mut inner = self.inner.lock().unwrap();
        let now = Instant::now();
        let busy = inner.games.len() + inner.accepted.len();
        let max = inner.settings.max_games as usize;
        let (online, connected) = (inner.online, inner.state == BotState::Online);
        let seek = &mut inner.seek;
        if seek.mode == SeekMode::Off || !online {
            seek.mode = SeekMode::Off;
            seek.task = false;
            return Step::Stop;
        }
        if let Some(p) = &seek.pending {
            return if now >= p.expires { Step::Expired(p.clone()) } else { Step::Wait(Some(p.expires)) };
        }
        if !connected || busy >= max {
            return Step::Wait(None);
        }
        match seek.next_at {
            Some(at) if now < at => Step::Wait(Some(at)),
            _ => Step::Go,
        }
    }
}

async fn run(state: Arc<AppState>) {
    let bot = &state.lichess;
    loop {
        match bot.seek_step() {
            Step::Stop => break,
            Step::Wait(None) => bot.seek_wake.notified().await,
            Step::Wait(Some(at)) => {
                tokio::select! {
                    _ = bot.seek_wake.notified() => {}
                    _ = tokio::time::sleep_until(at) => {}
                }
            }
            Step::Expired(p) => expired(&state, p).await,
            Step::Go => attempt(&state).await,
        }
    }
}

async fn expired(state: &AppState, p: Pending) {
    let bot = &state.lichess;
    let current = {
        let mut inner = bot.inner.lock().unwrap();
        let seek = &mut inner.seek;
        let current = seek.pending.as_ref().is_some_and(|q| q.challenge.id == p.challenge.id);
        if current {
            let now = Instant::now();
            seek.pending = None;
            seek.shunned.insert(p.opponent.clone(), now);
            seek.note = Some(format!("{} did not answer. Trying another bot.", p.challenge.opponent.name));
            seek.next_at = Some(now + seek.timing.polite);
        }
        current
    };
    if current {
        if let Some(client) = bot.client() {
            withdraw(&client, &p.challenge.id).await;
        }
        bot.announce(state);
    }
}

/// Why no challenge went out, and when to try again.
struct Retry {
    note: String,
    after: Duration,
    /// A bot not to try again for a while.
    shun: Option<String>,
}

fn unreachable(e: CallError, timing: Timing) -> Retry {
    match e {
        CallError::RateLimited => Retry {
            note: "Lichess asked us to slow down; trying again in a minute.".into(),
            after: timing.rate_limited,
            shun: None,
        },
        e => Retry { note: format!("{e}; trying again in a minute."), after: timing.retry, shun: None },
    }
}

/// Picks a clock and an opponent and challenges them.
async fn challenge(state: &AppState) -> Result<Pending, Retry> {
    let bot = &state.lichess;
    let (settings, timing, perfs) = {
        let inner = bot.inner.lock().unwrap();
        (inner.settings.clone(), inner.seek.timing, inner.seek.perfs.clone())
    };
    let wait = |note: &str| Retry { note: note.to_owned(), after: timing.retry, shun: None };
    let (Some(client), Some(me)) = (bot.client(), bot.account()) else {
        return Err(wait("Not signed in to Lichess; trying again in a minute."));
    };
    let clocks: Vec<SeekClock> = settings.seek_clocks.iter().filter_map(|c| SeekClock::parse(c).ok()).collect();
    if clocks.is_empty() {
        return Err(wait("No clocks to look for games at: set some in the settings."));
    }
    let clock = clocks[roll(clocks.len())];
    let speed = clock.speed();
    let perfs = match perfs {
        Some(p) => p,
        None => {
            let p = client.get("/api/account").await.map_err(|e| unreachable(e, timing))?["perfs"].clone();
            bot.inner.lock().unwrap().seek.perfs = Some(p.clone());
            p
        }
    };
    let ours = our_rating(&perfs, speed);
    let bots: Vec<OnlineBot> = client
        .get_lines("/api/bot/online?nb=100")
        .await
        .map_err(|e| unreachable(e, timing))?
        .iter()
        .map(read_bot)
        .collect();
    let shunned: HashSet<String> = {
        let mut inner = bot.inner.lock().unwrap();
        let now = Instant::now();
        inner.seek.shunned.retain(|_, at| now.duration_since(*at) < SHUN);
        inner.seek.shunned.keys().cloned().collect()
    };
    let Some((opponent, rating)) = pick(&bots, &me.id, speed, ours, settings.seek_range, &shunned, &mut roll) else {
        return Err(wait(&format!("No bots to play at {clock} are online; trying again in a minute.")));
    };
    let form = [
        ("rated", settings.seek_rated.to_string()),
        ("clock.limit", clock.initial.to_string()),
        ("clock.increment", clock.increment.to_string()),
        ("color", "random".to_owned()),
        ("variant", "standard".to_owned()),
    ];
    let v = client.post(&format!("/api/challenge/{}", opponent.name), &form).await.map_err(|e| match e {
        CallError::Status { status: 400..=499, message } => Retry {
            note: format!("Could not challenge {}: {message}. Trying another bot.", opponent.name),
            after: timing.polite,
            shun: Some(opponent.name.to_lowercase()),
        },
        e => unreachable(e, timing),
    })?;
    let c = if v["challenge"].is_object() { &v["challenge"] } else { &v };
    let id = c["id"].as_str().unwrap_or_default().to_owned();
    Ok(Pending {
        challenge: SeekChallenge {
            url: c["url"].as_str().map(str::to_owned).unwrap_or_else(|| format!("https://lichess.org/{id}")),
            id,
            opponent: SeekOpponent { name: opponent.name.clone(), title: opponent.title.clone(), rating },
            clock,
            rated: settings.seek_rated,
        },
        opponent: opponent.name.to_lowercase(),
        expires: Instant::now() + timing.answer,
    })
}

async fn attempt(state: &AppState) {
    let bot = &state.lichess;
    let result = challenge(state).await;
    if let Some(stale) = settle_attempt(bot, result) {
        // Stopped while the challenge was on its way.
        if let Some(client) = bot.client() {
            withdraw(&client, &stale).await;
        }
        return;
    }
    bot.announce(state);
}

/// Records how an attempt went. Answers the id of a challenge to withdraw at once, when the search was stopped while
/// it was being sent.
fn settle_attempt(bot: &Bot, result: Result<Pending, Retry>) -> Option<String> {
    let mut inner = bot.inner.lock().unwrap();
    let seek = &mut inner.seek;
    let now = Instant::now();
    match result {
        Err(retry) => {
            if let Some(id) = retry.shun {
                seek.shunned.insert(id, now);
            }
            if seek.mode != SeekMode::Off {
                seek.note = Some(retry.note);
            }
            seek.next_at = Some(now + retry.after);
        }
        Ok(mut p) => {
            if seek.mode == SeekMode::Off {
                return Some(p.challenge.id);
            }
            let c = &p.challenge;
            seek.note = Some(format!(
                "Challenged {} ({}) to {} {}.",
                c.opponent.name,
                c.opponent.rating,
                c.clock,
                rated_word(c.rated)
            ));
            p.expires = now + seek.timing.answer;
            let id = c.id.clone();
            seek.pending = Some(p);
            if let Some(i) = seek.early.iter().position(|(e, _)| *e == id)
                && let Some((_, outcome)) = seek.early.remove(i)
            {
                seek.settle(&id, outcome);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot(name: &str, blitz: Option<u32>) -> OnlineBot {
        OnlineBot { name: name.into(), title: Some("BOT".into()), ratings: Ratings { blitz, ..Ratings::default() } }
    }

    fn names(bots: &[OnlineBot], shunned: &[&str], ours: u32) -> Vec<String> {
        let shunned: HashSet<String> = shunned.iter().map(|s| s.to_string()).collect();
        // Every index the roll can give, to see the whole tier.
        let mut seen = Vec::new();
        for i in 0..bots.len() {
            if let Some((b, _)) = pick(bots, "recklessbot", Speed::Blitz, ours, 300, &shunned, &mut |_| i)
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
    }

    #[test]
    fn a_provisional_or_missing_rating_counts_as_1500() {
        let perfs = serde_json::json!({
            "blitz": { "rating": 1873, "prov": false },
            "bullet": { "rating": 2100, "prov": true },
        });
        assert_eq!(our_rating(&perfs, Speed::Blitz), 1873);
        assert_eq!(our_rating(&perfs, Speed::Bullet), 1500);
        assert_eq!(our_rating(&perfs, Speed::Rapid), 1500);
    }

    #[test]
    fn an_early_outcome_waits_for_its_challenge() {
        let mut s = Seeking { mode: SeekMode::Once, ..Seeking::default() };
        assert!(!s.settle("x", Outcome::Declined(Some("tooFast".into()))));
        assert_eq!(s.early.len(), 1);
        s.pending = Some(Pending {
            challenge: SeekChallenge {
                id: "x".into(),
                url: String::new(),
                opponent: SeekOpponent { name: "Other".into(), title: None, rating: 1500 },
                clock: SeekClock { initial: 180, increment: 2 },
                rated: true,
            },
            opponent: "other".into(),
            expires: Instant::now(),
        });
        let (id, outcome) = s.early.pop_front().unwrap();
        assert!(s.settle(&id, outcome));
        assert_eq!(s.note.as_deref(), Some("Other declined (too fast). Trying another bot."));
        assert!(s.shunned.contains_key("other"));
    }
}
