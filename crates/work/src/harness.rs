//! Check the harness: a ready-made custom idea that plays the champion against a functionally identical branch, so a
//! machine whose testing would accept a change that does nothing is caught before any result on it is trusted. The
//! rules and verdicts are in `core::harness`; this runs them.
//!
//! The identical branch, `harness/identical`, is made by the app: one commit on top of `main`'s tip that changes only
//! a comment, with `main`'s `Bench:` line (the bench is the same, as nothing that plays changed). It is written with
//! git's plumbing (`engine::git::commit_files`), so the user's checkout is never touched, and made again from the
//! current tip for every check.
//!
//! The idea's first trial is VSTC at [0, 3]; with `pairs`, a second trial plays that many pairs for the symmetry
//! check. Both always run. When the last ends, the check is judged, the idea ends kept (passed) or dropped (failed)
//! with the check's sentence as its verdict, and the result is stored in `meta` as `harness.check` with the machine
//! and tools it certifies.

use std::{path::Path, sync::Arc};

use dameless_core::{
    Error, IdeaId,
    harness::{self, Environment, HarnessCheck, HarnessStatus},
    ideas::{self as funnel, Author, Idea, IdeaKind, IdeaStatus, TrialStatus},
    stats::Verdict,
};
use dameless_engine::git::CommitFiles;
use dameless_store::{IdeaFilter, IdeaUpdate, now};
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
    /// Also play this many pairs for the symmetry check (the UI offers 2,000); none skips it.
    #[serde(default)]
    pub pairs: Option<u32>,
}

/// What a harness check idea carries in its spec: what it certifies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessRun {
    pub environment: Environment,
    /// `main`'s tip it was made from.
    pub engine_commit: String,
    /// Whether its second trial is the symmetry run.
    pub symmetry: bool,
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
    /// without fastchess and the book.
    pub async fn start_check(&self, req: HarnessCheckRequest) -> Result<Idea, Error> {
        if req.pairs == Some(0) {
            return Err(Error::bad_request("A symmetry run needs at least one pair"));
        }
        if let Some(i) = self.running().await? {
            return Err(Error::conflict(format!("The harness is being checked already (idea {})", i.id)));
        }
        let versions = self.ideas.versions();
        require_tools(&versions.toolchain().tools(&versions.hub().config.tools_dir())).map_err(Error::conflict)?;
        let environment = self.environment();
        let engine_commit = self.make_identical().await?;

        let mut plan = funnel::custom(harness::PRESET, Some(harness::BOUNDS), None);
        if let Some(p) = req.pairs {
            plan.extend(funnel::custom(harness::PRESET, None, Some(p)));
        }
        let custom = CustomTrial { preset: harness::PRESET, bounds: Some(harness::BOUNDS), pairs: None };
        let request = NewIdeaRequest {
            title: "Check the harness".into(),
            hypothesis: format!(
                "An engine identical to the champion is rejected at [{}, {}]: testing on this machine does not \
                 accept a change that does nothing.",
                harness::BOUNDS[0],
                harness::BOUNDS[1]
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
            harness: Some(HarnessRun { environment, engine_commit, symmetry: req.pairs.is_some() }),
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

/// Ends a harness check idea once its last trial is over: judges it, stores the result for the machine, and ends the
/// idea kept (passed) or dropped (failed). An SPRT that did not decide (it broke off) stores nothing and fails the idea.
pub(crate) async fn finish(ideas: &Ideas, id: IdeaId, run: &HarnessRun) -> Result<(), Error> {
    let store = &ideas.hub().store;
    let trials = store.trials(id).await?;
    let sprt_trial = trials.first().ok_or_else(|| Error::internal(format!("Harness check {id} has no trial")))?;
    let bounds = sprt_trial.bounds.unwrap_or(harness::BOUNDS);
    let s = funnel::sprt(bounds);
    let decided = s.verdict(s.llr(&sprt_trial.penta)) != Verdict::Continue;
    let symmetry =
        trials.get(1).filter(|t| run.symmetry && t.status == TrialStatus::Passed).map(|t| harness::symmetry(&t.penta));
    let (passed, elo, summary) = harness::judge(&sprt_trial.penta, bounds, symmetry.as_ref());
    let (status, verdict) = if !decided {
        (IdeaStatus::Failed, format!("Failed: {summary}"))
    } else {
        let check = HarnessCheck {
            machine: run.environment.machine.clone(),
            at: now(),
            idea: id,
            passed,
            elo,
            symmetry,
            engine_commit: run.engine_commit.clone(),
            fastchess: run.environment.fastchess.clone(),
            book: run.environment.book.clone(),
            summary: summary.clone(),
        };
        store.put_meta(META_KEY, &check).await?;
        if passed {
            (IdeaStatus::Kept, format!("Harness checked: {summary}"))
        } else {
            (IdeaStatus::Dropped, format!("Harness check failed: {summary}"))
        }
    };
    store
        .update_idea(
            id,
            IdeaUpdate { status: Some(status), verdict: Some(verdict), ended_at: Some(now()), ..Default::default() },
        )
        .await?;
    ideas.emit_idea(id).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
