//! Reviews: the engine reading every move of finished games, as work.
//!
//! [`Reviews::prepare`] turns a request (some games, or a player's newest games not yet reviewed) into the work item the
//! scheduler queues, resolved when queued: the games, the version, the budget. [`Reviews`] is also the work's
//! [`Executor`]: it reads the games a few ahead, takes each one's positions from `core`'s [`to_search`], and has the
//! uncached ones searched in [runs](crate::runs) (each game's positions in the order played, on one engine that keeps
//! its hash table) on as many engines as the ledger grants, so the next games are already being searched while one
//! finishes and no engine waits for the end of a game. Each game, once its positions are in, is judged with `core`'s
//! [`judge_game`], stored and announced, in order. It checkpoints after each game, so a restart carries on with the
//! next one (positions searched for games after it are in the position cache by then); it waits at a safe point before
//! each batch, and within one each engine stops before its next position when the review must pause or give way, so it
//! pauses while you play on Lichess and yields to live play and to work of higher priority within one search.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use dameless_core::{
    Error, GameId, PlayerKey, Speed, VersionId,
    events::{AssistKind, Payload, ReviewUpserted},
    game::{Game, GameStatus},
    review::{self, PositionEval, Review, ReviewInput, judge_game, to_search},
    work::{Priority, Progress, Subject, Work, WorkKind},
};
use dameless_hub::{Executor, WorkContext};
use dameless_store::{GameFilter, NewWork, positions::SearchKey};
use dameless_work::Scheduler;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{Eval, Job, drills::Drills, runs::RunQueue, versions::VersionRef};

/// The smallest and largest node budgets a review takes.
pub const NODES_RANGE: std::ops::RangeInclusive<u64> = 1_000..=100_000_000;

/// The most games one review of a player takes.
pub const MAX_LIMIT: u32 = 1000;

/// The longest a batch of searches runs: between batches the review stores what it found, judges the games that are
/// complete, waits at a safe point and asks for cores again, so the ledger may grant more or fewer.
const BATCH: Duration = Duration::from_secs(8);

/// Positions kept queued per engine: games are read ahead until this many wait, enough to keep every engine busy for
/// a whole batch.
const LOOKAHEAD_PER_ENGINE: usize = 40;

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
    /// Engines at once, one thread each. When none: half the machine's cores; in the background, all of them (it takes
    /// only cores nothing else wants, and gives them back within one search when live play or other work needs them).
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
        let default = if req.background { cores } else { (cores / 2).max(1) };
        let concurrency = req.concurrency.unwrap_or(default).clamp(1, cores);
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

    /// How many of the player's games a background review (`background` with no `limit`, the default version and
    /// budget) would review now: the same query that picks its games, so the count and the work agree.
    pub async fn unreviewed_count(&self, player: &PlayerKey) -> Result<u32, Error> {
        let version = self.eval.binaries().resolve(&VersionRef::Champion).await?;
        Ok(self.unreviewed(player, &[], None, version.id, review::DEFAULT_NODES, u32::MAX).await?.len() as u32)
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

    /// Reads the game at `index` into the window: skipped when it is gone, live, empty or already reviewed with this
    /// version and budget; otherwise its positions, the cached ones filled in, the rest queued as one run in the order
    /// they were played, or, when a game before it in the window queued them already, waited for.
    async fn load(&self, window: &mut Window, index: usize, id: GameId, version: VersionId, search: &SearchKey) {
        let began = Instant::now();
        let state = match self.read(window, index, id, version, search).await {
            Ok(state) => state,
            Err(e) => Pend::Failed(e.message().to_owned()),
        };
        window.games.push_back(Pending { id, began, state });
    }

    async fn read(
        &self,
        window: &mut Window,
        index: usize,
        id: GameId,
        version: VersionId,
        search: &SearchKey,
    ) -> Result<Pend, Error> {
        let store = &self.eval.hub().store;
        let Some(game) = store.game(id).await? else { return Ok(Pend::Skipped("no such game")) };
        if matches!(game.summary.status, GameStatus::Live | GameStatus::Starting) {
            return Ok(Pend::Skipped("still being played"));
        }
        if game.moves.is_empty() {
            return Ok(Pend::Skipped("no moves"));
        }
        if store.has_review(id, version, search.nodes).await? {
            return Ok(Pend::Skipped("already reviewed"));
        }
        let positions = to_search(ReviewInput::from(&game))?;
        let mut keys = Vec::new();
        let mut seen = HashSet::new();
        for p in positions.iter().filter(|p| !p.terminal) {
            if seen.insert(p.key.as_str()) {
                keys.push(p.key.clone());
            }
        }
        let known = store.positions(keys, search).await?;
        let mut evals = vec![None; positions.len()];
        let (mut missing, mut searched, mut cached) = (0, 0, 0);
        let mut run = Vec::new();
        let mut counted = HashSet::new();
        for (k, p) in positions.iter().enumerate().filter(|(_, p)| !p.terminal) {
            let first_here = counted.insert(p.key.as_str());
            if let Some(eval) = known.get(&p.key) {
                evals[k] = Some(eval.clone());
                cached += usize::from(first_here);
                continue;
            }
            missing += 1;
            match window.waiting.get_mut(&p.key) {
                Some(waiters) => {
                    // Queued by a game before this one (cached, as far as this game goes), or earlier in this one.
                    if first_here && waiters.iter().all(|&(g, _)| g != index) {
                        cached += 1;
                    }
                    waiters.push((index, k));
                }
                None => {
                    window.waiting.insert(p.key.clone(), vec![(index, k)]);
                    run.push(Job { key: p.key.clone(), fen: p.fen.clone() });
                    searched += 1;
                }
            }
        }
        window.lock().push(run);
        Ok(Pend::Searching { game: Box::new(game), evals, missing, searched, cached })
    }

    /// Judges a game whose positions are all evaluated, stores its review and announces it.
    async fn judge(&self, game: &Game, evals: &[Option<PositionEval>], spec: &ReviewSpec) -> Result<Review, Error> {
        let (summary, plies) = judge_game(ReviewInput::from(game), evals, spec.version, spec.nodes)?;
        let hub = self.eval.hub();
        let review = hub.store.put_review(game.summary.id, summary, plies).await?;
        hub.bus.emit(Payload::ReviewUpserted(ReviewUpserted {
            game: game.summary.id,
            accuracy: review.summary.accuracy(),
        }));
        if let Some(summary) = hub.store.game_summary(game.summary.id).await? {
            hub.bus.emit(Payload::GameUpserted(Box::new(summary)));
        }
        Ok(review)
    }

    /// The game at the front of the window, done with: judged and stored, or counted as skipped or failed, then the
    /// checkpoint and progress moved past it.
    async fn finish(
        &self,
        cx: &WorkContext,
        done: Pending,
        spec: &ReviewSpec,
        run: &mut RunState,
    ) -> Result<(), Error> {
        let RunState { state, total, first, started } = run;
        let (i, total, id) = (state.next, *total, done.id);
        match done.state {
            Pend::Skipped(why) => {
                state.skipped += 1;
                cx.log(format!("{}/{total} game {id}: skipped ({why}).", i + 1));
            }
            Pend::Failed(why) => {
                state.failed += 1;
                cx.log(format!("{}/{total} game {id}: failed: {why}", i + 1));
            }
            Pend::Searching { game, evals, searched, cached, .. } => match self.judge(&game, &evals, spec).await {
                Ok(review) => {
                    state.reviewed += 1;
                    state.searched += searched as u64;
                    state.cached += cached as u64;
                    cx.log(format!(
                        "{}/{total} game {id}: accuracy {} / {}, {searched} positions searched, {cached} cached, {:.1} s.",
                        i + 1,
                        accuracy_text(review.summary.white.accuracy),
                        accuracy_text(review.summary.black.accuracy),
                        done.began.elapsed().as_secs_f64()
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
                Err(e) => {
                    state.failed += 1;
                    cx.log(format!("{}/{total} game {id}: failed: {}", i + 1, e.message()));
                }
            },
        }
        state.next = i + 1;
        cx.save_checkpoint(&*state).await?;
        let done_now = (state.next - *first) as u64;
        cx.progress_now(Progress {
            done: state.next as u64,
            total: Some(total as u64),
            unit: "games".into(),
            eta_seconds: Progress::eta(done_now, (total - *first) as u64, started.elapsed().as_secs()),
            text: Some(format!("{} of {total} games, {} positions searched", state.next, state.searched)),
        })
        .await
    }
}

fn accuracy_text(a: Option<f64>) -> String {
    a.map_or_else(|| "-".to_owned(), |a| format!("{a:.1}"))
}

/// A game of a review between being read and being judged.
struct Pending {
    id: GameId,
    began: Instant,
    state: Pend,
}

enum Pend {
    Skipped(&'static str),
    Failed(String),
    /// Waiting for `missing` of its positions.
    Searching {
        game: Box<Game>,
        evals: Vec<Option<PositionEval>>,
        missing: usize,
        searched: usize,
        cached: usize,
    },
}

/// The games a review has read and not yet judged, in order from the one at `first` (in the spec's list), the runs
/// of their positions still to search, and which game and position each queued position is for.
struct Window {
    first: usize,
    games: VecDeque<Pending>,
    waiting: HashMap<String, Vec<(usize, usize)>>,
    queue: Mutex<RunQueue<Job>>,
}

impl Window {
    fn new(first: usize) -> Self {
        Self { first, games: VecDeque::new(), waiting: HashMap::new(), queue: Mutex::new(RunQueue::new(0)) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RunQueue<Job>> {
        self.queue.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Positions queued and not yet searched.
    fn queued(&self) -> usize {
        self.lock().len()
    }

    /// Whether the game at the front has nothing left to wait for.
    fn front_done(&self) -> bool {
        self.games.front().is_some_and(|g| !matches!(g.state, Pend::Searching { missing, .. } if missing > 0))
    }

    fn pop_front(&mut self) -> Option<Pending> {
        let done = self.games.pop_front()?;
        self.first += 1;
        Some(done)
    }

    /// A searched position, for every game waiting for it: its evaluation, or the error that fails those games.
    fn deliver(&mut self, key: &str, result: Result<PositionEval, Error>) {
        for (index, k) in self.waiting.remove(key).unwrap_or_default() {
            let Some(pending) = index.checked_sub(self.first).and_then(|i| self.games.get_mut(i)) else { continue };
            match (&mut pending.state, &result) {
                (Pend::Searching { evals, missing, .. }, Ok(eval)) => {
                    evals[k] = Some(eval.clone());
                    *missing -= 1;
                }
                (Pend::Searching { .. }, Err(e)) => pending.state = Pend::Failed(e.message().to_owned()),
                _ => {}
            }
        }
    }
}

/// Where a running review stands, for [`Reviews::finish`].
struct RunState {
    state: ReviewCheckpoint,
    total: usize,
    first: usize,
    started: Instant,
}

#[async_trait]
impl Executor for Reviews {
    fn kind(&self) -> WorkKind {
        WorkKind::Review
    }

    /// Reads games a few ahead into a window and searches their positions in runs on as many engines as the ledger
    /// grants, in batches of at most [`BATCH`]: before each batch it waits at a safe point (paused, or while you play
    /// on Lichess) and asks for cores; within one, each engine stops before its next position as soon as the review is
    /// stopped, paused, refused by the policy, or work of higher priority waits for cores, so it yields within one
    /// search. Games are judged and stored in order as their positions come in, with a checkpoint after each.
    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let spec: ReviewSpec = cx.spec()?;
        let state: ReviewCheckpoint = cx.checkpoint().unwrap_or_default();
        let total = spec.games.len();
        let hub = self.eval.hub().clone();
        let version = hub
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
        let search = SearchKey { commit: version.commit.clone(), nodes: spec.nodes, multipv: spec.multipv };
        let lookahead = spec.concurrency as usize * LOOKAHEAD_PER_ENGINE;
        let mut run = RunState { first: state.next, state, total, started: Instant::now() };
        let mut window = Window::new(run.state.next);
        let mut next_load = run.state.next;
        let mut engines = None;
        loop {
            while window.front_done() {
                let done = window.pop_front().expect("a game at the front");
                self.finish(&cx, done, &spec, &mut run).await?;
            }
            if window.games.is_empty() && next_load >= total {
                break;
            }
            while next_load < total && window.queued() < lookahead {
                if cx.cancel.is_cancelled() {
                    cx.save_checkpoint(&run.state).await?;
                    return Err(cx.stopped());
                }
                self.load(&mut window, next_load, spec.games[next_load], spec.version, &search).await;
                next_load += 1;
            }
            if window.front_done() {
                continue;
            }
            if window.queued() == 0 {
                return Err(Error::internal("the review waits for positions it has not queued"));
            }
            // The binary first, holding no cores: when it must be built, the build is work that needs them.
            let engines = match &engines {
                Some(e) => e,
                None => engines.insert(self.eval.review_engines(&version, spec.nodes, spec.multipv).await?),
            };
            if !cx.safe_point(Some(AssistKind::Review)).await {
                cx.save_checkpoint(&run.state).await?;
                return Err(cx.stopped());
            }
            let Some(grant) = cx.cores(1, spec.concurrency).await else {
                cx.save_checkpoint(&run.state).await?;
                return Err(cx.stopped());
            };
            let until = Instant::now() + BATCH;
            let go_on = || {
                !cx.cancel.is_cancelled()
                    && !cx.pause_asked()
                    && hub.policy.allows(AssistKind::Review)
                    && !hub.ledger.waiting_ahead_of(cx.priority)
                    && Instant::now() < until
            };
            let found = Mutex::new(Vec::new());
            let stopped = tokio::select! {
                _ = self.eval.search_runs(
                    engines, &window.queue, grant.cores() as usize, &go_on, &found,
                ) => false,
                _ = cx.cancel.cancelled() => true,
            };
            drop(grant);
            window.lock().give_back_all();
            let found = found.into_inner().unwrap_or_else(|p| p.into_inner());
            let fresh: Vec<(String, PositionEval)> =
                found.iter().filter_map(|(job, r)| r.as_ref().ok().map(|e| (job.key.clone(), e.clone()))).collect();
            if !fresh.is_empty() {
                hub.store.put_positions(fresh, &search).await?;
            }
            for (job, result) in found {
                window.deliver(&job.key, result);
            }
            if stopped {
                cx.save_checkpoint(&run.state).await?;
                return Err(cx.stopped());
            }
        }
        let state = run.state;
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
