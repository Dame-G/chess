//! Drills: the positions before your mistakes, to find the move you missed, on a spaced schedule. The rules are
//! `core::drills` (making, checking, SM-2 grading, today's set, the hit rate); the rows are `store::drills`. This
//! service puts them together and announces every change on `player:you` (`drill.upserted`, `drill.answered`,
//! `drill.deleted`).
//!
//! Drills are made from your reviewed games: on request ([`Drills::make_from_weak_spots`]) and after each review of one
//! of your games ([`Drills::make_from_game`], which [`crate::Reviews`] calls when given the service). Each weak spot's
//! correct moves are the reviewed move's best and second-best lines (`core::drills::lines_of`) within 5 win%.

use std::sync::Arc;

use dameless_core::{
    Error, GameId, PlayerKey, Timestamp,
    drills::{self, Drill, DrillAnswer, DrillId, DrillOutcome, HitRatePoint, NewDrill},
    events::{DrillAnswered, DrillDeleted, Payload},
    profile::{self, ProfileFilter, ProfileGame},
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
    /// In the order to play them. The client shows `fen`, `side` and `theme` only until the drill is answered.
    pub drills: Vec<Drill>,
    /// How many of them are tried drills due again, and how many new.
    pub due: u32,
    pub new: u32,
    /// New drills started today, counted against the day's cap.
    pub started_today: u32,
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
}

impl Drills {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }

    /// Every drill, the soonest due first.
    pub async fn list(&self) -> Result<Vec<Drill>, Error> {
        self.hub.store.drills().await
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
            self.hub.bus.emit(Payload::DrillUpserted(Box::new(d.clone())));
        }
        Ok(made)
    }

    /// Today's drills at `now`: due tried drills first, then new ones up to `new_per_day` (10) a day, at most `limit`
    /// (30) in all.
    pub async fn today(
        &self,
        now: &Timestamp,
        new_per_day: Option<u32>,
        limit: Option<usize>,
    ) -> Result<DrillsToday, Error> {
        let store = &self.hub.store;
        let all = store.drills().await?;
        let started_today = store.drills_started_since(drills::day_start(now)).await?;
        let ids = drills::today(
            &all,
            now,
            new_per_day.unwrap_or(drills::NEW_PER_DAY),
            started_today,
            limit.unwrap_or(drills::DAY_LIMIT),
        );
        let picked: Vec<Drill> = ids.iter().filter_map(|id| all.iter().find(|d| d.id == *id).cloned()).collect();
        let new = picked.iter().filter(|d| d.is_new()).count() as u32;
        Ok(DrillsToday { due: picked.len() as u32 - new, new, started_today, drills: picked })
    }

    /// Answers drill `id` with `answer` (UCI or SAN) at `now`: checked, graded by SM-2, stored with the answer, and
    /// announced (`drill.answered`, then `drill.upserted`). 404 for no such drill.
    pub async fn answer(&self, id: DrillId, answer: &str, now: Timestamp) -> Result<DrillAnswerResult, Error> {
        let drill = self.get(id).await?;
        let outcome = drills::check(&drill, answer);
        let graded = drills::grade(&drill, outcome, &now);
        let typed = dameless_core::chess::Position::from_fen(&drill.fen)
            .and_then(|p| p.to_uci(answer))
            .unwrap_or_else(|_| answer.trim().to_owned());
        let record = DrillAnswer { drill: id, at: now.clone(), answer: typed, outcome };
        let stored = self.hub.store.record_drill_answer(graded, record).await?;
        self.hub.bus.emit(Payload::DrillAnswered(DrillAnswered { drill: id, outcome, at: now }));
        self.hub.bus.emit(Payload::DrillUpserted(Box::new(stored.clone())));
        Ok(DrillAnswerResult {
            outcome,
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
    let player = profile::Player {
        key: PlayerKey::You,
        name: "You".into(),
        title: None,
        ratings: Default::default(),
        lichess: None,
    };
    let one = std::slice::from_ref(g);
    let spots = profile::profile(player, ProfileFilter::default(), one, true).weak_spots;
    spots
        .iter()
        .filter_map(|spot| {
            let lines = review.plies.iter().find(|p| p.ply == spot.ply).map(drills::lines_of).unwrap_or_default();
            drills::new_drill(spot, &lines).ok()
        })
        .collect()
}
