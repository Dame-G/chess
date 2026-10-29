//! Reviews: the engine reading every move of finished games, as work.
//!
//! [`Reviews::prepare`] turns a request (some games, or a player's newest games not yet reviewed) into the work item the
//! scheduler queues, resolved when queued: the games, the version, the budget. [`Reviews`] is also the work's
//! [`Executor`]: for each game it takes the positions from `core`'s [`to_search`], has the uncached ones searched on as
//! many engines as the ledger grants, judges the game with `core`'s [`judge_game`], stores the review and announces it.
//! It checkpoints after each game, so a restart carries on with the next one, and it stops at a safe point before each
//! batch of positions, so it pauses while you play on Lichess.

use std::time::Instant;

use async_trait::async_trait;
use dameless_core::{
    Error, GameId, PlayerKey, Speed, VersionId,
    events::{AssistKind, Payload, ReviewUpserted},
    game::GameStatus,
    review::{self, Review, ReviewInput, judge_game, to_search},
    version::Version,
    work::{Priority, Progress, Subject, Work, WorkKind},
};
use dameless_hub::{Executor, WorkContext};
use dameless_store::{GameFilter, NewWork};
use dameless_work::Scheduler;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{Eval, drills::Drills, versions::VersionRef};

/// The smallest and largest node budgets a review takes.
pub const NODES_RANGE: std::ops::RangeInclusive<u64> = 1_000..=100_000_000;

/// The most games one review of a player takes.
pub const MAX_LIMIT: u32 = 1000;

/// Positions searched per batch, per engine: between batches the review may pause, and the ledger may grant more or
/// fewer cores.
const POSITIONS_PER_ENGINE: usize = 4;

/// `POST /api/reviews`: some games, or a player's newest games not yet reviewed with this version and budget.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReviewRequest {
    /// These games. Or, instead, `player`.
    #[serde(default)]
    pub games: Vec<GameId>,
    /// This player's newest finished games not yet reviewed with this version and budget.
    #[serde(default)]
    pub player: Option<PlayerKey>,
    /// How many of the player's games; 20 when none, at most 1000. In the background: every one when none.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Review in the background: at the lowest priority, below ideas, so trials keep their cores, and in a lane of its
    /// own, so a review asked for meanwhile runs at once. For long reviews: with `player` and no `limit`, every game
    /// of theirs not reviewed yet, newest first.
    #[serde(default)]
    pub background: bool,
    /// Only the player's games at these speeds.
    #[serde(default)]
    pub speeds: Vec<Speed>,
    /// Only the player's rated (true) or casual (false) games.
    #[serde(default)]
    pub rated: Option<bool>,
    /// The version to review with; the champion when none.
    #[serde(default)]
    pub version: Option<VersionRef>,
    /// Nodes per position; 200,000 when none.
    #[serde(default)]
    pub nodes: Option<u64>,
    /// Engines at once, one thread each; half the machine's cores when none.
    #[serde(default)]
    pub concurrency: Option<u32>,
}

/// A review as queued: the request resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReviewSpec {
    pub games: Vec<GameId>,
    pub version: VersionId,
    pub nodes: u64,
    pub multipv: u32,
    pub concurrency: u32,
}

/// Where a review stands: the next game to review and the counts so far.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewCheckpoint {
    pub next: usize,
    pub reviewed: u32,
    pub skipped: u32,
    pub failed: u32,
    pub searched: u64,
    pub cached: u64,
}

/// What a finished review did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReviewResult {
    pub reviewed: u32,
    pub skipped: u32,
    pub failed: u32,
    pub positions_searched: u64,
    pub positions_cached: u64,
    pub version: VersionId,
    pub nodes: u64,
}

/// What reviewing one game came to.
enum Outcome {
    Reviewed { review: Box<Review>, searched: usize, cached: usize },
    Skipped(&'static str),
    Stopped,
}

/// Queues and runs reviews. Cheap to clone; register it with the scheduler as the `review` executor.
#[derive(Clone)]
pub struct Reviews {
    eval: Eval,
    drills: Option<Drills>,
}

impl Reviews {
    pub fn new(eval: Eval) -> Self {
        Self { eval, drills: None }
    }

    /// Makes drills from each of your games once it is reviewed.
    pub fn with_drills(mut self, drills: Drills) -> Self {
        self.drills = Some(drills);
        self
    }

    /// The work item for a request, resolved now: the games (checked to exist), the version, the budget and the
    /// engines. Refused (409, fair play) while you play on Lichess. The caller queues it with the scheduler.
    pub async fn prepare(&self, req: ReviewRequest) -> Result<NewWork, Error> {
        let hub = self.eval.hub();
        hub.policy.check(AssistKind::Review)?;
        let nodes = req.nodes.unwrap_or(review::DEFAULT_NODES);
        if !NODES_RANGE.contains(&nodes) {
            return Err(Error::bad_request("nodes must be between 1,000 and 100,000,000"));
        }
        let cores = hub.ledger.total();
        let concurrency = req.concurrency.unwrap_or((cores / 2).max(1)).clamp(1, cores);
        let version = self.eval.binaries().resolve(&req.version.clone().unwrap_or(VersionRef::Champion)).await?;
        let (games, title, subject) = match (&req.player, req.games.is_empty()) {
            (None, false) => {
                let mut first = None;
                for &id in &req.games {
                    let summary =
                        hub.store.game_summary(id).await?.ok_or_else(|| Error::not_found(format!("No game {id}")))?;
                    first.get_or_insert(summary);
                }
                let first = first.expect("at least one game");
                let title = if req.games.len() == 1 {
                    format!("Reviewing {} vs {}", first.white.name, first.black.name)
                } else {
                    format!("Reviewing {} games", req.games.len())
                };
                (req.games.clone(), title, Subject::Game { id: first.id })
            }
            (Some(player), true) => {
                let limit = match (req.limit, req.background) {
                    (None, true) => u32::MAX,
                    (limit, _) => limit.unwrap_or(review::DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
                };
                let games = self.unreviewed(player, &req.speeds, req.rated, version.id, nodes, limit).await?;
                let whose = match player {
                    PlayerKey::You => "your".to_owned(),
                    PlayerKey::Dameless => "Dameless's".to_owned(),
                    other => format!("{other}'s"),
                };
                let n = games.len();
                let title = if limit == u32::MAX {
                    format!("Reviewing all {n} of {whose} games")
                } else {
                    format!("Reviewing {n} of {whose} games")
                };
                (games, title, Subject::Player { key: player.clone() })
            }
            _ => return Err(Error::bad_request("give games, or a player")),
        };
        let spec = ReviewSpec { games, version: version.id, nodes, multipv: review::MULTIPV, concurrency };
        Ok(NewWork {
            kind: WorkKind::Review,
            priority: if req.background { Priority::Background } else { Priority::Review },
            title,
            subject,
            spec: serde_json::to_value(&spec).map_err(|e| Error::internal(e.to_string()))?,
        })
    }

    /// Prepares a review and queues it: what `POST /api/reviews` does.
    pub async fn submit(&self, scheduler: &Scheduler, req: ReviewRequest) -> Result<Work, Error> {
        scheduler.submit(self.prepare(req).await?).await
    }

    /// A game's newest review, or the one with this version and budget.
    pub async fn get(
        &self,
        game: GameId,
        version: Option<&VersionRef>,
        nodes: Option<u64>,
    ) -> Result<Option<Review>, Error> {
        let version = match version {
            Some(r) => Some(self.eval.binaries().resolve(r).await?.id),
            None => None,
        };
        self.eval.hub().store.review(game, version, nodes).await
    }

    /// The player's newest finished games with moves and no review with this version and budget, newest first.
    async fn unreviewed(
        &self,
        player: &PlayerKey,
        speeds: &[Speed],
        rated: Option<bool>,
        version: VersionId,
        nodes: u64,
        limit: u32,
    ) -> Result<Vec<GameId>, Error> {
        let store = &self.eval.hub().store;
        let mut out = Vec::new();
        let mut before = None;
        loop {
            let page = store
                .games(GameFilter {
                    player: Some(player.clone()),
                    statuses: vec![GameStatus::Finished],
                    speeds: speeds.to_vec(),
                    rated,
                    before,
                    limit: 200,
                    ..Default::default()
                })
                .await?;
            let Some(last) = page.last() else { break };
            before = Some(last.id);
            for g in &page {
                if g.plies > 0 && !store.has_review(g.id, version, nodes).await? {
                    out.push(g.id);
                    if out.len() as u32 >= limit {
                        return Ok(out);
                    }
                }
            }
            if page.len() < 200 {
                break;
            }
        }
        Ok(out)
    }

    /// Reviews one game: skips it when it is gone, live, empty or already reviewed with this version and budget;
    /// otherwise searches its positions in batches (pausing between them as the policy and the scheduler ask),
    /// judges it and stores the review.
    async fn review_game(
        &self,
        cx: &WorkContext,
        id: GameId,
        version: &Version,
        spec: &ReviewSpec,
    ) -> Result<Outcome, Error> {
        let store = &self.eval.hub().store;
        let Some(game) = store.game(id).await? else { return Ok(Outcome::Skipped("no such game")) };
        if matches!(game.summary.status, GameStatus::Live | GameStatus::Starting) {
            return Ok(Outcome::Skipped("still being played"));
        }
        if game.moves.is_empty() {
            return Ok(Outcome::Skipped("no moves"));
        }
        if store.has_review(id, version.id, spec.nodes).await? {
            return Ok(Outcome::Skipped("already reviewed"));
        }
        // The binary first, holding no cores: when it must be built, the build is work that needs them.
        self.eval.binaries().binary(&VersionRef::Id(version.id), &[]).await?;
        let input = ReviewInput::from(&game);
        let positions = to_search(input)?;
        let mut evals = Vec::with_capacity(positions.len());
        let (mut searched, mut cached) = (0, 0);
        let mut at = 0;
        while at < positions.len() {
            if !cx.safe_point(Some(AssistKind::Review)).await {
                return Ok(Outcome::Stopped);
            }
            let Some(grant) = cx.cores(1, spec.concurrency).await else { return Ok(Outcome::Stopped) };
            let engines = grant.cores() as usize;
            let end = (at + engines * POSITIONS_PER_ENGINE).min(positions.len());
            let batch = &positions[at..end];
            let found = tokio::select! {
                found = self.eval.evaluate_positions(batch, version, spec.nodes, spec.multipv, engines) => found?,
                _ = cx.cancel.cancelled() => return Ok(Outcome::Stopped),
            };
            drop(grant);
            searched += found.searched;
            cached += found.cached;
            evals.extend(found.evals);
            at = end;
        }
        let (summary, plies) = judge_game(input, &evals, version.id, spec.nodes)?;
        let review = store.put_review(id, summary, plies).await?;
        let hub = self.eval.hub();
        hub.bus.emit(Payload::ReviewUpserted(ReviewUpserted { game: id, accuracy: review.summary.accuracy() }));
        if let Some(summary) = store.game_summary(id).await? {
            hub.bus.emit(Payload::GameUpserted(Box::new(summary)));
        }
        Ok(Outcome::Reviewed { review: Box::new(review), searched, cached })
    }
}

fn accuracy_text(a: Option<f64>) -> String {
    a.map_or_else(|| "-".to_owned(), |a| format!("{a:.1}"))
}

#[async_trait]
impl Executor for Reviews {
    fn kind(&self) -> WorkKind {
        WorkKind::Review
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let spec: ReviewSpec = cx.spec()?;
        let mut state: ReviewCheckpoint = cx.checkpoint().unwrap_or_default();
        let total = spec.games.len();
        let version = self
            .eval
            .hub()
            .store
            .version(spec.version)
            .await?
            .ok_or_else(|| Error::not_found(format!("No version {}", spec.version)))?;
        let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
        if state.next == 0 {
            cx.log(format!(
                "Reviewing {} with {} at {} nodes a position, up to {}.",
                plural(total, "game"),
                version.name,
                spec.nodes,
                plural(spec.concurrency as usize, "engine")
            ));
        } else if state.next < total {
            cx.log(format!("Carrying on from game {} of {total}.", state.next + 1));
        }
        let started = Instant::now();
        let first = state.next;
        while state.next < total {
            let i = state.next;
            let id = spec.games[i];
            let began = Instant::now();
            let outcome = self.review_game(&cx, id, &version, &spec).await;
            match outcome {
                Ok(Outcome::Stopped) => {
                    cx.save_checkpoint(&state).await?;
                    return Err(cx.stopped());
                }
                Ok(Outcome::Reviewed { review, searched, cached }) => {
                    state.reviewed += 1;
                    state.searched += searched as u64;
                    state.cached += cached as u64;
                    cx.log(format!(
                        "{}/{total} game {id}: accuracy {} / {}, {searched} positions searched, {cached} cached, {:.1} s.",
                        i + 1,
                        accuracy_text(review.summary.white.accuracy),
                        accuracy_text(review.summary.black.accuracy),
                        began.elapsed().as_secs_f64()
                    ));
                    if let Some(drills) = &self.drills {
                        match drills.make_from_game(id).await {
                            Ok(made) if !made.is_empty() => {
                                cx.log(format!("{} new drills from game {id}.", made.len()))
                            }
                            Ok(_) => {}
                            Err(e) => cx.log(format!("No drills from game {id}: {}", e.message())),
                        }
                    }
                }
                Ok(Outcome::Skipped(why)) => {
                    state.skipped += 1;
                    cx.log(format!("{}/{total} game {id}: skipped ({why}).", i + 1));
                }
                Err(_) if cx.cancel.is_cancelled() => {
                    cx.save_checkpoint(&state).await?;
                    return Err(cx.stopped());
                }
                Err(e) => {
                    state.failed += 1;
                    cx.log(format!("{}/{total} game {id}: failed: {}", i + 1, e.message()));
                }
            }
            state.next = i + 1;
            cx.save_checkpoint(&state).await?;
            let done_now = (state.next - first) as u64;
            cx.progress_now(Progress {
                done: state.next as u64,
                total: Some(total as u64),
                unit: "games".into(),
                eta_seconds: Progress::eta(done_now, (total - first) as u64, started.elapsed().as_secs()),
                text: Some(format!("{} of {total} games, {} positions searched", state.next, state.searched)),
            })
            .await?;
        }
        cx.log(format!("Done: {} reviewed, {} skipped, {} failed.", state.reviewed, state.skipped, state.failed));
        if total > 0 && state.failed as usize == total {
            return Err(Error::internal("every game failed"));
        }
        let result = ReviewResult {
            reviewed: state.reviewed,
            skipped: state.skipped,
            failed: state.failed,
            positions_searched: state.searched,
            positions_cached: state.cached,
            version: version.id,
            nodes: spec.nodes,
        };
        serde_json::to_value(result).map_err(|e| Error::internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_read_with_defaults() {
        let req: ReviewRequest =
            serde_json::from_value(serde_json::json!({ "player": "you", "speeds": ["blitz"] })).unwrap();
        assert_eq!(req.player, Some(PlayerKey::You));
        assert_eq!(req.speeds, [Speed::Blitz]);
        assert!(req.games.is_empty() && req.version.is_none());
        let req: ReviewRequest =
            serde_json::from_value(serde_json::json!({ "games": [3, 4], "version": { "type": "id", "value": 2 } }))
                .unwrap();
        assert_eq!(req.games, [GameId(3), GameId(4)]);
        assert_eq!(req.version, Some(VersionRef::Id(VersionId(2))));
    }
}
