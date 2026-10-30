//! Drills: the positions before your mistakes, to find the move you missed, on a spaced schedule. The rules are
//! `core::drills` (making, checking, SM-2 grading, today's set, sessions, hints, the hit rate); the rows are
//! `store::drills`. This service puts them together and announces every change on `player:you` (`drill.upserted`,
//! `drill.answered`, `drill.deleted`).
//!
//! Drills are made from your reviewed games: on request ([`Drills::make_from_weak_spots`]) and after each review of one
//! of your games ([`Drills::make_from_game`], which [`crate::Reviews`] calls when given the service). Each weak spot's
//! correct moves are the reviewed move's best and second-best lines (`core::drills::lines_of`) within 5 win%.
//!
//! A drill goes out as a question: the position and its theme, without the moves that answer it (`Drill::question`),
//! in lists and in events. The answer comes back with the answer, which also tells whether a hint was used: hints are
//! remembered here until the drill is answered.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use dameless_core::{
    Error, GameId, PlayerKey, Timestamp,
    drills::{self, Drill, DrillAnswer, DrillHint, DrillId, DrillOutcome, HitRatePoint, NewDrill, SessionFilter},
    events::{DrillAnswered, DrillDeleted, Payload},
    profile::{self, ProfileFilter, ProfileGame},
    themes::ThemeKind,
};
use dameless_hub::Hub;
use dameless_store::now;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Drills `POST /api/drills` makes when the request gives no `limit`.
pub const DEFAULT_MAKE: usize = 20;

/// Your games `POST /api/drills` looks through, newest first.
pub const GAMES_LOOKED_AT: u32 = 500;

/// Answers the hit rate's rolling mean takes when the request gives no `window`.
pub const DEFAULT_TREND_WINDOW: usize = 50;

/// `GET /api/drills/today`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DrillsToday {
    /// In the order to play them: questions, without their answers.
    pub drills: Vec<Drill>,
    /// How many of them are tried drills due again, and how many new.
    pub due: u32,
    pub new: u32,
    /// New drills started today, counted against the day's cap.
    pub started_today: u32,
    /// Every drill there is, to tell "none yet" from "none today".
    pub total: u32,
}

/// What `GET /api/drills/today` can narrow a session to, besides the day's schedule.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionRequest {
    /// New drills a day (10), for the day's own set.
    pub new_per_day: Option<u32>,
    /// At most this many in all (30 for the day's set, 10 for a theme or line).
    pub limit: Option<usize>,
    /// Only this theme (`hanging_piece`, …): a session on it, whatever is due.
    pub theme: Option<ThemeKind>,
    /// Only drills from these games and by this ply: the games of an opening line (`PrepLine::drill_games`).
    pub games: Vec<GameId>,
    pub max_ply: Option<u32>,
}

/// `POST /api/drills/{id}/answer`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AnswerRequest {
    /// UCI or SAN.
    #[serde(rename = "move")]
    pub mv: String,
}

/// What an answer came to: the outcome, the drill rescheduled, and the answer itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DrillAnswerResult {
    pub outcome: DrillOutcome,
    /// A hint was asked for first: a pass then counts half and the drill comes back tomorrow.
    pub hinted: bool,
    pub drill: Drill,
    pub best_moves: Vec<String>,
    pub best_san: Vec<String>,
    pub line: Vec<String>,
    pub played_san: String,
}

/// `POST /api/drills`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct MakeDrillsRequest {
    /// At most this many new drills (20 when none).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// The drills. Cheap to clone.
#[derive(Clone)]
pub struct Drills {
    hub: Arc<Hub>,
    /// The deepest hint asked for each drill not answered yet.
    hints: Arc<Mutex<HashMap<DrillId, u8>>>,
    /// Held while drills from before themes are classified, so a request that needs them waits for the one running.
    classifying: Arc<tokio::sync::Mutex<()>>,
}

impl Drills {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub, hints: Arc::default(), classifying: Arc::default() }
    }

    /// Every drill as a question, the soonest due first.
    pub async fn list(&self) -> Result<Vec<Drill>, Error> {
        Ok(self.hub.store.drills().await?.iter().map(Drill::question).collect())
    }

    pub async fn get(&self, id: DrillId) -> Result<Drill, Error> {
        self.hub.store.drill(id).await?.ok_or_else(|| Error::not_found(format!("No drill {id}")))
    }

    /// New drills from the weak spots of your reviewed games (the newest [`GAMES_LOOKED_AT`]), the biggest swings
    /// first, at most `limit`: one per position, none already a drill. Returns the drills made.
    pub async fn make_from_weak_spots(&self, limit: Option<usize>) -> Result<Vec<Drill>, Error> {
        let limit = limit.unwrap_or(DEFAULT_MAKE).clamp(1, 500);
        let filter = ProfileFilter { limit: GAMES_LOOKED_AT, ..ProfileFilter::default() };
        let games = self.hub.store.profile_games(PlayerKey::You, filter, true).await?;
        let mut candidates: Vec<NewDrill> = games.iter().flat_map(candidates_of).collect();
        candidates.sort_by(|a, b| b.swing.total_cmp(&a.swing));
        let existing = self.hub.store.drill_keys().await?;
        let mut fresh = drills::dedupe(&existing, candidates);
        fresh.truncate(limit);
        self.insert(fresh).await
    }

    /// New drills from one reviewed game of yours (none when it is not yours or not reviewed). Called after a review.
    pub async fn make_from_game(&self, game: GameId) -> Result<Vec<Drill>, Error> {
        let store = &self.hub.store;
        let Some(full) = store.game(game).await? else { return Ok(Vec::new()) };
        if full.summary.side_of(&PlayerKey::You).is_none() {
            return Ok(Vec::new());
        }
        let Some(review) = store.review(game, None, None).await? else { return Ok(Vec::new()) };
        let pg = ProfileGame { summary: full.summary, start_fen: full.start_fen, review: Some(review) };
        let existing = store.drill_keys().await?;
        self.insert(drills::dedupe(&existing, candidates_of(&pg))).await
    }

    async fn insert(&self, fresh: Vec<NewDrill>) -> Result<Vec<Drill>, Error> {
        if fresh.is_empty() {
            return Ok(Vec::new());
        }
        let made = self.hub.store.insert_drills(fresh).await?;
        for d in &made {
            self.hub.bus.emit(Payload::DrillUpserted(Box::new(d.question())));
        }
        Ok(made.iter().map(Drill::question).collect())
    }

    /// Finds the theme of every drill that has none yet (from before themes), game by game, from the weak spots of the
    /// game's review: the same finder a new drill's theme comes from. A drill whose game or review is gone is
    /// `other`, so it is not looked at again. Returns how many were classified. The server runs it once at start; a
    /// session on a theme waits for it.
    pub async fn classify_pending(&self) -> Result<usize, Error> {
        let _one = self.classifying.lock().await;
        let store = &self.hub.store;
        let pending = store.unclassified_drills().await?;
        let mut done = 0;
        let mut i = 0;
        while i < pending.len() {
            let game = pending[i].1;
            let end = pending[i..].iter().position(|p| p.1 != game).map_or(pending.len(), |n| i + n);
            let spots = match (store.game(game).await?, store.review(game, None, None).await?) {
                (Some(full), Some(review)) => full.summary.side_of(&PlayerKey::You).map(|side| {
                    let pg =
                        ProfileGame { summary: full.summary, start_fen: full.start_fen, review: Some(review.clone()) };
                    profile::weak_spots_of(&pg, &review, side)
                }),
                _ => None,
            }
            .unwrap_or_default();
            let found = pending[i..end]
                .iter()
                .map(|(id, _, ply)| {
                    let theme = spots.iter().find(|s| s.ply == *ply).map(|s| (s.kind, s.detail.clone()));
                    let (kind, detail) = theme.unwrap_or((ThemeKind::Other, None));
                    (*id, kind, detail)
                })
                .collect::<Vec<_>>();
            done += found.len();
            store.set_drill_themes(found).await?;
            i = end;
        }
        Ok(done)
    }

    /// Today's drills at `now`, as questions. Without a theme or line: due tried drills first, then new ones up to
    /// `new_per_day` (10) a day, at most `limit` (30) in all. With a theme or a line (`games`): a session on it
    /// (`core::drills::session`), at most `limit` (10), whatever the day's cap says.
    pub async fn today(&self, now: &Timestamp, req: &SessionRequest) -> Result<DrillsToday, Error> {
        let filter = SessionFilter { theme: req.theme, games: req.games.clone(), max_ply: req.max_ply };
        if filter.theme.is_some() {
            self.classify_pending().await?;
        }
        let store = &self.hub.store;
        let all = store.drills().await?;
        let started_today = store.drills_started_since(drills::day_start(now)).await?;
        let ids = if filter.is_empty() {
            drills::today(
                &all,
                now,
                req.new_per_day.unwrap_or(drills::NEW_PER_DAY),
                started_today,
                req.limit.unwrap_or(drills::DAY_LIMIT),
            )
        } else {
            drills::session(&all, &filter, now, req.limit.unwrap_or(drills::SESSION_LIMIT))
        };
        let by_id: HashMap<DrillId, &Drill> = all.iter().map(|d| (d.id, d)).collect();
        let picked: Vec<Drill> = ids.iter().filter_map(|id| by_id.get(id).map(|d| d.question())).collect();
        let new = picked.iter().filter(|d| d.is_new()).count() as u32;
        Ok(DrillsToday { due: picked.len() as u32 - new, new, started_today, total: all.len() as u32, drills: picked })
    }

    /// The next hint at drill `id`: what kind of move first, then which piece. Asking counts: the answer, if right,
    /// is half a miss. 404 for no such drill.
    pub async fn hint(&self, id: DrillId) -> Result<DrillHint, Error> {
        let drill = self.get(id).await?;
        let level = {
            let mut hints = self.hints.lock().expect("hints lock");
            let level = hints.get(&id).map_or(1, |l| (l + 1).min(drills::HINT_LEVELS));
            hints.insert(id, level);
            level
        };
        drills::hint(&drill, level)
    }

    /// Answers drill `id` with `answer` (UCI or SAN) at `now`: checked, graded by SM-2, stored with the answer, and
    /// announced (`drill.answered`, then `drill.upserted`). 404 for no such drill.
    pub async fn answer(&self, id: DrillId, answer: &str, now: Timestamp) -> Result<DrillAnswerResult, Error> {
        let drill = self.get(id).await?;
        let outcome = drills::check(&drill, answer);
        let asked = self.hints.lock().expect("hints lock").remove(&id).is_some();
        let hinted = asked && outcome == DrillOutcome::Pass;
        let graded = drills::grade(&drill, outcome, hinted, &now);
        let typed = dameless_core::chess::Position::from_fen(&drill.fen)
            .and_then(|p| p.to_uci(answer))
            .unwrap_or_else(|_| answer.trim().to_owned());
        let record = DrillAnswer { drill: id, at: now.clone(), answer: typed, outcome, hinted };
        let stored = self.hub.store.record_drill_answer(graded, record).await?;
        self.hub.bus.emit(Payload::DrillAnswered(DrillAnswered { drill: id, outcome, at: now }));
        self.hub.bus.emit(Payload::DrillUpserted(Box::new(stored.question())));
        Ok(DrillAnswerResult {
            outcome,
            hinted,
            best_moves: stored.best_moves.clone(),
            best_san: stored.best_san.clone(),
            line: stored.line.clone(),
            played_san: stored.played_san.clone(),
            drill: stored,
        })
    }

    /// [`Drills::answer`] now.
    pub async fn answer_now(&self, id: DrillId, answer: &str) -> Result<DrillAnswerResult, Error> {
        self.answer(id, answer, now()).await
    }

    /// Removes a drill and its answers (`drill.deleted`). 404 for no such drill.
    pub async fn delete(&self, id: DrillId) -> Result<(), Error> {
        if !self.hub.store.delete_drill(id).await? {
            return Err(Error::not_found(format!("No drill {id}")));
        }
        self.hints.lock().expect("hints lock").remove(&id);
        self.hub.bus.emit(Payload::DrillDeleted(DrillDeleted { id }));
        Ok(())
    }

    /// Your hit rate per day, with a rolling mean over `window` answers (50).
    pub async fn trend(&self, window: Option<usize>) -> Result<Vec<HitRatePoint>, Error> {
        let answers = self.hub.store.drill_answers(None).await?;
        Ok(drills::trend(&answers, window.unwrap_or(DEFAULT_TREND_WINDOW).max(1)))
    }
}

/// The drills one reviewed game of yours offers: each weak spot with the reviewed move's lines. Spots that cannot make
/// a drill (the best move was the move played) are left out.
fn candidates_of(g: &ProfileGame) -> Vec<NewDrill> {
    let Some(review) = &g.review else { return Vec::new() };
    let Some(side) = g.summary.side_of(&PlayerKey::You) else { return Vec::new() };
    profile::weak_spots_of(g, review, side)
        .iter()
        .filter_map(|spot| {
            let lines = review.plies.iter().find(|p| p.ply == spot.ply).map(drills::lines_of).unwrap_or_default();
            drills::new_drill(spot, &lines).ok()
        })
        .collect()
}
