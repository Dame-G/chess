//! Check the harness: a ready-made custom idea that plays the champion against a functionally identical branch, so a
//! machine whose testing does not treat identical engines as identical is caught before any result on it is trusted.
//! The rules and verdicts are in `core::harness`; this runs them.
//!
//! The identical branch, `harness/identical`, is made by the app: one commit on top of `main`'s tip that changes only
//! a comment, with `main`'s `Bench:` line (the bench is the same, as nothing that plays changed). It is written with
//! git's plumbing (`engine::git::commit_files`), so the user's checkout is never touched, and made again from the
//! current tip for every check.
//!
//! The idea has two trials of fixed pairs: the determinism part at 25,000 nodes (1,000 pairs by default), then the
//! symmetry part at VSTC (4,000 pairs). When the determinism part fails, the symmetry part is skipped: the check has
//! failed, and hours of timed games would not change that. When the check ends it is judged, the idea ends kept
//! (passed) or dropped (failed) with the check's sentence as its verdict, and the result is stored in `meta` as
//! `harness.check` with the machine and tools it certifies.

use std::{path::Path, sync::Arc};

use dameless_core::{
    Error, IdeaId,
    game::Outcome,
    harness::{self, Environment, HarnessCheck, HarnessStatus, PlayedGame},
    ideas::{self as funnel, Author, Idea, IdeaKind, IdeaStatus, Trial, TrialStatus},
    time::TimeControl,
};
use dameless_engine::git::CommitFiles;
use dameless_store::{GameFilter, IdeaFilter, IdeaUpdate, now};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    health::{Probes, SystemProbes},
    ideas::{CustomTrial, IdeaSpec, Ideas, NewIdeaRequest},
    toolchain::require_tools,
    versions::MAIN,
};

/// Where the last check is kept.
pub const META_KEY: &str = "harness.check";

/// `POST /api/harness/check`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HarnessCheckRequest {
    /// Pairs of the determinism part, at 25,000 nodes (default 1,000).
    #[serde(default)]
    pub determinism_pairs: Option<u32>,
    /// Pairs of the symmetry part, at VSTC (default 4,000).
    #[serde(default)]
    pub symmetry_pairs: Option<u32>,
}

/// What a harness check idea carries in its spec: what it certifies, and how long each part is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessRun {
    pub environment: Environment,
    /// `main`'s tip it was made from.
    pub engine_commit: String,
    /// Pairs of the determinism part (its first trial). Zero for a check queued before there was one (an SPRT), which
    /// cannot be judged by today's rules.
    #[serde(default)]
    pub determinism_pairs: u32,
    /// Pairs of the symmetry part (its second trial).
    #[serde(default)]
    pub symmetry_pairs: u32,
}

/// The harness check. Cheap to clone.
#[derive(Clone)]
pub struct Harness {
    ideas: Ideas,
    probes: Arc<dyn Probes>,
}

/// A tool's identity from its installed file: tools are installed under pinned names ("fastchess-1.8.2"), so the name
/// is the version.
pub fn tool_name(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.strip_suffix(".exe").map(str::to_owned).unwrap_or(name)
}

/// The comment a check's identical commit adds.
const COMMENT: &str = "Harness check: identical to main; only this comment differs.";

impl Harness {
    /// Over the real machine.
    pub fn new(ideas: Ideas) -> Self {
        Self::with_probes(ideas, Arc::new(SystemProbes::default()))
    }

    /// Over other probes (a fake machine, in tests).
    pub fn with_probes(ideas: Ideas, probes: Arc<dyn Probes>) -> Self {
        Self { ideas, probes }
    }

    pub fn ideas(&self) -> &Ideas {
        &self.ideas
    }

    pub fn probes(&self) -> &Arc<dyn Probes> {
        &self.probes
    }

    /// The machine and the tools installed now: what a check made now would certify.
    pub fn environment(&self) -> Environment {
        let versions = self.ideas.versions();
        let tools = versions.toolchain().tools(&versions.hub().config.tools_dir());
        Environment {
            machine: self.probes.machine(),
            fastchess: tools.fastchess.as_deref().map(tool_name),
            book: tools.book.as_deref().map(tool_name),
        }
    }

    /// The harness check idea queued or running now, if any.
    async fn running(&self) -> Result<Option<Idea>, Error> {
        let filter = IdeaFilter {
            statuses: vec![IdeaStatus::Queued, IdeaStatus::Running],
            tag: Some(harness::TAG.to_owned()),
            limit: 1,
        };
        Ok(self.ideas.list(filter).await?.into_iter().next())
    }

    /// Whether results on this machine can be trusted: the last check, and whether it still applies here.
    pub async fn status(&self) -> Result<HarnessStatus, Error> {
        let last = self.ideas.hub().store.meta::<HarnessCheck>(META_KEY).await?;
        let running = self.running().await?.map(|i| i.id);
        Ok(harness::status(last, &self.environment(), running))
    }

    /// Starts a check: makes `harness/identical` again from `main`'s tip and queues the idea. 409 while one runs or
    /// without fastchess and the book; 400 for a part of no pairs.
    pub async fn start_check(&self, req: HarnessCheckRequest) -> Result<Idea, Error> {
        let determinism_pairs = req.determinism_pairs.unwrap_or(harness::DETERMINISM_PAIRS);
        let symmetry_pairs = req.symmetry_pairs.unwrap_or(harness::SYMMETRY_PAIRS);
        if determinism_pairs == 0 || symmetry_pairs == 0 {
            return Err(Error::bad_request("Each part of the check needs at least one pair"));
        }
        if let Some(i) = self.running().await? {
            return Err(Error::conflict(format!("The harness is being checked already (idea {})", i.id)));
        }
        let versions = self.ideas.versions();
        require_tools(&versions.toolchain().tools(&versions.hub().config.tools_dir())).map_err(Error::conflict)?;
        let environment = self.environment();
        let engine_commit = self.make_identical().await?;

        let mut plan = funnel::custom(harness::DETERMINISM_PRESET, None, Some(determinism_pairs));
        plan.extend(funnel::custom(harness::SYMMETRY_PRESET, None, Some(symmetry_pairs)));
        let custom = CustomTrial { preset: harness::DETERMINISM_PRESET, bounds: None, pairs: Some(determinism_pairs) };
        let request = NewIdeaRequest {
            title: "Check the harness".into(),
            hypothesis: format!(
                "An engine identical to the champion plays the same game twice from every opening at fixed nodes \
                 ({determinism_pairs} pairs), and over {symmetry_pairs} timed pairs scores 0 Elo with symmetric \
                 pairs: testing on this machine treats identical engines as identical."
            ),
            kind: IdeaKind::Gain,
            branch: harness::BRANCH.into(),
            base: None,
            author: Author::You,
            tags: vec![harness::TAG.into()],
            notes: None,
            long: false,
            tune: None,
            train: None,
            custom: Some(custom),
            fix_bench: false,
        };
        let spec = IdeaSpec {
            train: None,
            funnel: plan,
            long: false,
            tune: None,
            custom: Some(custom),
            harness: Some(HarnessRun { environment, engine_commit, determinism_pairs, symmetry_pairs }),
        };
        self.ideas.insert_spec(request, spec).await
    }

    /// Makes `harness/identical` again: `main`'s tip plus a comment, with its `Bench:` line. Returns `main`'s sha.
    pub async fn make_identical(&self) -> Result<String, Error> {
        let repo = self.ideas.versions().repo();
        let git = |e: anyhow::Error| Error::conflict(format!("git: {e:#}"));
        let main = repo.resolve(MAIN).await.map_err(|_| Error::conflict("The engine checkout has no branch main"))?;
        if repo.has_branch(harness::BRANCH).await {
            repo.delete_branch(harness::BRANCH).await.map_err(git)?;
        }
        // A comment in the crate root when there is one (Reckless's src/main.rs), else a line in the README.
        let mut change = None;
        for path in ["src/main.rs", "src/lib.rs"] {
            if let Ok(text) = repo.read_file(&main.sha, path).await {
                change = Some((path, format!("{text}\n\n// {COMMENT}\n")));
                break;
            }
        }
        let (path, contents) = match change {
            Some(c) => c,
            None => {
                let text = repo.read_file(&main.sha, "README.md").await.unwrap_or_default();
                ("README.md", format!("{text}\n\n<!-- {COMMENT} -->\n"))
            }
        };
        let short = main.sha.get(..8).unwrap_or(&main.sha);
        let bench = main.bench.map(|b| format!("\n\nBench: {b}")).unwrap_or_default();
        let message = format!(
            "Harness check: identical to main ({short})\n\nOnly a comment changes, so this plays exactly as main does. \
             Made by Dameless for Check the harness, and made again for every check.{bench}\n"
        );
        repo.commit_files(&CommitFiles {
            branch: harness::BRANCH,
            from: &main.sha,
            files: &[(path, contents.as_bytes())],
            message: &message,
        })
        .await
        .map_err(git)?;
        Ok(main.sha)
    }
}

/// The determinism part's verdict, from its trial and the games it stored.
async fn determinism(ideas: &Ideas, trial: &Trial, run: &HarnessRun) -> Result<harness::Determinism, Error> {
    let store = &ideas.hub().store;
    let mut summaries = Vec::new();
    loop {
        let filter = GameFilter {
            trial: Some(trial.id),
            before: summaries.last().map(|s: &dameless_core::game::GameSummary| s.id),
            limit: dameless_store::games::MAX_LIMIT,
            ..Default::default()
        };
        let page = store.games(filter).await?;
        if page.is_empty() {
            break;
        }
        summaries.extend(page);
    }
    let mut games = Vec::with_capacity(summaries.len());
    for s in summaries {
        let Some(g) = store.game(s.id).await? else { continue };
        let result = match g.summary.outcome {
            Some(Outcome::WhiteWins) => "1-0",
            Some(Outcome::BlackWins) => "0-1",
            Some(Outcome::Draw) => "1/2-1/2",
            None => "*",
        };
        games.push(PlayedGame {
            opening: g.start_fen.unwrap_or_else(|| "startpos".into()),
            moves: g.moves.into_iter().map(|m| m.uci).collect(),
            result: result.into(),
        });
    }
    let nodes = match trial.played.or(trial.time) {
        Some(TimeControl::Nodes { nodes }) => nodes,
        _ => 0,
    };
    Ok(harness::determinism(&trial.penta, &games, u64::from(run.determinism_pairs), nodes))
}

/// Whether a harness check is over after trial `index`: after the determinism part when it failed (the symmetry part
/// would not change the verdict), and after its last trial.
pub(crate) async fn over_after(ideas: &Ideas, id: IdeaId, index: usize, run: &HarnessRun) -> Result<bool, Error> {
    if index > 0 || run.determinism_pairs == 0 {
        return Ok(false);
    }
    let trials = ideas.hub().store.trials(id).await?;
    let Some(first) = trials.first() else { return Ok(false) };
    Ok(first.status != TrialStatus::Passed || !determinism(ideas, first, run).await?.passed)
}

/// Ends a harness check idea once it is over: judges it, stores the result for the machine, and ends the idea kept
/// (passed) or dropped (failed). A part that broke off (its pairs not all played) stores nothing and fails the idea.
pub(crate) async fn finish(ideas: &Ideas, id: IdeaId, run: &HarnessRun) -> Result<(), Error> {
    let store = &ideas.hub().store;
    let trials = store.trials(id).await?;
    let fail = |verdict: String| IdeaUpdate {
        status: Some(IdeaStatus::Failed),
        verdict: Some(verdict),
        ended_at: Some(now()),
        ..Default::default()
    };
    let first = trials.first().ok_or_else(|| Error::internal(format!("Harness check {id} has no trial")))?;
    let update = if run.determinism_pairs == 0 {
        fail("Failed: this check was queued by an older Dameless; start a new one".into())
    } else if first.status != TrialStatus::Passed {
        fail(format!("Failed: the determinism part did not finish ({})", first.detail.clone().unwrap_or_default()))
    } else {
        let det = determinism(ideas, first, run).await?;
        let symmetry_run = trials
            .get(1)
            .filter(|t| det.passed && t.status == TrialStatus::Passed)
            .map(|t| (&t.penta, u64::from(run.symmetry_pairs)));
        if det.passed && symmetry_run.is_none() {
            fail("Failed: the symmetry part did not finish".into())
        } else {
            for t in trials.iter().skip(1).filter(|t| t.status == TrialStatus::Waiting) {
                ideas.save_trial(&Trial { status: TrialStatus::Skipped, ..t.clone() }).await?;
            }
            let (passed, elo, symmetry, summary) = harness::judge(&det, symmetry_run);
            let check = HarnessCheck {
                machine: run.environment.machine.clone(),
                at: now(),
                idea: id,
                passed,
                determinism: Some(det),
                elo,
                symmetry,
                engine_commit: run.engine_commit.clone(),
                fastchess: run.environment.fastchess.clone(),
                book: run.environment.book.clone(),
                summary: summary.clone(),
            };
            store.put_meta(META_KEY, &check).await?;
            let (status, verdict) = if passed {
                (IdeaStatus::Kept, format!("Harness checked: {summary}"))
            } else {
                (IdeaStatus::Dropped, format!("Harness check failed: {summary}"))
            };
            IdeaUpdate { status: Some(status), verdict: Some(verdict), ended_at: Some(now()), ..Default::default() }
        }
    };
    store.update_idea(id, update).await?;
    ideas.emit_idea(id).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
