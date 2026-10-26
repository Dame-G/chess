//! Dameless's levels as a service: the ladder, the calibrated ratings (`GET /api/levels`), the plan of calibration
//! matches, and the refit after games at a level. The maths is `core::levels`; the games are played by [`Games`]
//! (yours) and the [`crate::Calibrate`] executor (levels against each other).
//!
//! Your Lichess rating anchors the ladder on the human scale. [`Levels`] reads it through a source the server sets
//! ([`Levels::set_anchor`], usually [`your_anchor`] over the you connection's status), since the connections are made
//! after the games.
//!
//! [`Games`]: crate::Games

use std::sync::{Arc, Mutex};

use dameless_core::{
    Error, PlayerKey, Timestamp,
    events::{LevelsUpdated, Payload},
    levels::{self, Entrant, EntrantRating, Level, PlannedMatch},
    lichess::ConnectionStatus,
    version::Estimate,
    work::{Priority, Subject, WorkKind},
};
use dameless_hub::Hub;
use dameless_store::{NewWork, now};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::binaries::Binaries;

/// Where your rating (and its deviation, when known) comes from: `None` without a Lichess account.
pub type AnchorSource = Arc<dyn Fn() -> Option<(u32, Option<f64>)> + Send + Sync>;

/// Calibration matches queued when the request names none.
pub const DEFAULT_MATCHES: usize = 3;

/// One level as `GET /api/levels` lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct LevelRating {
    pub level: Level,
    /// The calibrated rating, once the level has games; else `null` (the UI says "about <nominal>").
    pub rating: Option<Estimate>,
    pub games: u32,
}

/// `GET /api/levels`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct LevelsOverview {
    pub levels: Vec<LevelRating>,
    /// Your fitted rating on the ladder's scale, once there is a fit with you in it.
    pub you: Option<EntrantRating>,
    /// The level nearest your rating (fitted, else Lichess's): the New game dialog's default.
    pub suggested: Option<u32>,
    /// When the last fit was made.
    pub updated_at: Option<Timestamp>,
}

/// `POST /api/levels/calibrate`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CalibrateRequest {
    /// These matches; else the plan's `count` most useful ones.
    #[serde(default)]
    pub matches: Option<Vec<PlannedMatch>>,
    /// How many planned matches, when `matches` is not given. Default 3.
    #[serde(default)]
    pub count: Option<usize>,
    /// Games per match, overriding each match's own (`core::levels::CALIBRATION_GAMES`, 20).
    #[serde(default)]
    pub games_per_pair: Option<u32>,
    /// The version every level plays; the champion when none.
    #[serde(default)]
    pub version: Option<String>,
}

/// A calibration as queued: the request resolved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CalibrateSpec {
    pub matches: Vec<PlannedMatch>,
    /// The version's name, as resolved when queued.
    pub version: String,
}

struct Inner {
    hub: Arc<Hub>,
    ladder: Vec<Level>,
    anchor: Mutex<Option<AnchorSource>>,
}

/// The levels. Cheap to clone; [`crate::Games`] makes one (`Games::levels`).
#[derive(Clone)]
pub struct Levels {
    inner: Arc<Inner>,
}

impl Levels {
    /// The default ladder (`core::levels::ladder`).
    pub fn new(hub: Arc<Hub>) -> Self {
        Self::with_ladder(hub, levels::ladder())
    }

    /// Another ladder, for tests.
    pub fn with_ladder(hub: Arc<Hub>, ladder: Vec<Level>) -> Self {
        Self { inner: Arc::new(Inner { hub, ladder, anchor: Mutex::new(None) }) }
    }

    pub fn ladder(&self) -> &[Level] {
        &self.inner.ladder
    }

    /// A level by id: 400 for one the ladder does not have.
    pub fn level(&self, id: u32) -> Result<&Level, Error> {
        self.inner
            .ladder
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| Error::bad_request(format!("no level {id}: levels are 1 to {}", self.inner.ladder.len())))
    }

    /// Full strength: the ladder's level without a node budget, else its last.
    pub fn full(&self) -> Option<&Level> {
        self.inner.ladder.iter().find(|l| l.is_full()).or(self.inner.ladder.last())
    }

    /// Sets where your rating comes from (the server: [`your_anchor`] over the you connection's status).
    pub fn set_anchor(&self, source: AnchorSource) {
        *self.inner.anchor.lock().unwrap() = Some(source);
    }

    fn anchor(&self) -> Option<(u32, Option<f64>)> {
        let source = self.inner.anchor.lock().unwrap().clone();
        source.and_then(|f| f())
    }

    /// The ladder with its calibrated ratings, your place on it and the level to suggest.
    pub async fn overview(&self) -> Result<LevelsOverview, Error> {
        let (ratings, updated_at) = self.inner.hub.store.level_ratings().await?;
        let fitted = |id: u32| ratings.iter().find(|r| r.entrant == Entrant::Level(id) && r.games > 0);
        let levels = self
            .inner
            .ladder
            .iter()
            .map(|l| LevelRating {
                level: l.clone(),
                rating: fitted(l.id).map(|r| r.estimate),
                games: fitted(l.id).map_or(0, |r| r.games),
            })
            .collect();
        let you = ratings.iter().find(|r| r.entrant == Entrant::You).copied();
        let target = you.map(|r| r.estimate.elo).or_else(|| self.anchor().map(|(r, _)| f64::from(r)));
        let suggested = target.and_then(|t| levels::nearest_level(&self.effective(&ratings), t));
        Ok(LevelsOverview { levels, you, suggested, updated_at })
    }

    /// Each level's rating as it stands: calibrated once it has games, else its nominal one.
    fn effective(&self, ratings: &[EntrantRating]) -> Vec<EntrantRating> {
        self.inner
            .ladder
            .iter()
            .map(|l| {
                ratings.iter().find(|r| r.entrant == Entrant::Level(l.id) && r.games > 0).copied().unwrap_or(
                    EntrantRating {
                        entrant: Entrant::Level(l.id),
                        estimate: Estimate::plus_minus(f64::from(l.nominal), 0.0),
                        games: 0,
                    },
                )
            })
            .collect()
    }

    /// The level's rating to show, rounded: calibrated once it has games, else nominal.
    pub async fn rating_of(&self, level: &Level) -> u32 {
        let fitted = self.inner.hub.store.level_ratings().await.ok().and_then(|(ratings, _)| {
            ratings.into_iter().find(|r| r.entrant == Entrant::Level(level.id) && r.games > 0)
        });
        fitted.map_or(level.nominal, |r| r.estimate.elo.round().max(0.0) as u32)
    }

    /// "Dameless 1600", and the rating.
    pub(crate) async fn seat_name(&self, level: &Level) -> (String, u32) {
        let rating = self.rating_of(level).await;
        (format!("Dameless {rating}"), rating)
    }

    /// The `count` calibration matches to play next (`core::levels::plan` over the last fit).
    pub async fn plan(&self, count: usize) -> Result<Vec<PlannedMatch>, Error> {
        let (ratings, _) = self.inner.hub.store.level_ratings().await?;
        Ok(levels::plan(&self.inner.ladder, &ratings, count))
    }

    /// Fits the ladder again over every finished game at a level, with your rating as the anchor, stores the fit and
    /// announces it (`levels.updated`).
    pub async fn refit(&self) -> Result<LevelsUpdated, Error> {
        let store = &self.inner.hub.store;
        let games = store.level_games().await?;
        let results = levels::pair_results(&games);
        let priors = levels::priors(&self.inner.ladder, self.anchor());
        let ratings = levels::fit(&priors, &results);
        store.put_level_ratings(ratings.clone()).await?;
        let updated = LevelsUpdated { ratings, updated_at: now() };
        self.inner.hub.bus.emit(Payload::LevelsUpdated(updated.clone()));
        Ok(updated)
    }

    /// [`Levels::refit`], a failure only logged: for the end of a game.
    pub(crate) async fn refit_quietly(&self) {
        if let Err(e) = self.refit().await {
            tracing::warn!("could not refit the levels: {e}");
        }
    }

    /// The calibration work for a request, resolved now: the matches (planned when not given) and the version. The
    /// caller queues it with the scheduler (`Scheduler::submit`).
    pub async fn prepare_calibration(&self, binaries: &dyn Binaries, req: CalibrateRequest) -> Result<NewWork, Error> {
        let mut matches = match req.matches {
            Some(m) => m,
            None => self.plan(req.count.unwrap_or(DEFAULT_MATCHES).clamp(1, 64)).await?,
        };
        if matches.is_empty() {
            return Err(Error::bad_request("no matches to play: the ladder needs two levels"));
        }
        for m in &mut matches {
            self.level(m.a)?;
            self.level(m.b)?;
            if m.a == m.b {
                return Err(Error::bad_request("a calibration match is between two levels"));
            }
            if let Some(n) = req.games_per_pair {
                m.games = n;
            }
            if m.games == 0 || m.games > 1000 {
                return Err(Error::bad_request("a match plays 1 to 1000 games"));
            }
        }
        let version = binaries.resolve(req.version.as_deref().unwrap_or("champion")).await?;
        let games: u32 = matches.iter().map(|m| m.games).sum();
        let pairs = matches.iter().map(|m| format!("{}–{}", m.a, m.b)).collect::<Vec<_>>().join(", ");
        let spec = CalibrateSpec { matches, version: version.name.clone() };
        Ok(NewWork {
            kind: WorkKind::Calibrate,
            priority: Priority::Idea,
            title: format!("Calibrating levels {pairs} · {games} games"),
            subject: Subject::Player { key: PlayerKey::Dameless },
            spec: serde_json::to_value(&spec).map_err(|e| Error::internal(e.to_string()))?,
        })
    }
}

/// Your rating as the ladder's anchor, from the you connection's status: rapid, else blitz, classical, bullet, an
/// established rating before a provisional one. Lichess's deviation is not known here, so the default
/// (`core::levels::ANCHOR_SD`) applies.
pub fn your_anchor(status: &ConnectionStatus) -> Option<(u32, Option<f64>)> {
    let r = &status.account.as_ref()?.ratings;
    let order = [r.rapid, r.blitz, r.classical, r.bullet];
    let pick = order.iter().flatten().find(|x| !x.provisional).or_else(|| order.iter().flatten().next())?;
    Some((pick.rating, None))
}
