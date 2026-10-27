//! Ideas: a hypothesis and a change, run through the funnel of its kind (see `core::ideas`), one trial at a time.
//!
//! Creating an idea stores it with every trial of its funnel (waiting) and queues the first as work: a `Trial` item for
//! fingerprint, speed and game stages, a `Tune` item for SPSA. When an item ends, the scheduler calls back here and
//! `core::ideas::next` decides: queue the next trial, or finish the idea as kept or dropped with its one-sentence
//! verdict. A trial that breaks (a build fails, an engine crashes) fails the idea; a stopped one stops it, and
//! [`Ideas::resume`] carries on from the trial that did not finish, keeping the rounds it played.
//!
//! A kept idea can be promoted: merged into the fork's `main`, which makes the next champion ([`promote`]).

use std::sync::Arc;

use async_trait::async_trait;
use dameless_core::{
    Error, IdeaId, TrialId,
    events::Payload,
    ideas::{
        self as funnel, Author, Idea, IdeaKind, IdeaStatus, Next, Preset, Stage, StagePlan, StageRule, Trial,
        TrialStatus,
    },
    spsa::{TuneSpec, TuneStep},
    training::TrainSpec,
    version::Estimate,
    work::{Priority, Subject, Work, WorkKind, WorkStatus},
};
use dameless_hub::Hub;
use dameless_store::{IdeaFilter, IdeaUpdate, NewIdea, NewTrial, NewWork, now};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    scheduler::{EndHook, Scheduler},
    versions::{VersionRef, Versions},
};

mod bench;
pub mod promote;
mod trial;
mod tune;

pub use bench::{BenchLineStatus, BranchCheck, FixBenchRequest};
pub use promote::PromoteWork;

/// `POST /api/ideas`: an idea to try.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct NewIdeaRequest {
    /// "Deeper LMR at high depth".
    pub title: String,
    #[serde(default)]
    pub hypothesis: String,
    #[serde(default = "gain")]
    pub kind: IdeaKind,
    /// The change: a branch of the fork (for a tune, the branch with the tunables).
    pub branch: String,
    /// What it is tested against: a version (`v14`, a branch, `#12`); the champion when left out.
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub author: Author,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// Add the long-games stage to the funnel.
    #[serde(default)]
    pub long: bool,
    /// A tune's settings (a tune idea without them tunes with OpenBench's defaults).
    #[serde(default)]
    pub tune: Option<TuneSpec>,
    /// A net idea's training run (a fine-tune of its base's net with the default schedule when left out). A net idea
    /// needs no branch: it trains for the champion's code (`branch` may be left empty).
    #[serde(default)]
    pub train: Option<TrainSpec>,
    /// One trial with any preset and bounds or a fixed number of pairs, instead of the funnel.
    #[serde(default)]
    pub custom: Option<CustomTrial>,
    /// When the branch's tip has no `Bench:` line or a stale one, build it and reword the tip with the measured bench
    /// before creating the idea (this changes the fork's branch locally). Without it, such a branch is refused with a
    /// 409, since the fingerprint stage would fail on it.
    #[serde(default)]
    pub fix_bench: bool,
}

fn gain() -> IdeaKind {
    IdeaKind::Gain
}

/// A custom idea's single trial (what `chess test` was).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct CustomTrial {
    pub preset: Preset,
    /// SPRT bounds in normalized Elo; with neither these nor `pairs`, [0, 3].
    #[serde(default)]
    pub bounds: Option<[f64; 2]>,
    /// A fixed number of game pairs instead of an SPRT.
    #[serde(default)]
    pub pairs: Option<u32>,
}

/// `POST /api/ideas/batch`: several ideas under one tag.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct BatchRequest {
    pub ideas: Vec<NewIdeaRequest>,
    pub tag: String,
}

/// What the store keeps of an idea for running it: its funnel and settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IdeaSpec {
    pub funnel: Vec<StagePlan>,
    #[serde(default)]
    pub long: bool,
    #[serde(default)]
    pub tune: Option<TuneSpec>,
    /// A net idea's training run.
    #[serde(default)]
    pub train: Option<TrainSpec>,
    #[serde(default)]
    pub custom: Option<CustomTrial>,
    /// Set on a harness check: what it is checking, and that every trial runs whatever the one before showed.
    #[serde(default)]
    pub harness: Option<crate::harness::HarnessRun>,
}

/// What a `Trial` or `Tune` work item runs: one trial of an idea.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialWork {
    pub idea: IdeaId,
    pub trial: TrialId,
    /// Its index in the idea's funnel.
    pub index: usize,
}

/// `GET /api/ideas/{id}/tune`: a tune's settings and its SPSA history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TuneView {
    pub spec: TuneSpec,
    /// The parameters' names, in the order of each step's values.
    pub params: Vec<String>,
    pub steps: Vec<TuneStep>,
}

/// Ideas and their trials. Cheap to clone.
#[derive(Clone)]
pub struct Ideas {
    inner: Arc<Inner>,
}

struct Inner {
    versions: Versions,
}

impl Ideas {
    pub fn new(versions: Versions) -> Self {
        Self { inner: Arc::new(Inner { versions }) }
    }

    /// Registers the `Trial`, `Tune` and `Promote` executors with the scheduler, and the callback that moves ideas along
    /// their funnel.
    pub fn register(&self) {
        let s = self.scheduler();
        s.register(Arc::new(trial::TrialExecutor { ideas: self.clone() }));
        s.register(Arc::new(tune::TuneExecutor { ideas: self.clone() }));
        s.register(Arc::new(promote::PromoteExecutor { ideas: self.clone() }));
        let hook: Arc<dyn EndHook> = Arc::new(Funnel { ideas: self.clone() });
        s.on_end(WorkKind::Trial, hook.clone());
        s.on_end(WorkKind::Tune, hook.clone());
        s.on_end(WorkKind::Train, hook);
    }

    pub fn versions(&self) -> &Versions {
        &self.inner.versions
    }

    fn scheduler(&self) -> &Scheduler {
        self.inner.versions.scheduler()
    }

    pub(crate) fn hub(&self) -> &Arc<Hub> {
        self.inner.versions.hub()
    }

    /// Checks a request without storing anything: a batch is checked whole before any of it is queued.
    async fn check(&self, req: &NewIdeaRequest) -> Result<(), Error> {
        if req.title.trim().is_empty() {
            return Err(Error::bad_request("An idea needs a title"));
        }
        if req.kind == IdeaKind::Net && req.custom.is_none() {
            let train = req.train.clone().unwrap_or_default();
            if let Some(p) = train.schedule.problem() {
                return Err(Error::bad_request(p));
            }
            if train.from_scratch && train.parent.is_some() {
                return Err(Error::bad_request("A run from scratch has no parent net"));
            }
            if self.hub().ledger.gpus() == 0 {
                return Err(Error::conflict("A net idea needs an NVIDIA GPU, and none was found (DAMELESS_GPUS)"));
            }
            return Ok(());
        }
        let branch = req.branch.trim();
        if branch.is_empty() {
            return Err(Error::bad_request("An idea needs a branch"));
        }
        if branch == crate::versions::MAIN {
            return Err(Error::bad_request("An idea's branch cannot be main, the champion"));
        }
        if self.versions().repo().resolve(branch).await.is_err() {
            return Err(Error::bad_request(format!("No branch {branch} in the engine checkout")));
        }
        if let Some(c) = &req.custom
            && let Some(b) = c.bounds
            && b[0] >= b[1]
        {
            return Err(Error::bad_request("SPRT bounds need lower < upper"));
        }
        if let Some(t) = &req.tune
            && (t.iterations == 0 || t.pairs == 0)
        {
            return Err(Error::bad_request("A tune needs at least one iteration of at least one pair"));
        }
        if let Some(check) = self.bench_problem(req).await?
            && !req.fix_bench
        {
            return Err(Error::conflict(format!(
                "{}, so its fingerprint stage would fail. Let the app fix the line (Fix the Bench: line, or --fix-bench), or amend the commit",
                check.detail
            )));
        }
        Ok(())
    }

    /// The branch's `Bench:` problem, for an idea whose funnel fingerprints its branch (a tune's branch is written by
    /// SPSA, and a custom test has no fingerprint stage).
    async fn bench_problem(&self, req: &NewIdeaRequest) -> Result<Option<BranchCheck>, Error> {
        if req.custom.is_some() || matches!(req.kind, IdeaKind::Tune | IdeaKind::Net) {
            return Ok(None);
        }
        let check = self.check_branch(&req.branch).await?;
        Ok(check.blocks().then_some(check))
    }

    /// Fixes the branch's `Bench:` line first when the request asks for it and the line needs it.
    async fn fix_if_asked(&self, req: &NewIdeaRequest) -> Result<(), Error> {
        if req.fix_bench
            && let Some(check) = self.bench_problem(req).await?
        {
            self.fix_bench_line(&req.branch, Some(&check.commit)).await?;
        }
        Ok(())
    }

    /// Stores an idea with its trials and queues its first trial.
    pub async fn create(&self, req: NewIdeaRequest) -> Result<Idea, Error> {
        self.check(&req).await?;
        self.fix_if_asked(&req).await?;
        self.insert(req).await
    }

    /// Several ideas under one tag (a batch). All are checked before any is queued; they run one after another.
    pub async fn batch(&self, req: BatchRequest) -> Result<Vec<Idea>, Error> {
        for i in &req.ideas {
            self.check(i).await?;
        }
        let mut out = Vec::new();
        for mut i in req.ideas {
            if !req.tag.trim().is_empty() && !i.tags.contains(&req.tag) {
                i.tags.push(req.tag.clone());
            }
            self.fix_if_asked(&i).await?;
            out.push(self.insert(i).await?);
        }
        Ok(out)
    }

    async fn insert(&self, req: NewIdeaRequest) -> Result<Idea, Error> {
        let tune = (req.kind == IdeaKind::Tune && req.custom.is_none()).then(|| req.tune.clone().unwrap_or_default());
        let plan = match req.custom {
            Some(c) => funnel::custom(c.preset, c.bounds, c.pairs),
            None => funnel::funnel(req.kind, req.long),
        };
        let train = (req.kind == IdeaKind::Net && req.custom.is_none()).then(|| req.train.clone().unwrap_or_default());
        let spec = IdeaSpec { funnel: plan, long: req.long, tune, train, custom: req.custom, harness: None };
        self.insert_spec(req, spec).await
    }

    /// Stores an idea run by `spec` (its funnel and settings) with its trials, and queues its first trial.
    pub(crate) async fn insert_spec(&self, req: NewIdeaRequest, spec: IdeaSpec) -> Result<Idea, Error> {
        let store = &self.hub().store;
        let base =
            self.versions().resolve(&req.base.as_deref().map_or(VersionRef::Champion, VersionRef::parse)).await?;
        let plan = spec.funnel.clone();
        let mut branch = req.branch.trim().to_owned();
        if spec.train.is_some() && branch.is_empty() {
            // A net idea trains for its base's code.
            branch = base.branch.clone().unwrap_or_else(|| crate::versions::MAIN.to_owned());
        }
        let idea = store
            .insert_idea(NewIdea {
                title: req.title.trim().to_owned(),
                hypothesis: req.hypothesis,
                kind: req.kind,
                author: req.author,
                branch: branch.clone(),
                base: base.id,
                tags: req.tags,
                notes: req.notes,
                spec: serde_json::to_value(&spec).map_err(|e| Error::internal(e.to_string()))?,
            })
            .await?;
        for stage in &plan {
            let (preset, bounds, pairs) = match stage.rule {
                StageRule::Games { preset, bounds, pairs } => (Some(preset), bounds, pairs),
                _ => (None, None, None),
            };
            store
                .insert_trial(
                    idea.id,
                    NewTrial { stage: stage.stage, preset, time: preset.map(|p| p.settings().time), bounds, pairs },
                )
                .await?;
        }
        // A tune's version is the branch SPSA writes, a net idea's the base's code with the net it trains; every other
        // idea's is its branch.
        if spec.tune.is_none() && spec.train.is_none() {
            let version = self.versions().idea_version(&branch, idea.id, Some(base.id)).await?;
            store.update_idea(idea.id, IdeaUpdate { version: Some(version.id), ..Default::default() }).await?;
        }
        self.queue_trial(idea.id, 0).await?;
        self.emit_idea(idea.id).await
    }

    /// Queues the trial at `index` of an idea's funnel as work.
    async fn queue_trial(&self, idea: IdeaId, index: usize) -> Result<Work, Error> {
        let store = &self.hub().store;
        let i = store.idea(idea).await?.ok_or_else(|| Error::not_found(format!("No idea {idea}")))?;
        let trial = i.trials.get(index).ok_or_else(|| Error::internal(format!("Idea {idea} has no trial {index}")))?;
        let (kind, title, spec) = match trial.stage {
            Stage::Train => {
                let train = self.spec(idea).await?.train.unwrap_or_default();
                let work =
                    crate::train::TrainWork { train, idea: Some(idea), trial: Some(trial.id), index: Some(index) };
                (WorkKind::Train, format!("Training a net for {}", i.title), serde_json::to_value(work))
            }
            stage => (
                if stage == Stage::Tune { WorkKind::Tune } else { WorkKind::Trial },
                format!("Trying {} · {}", i.branch, stage.label()),
                serde_json::to_value(TrialWork { idea, trial: trial.id, index }),
            ),
        };
        self.scheduler()
            .submit(NewWork {
                kind,
                priority: Priority::Idea,
                title,
                subject: Subject::Trial { id: trial.id },
                spec: spec.map_err(|e| Error::internal(e.to_string()))?,
            })
            .await
    }

    pub async fn get(&self, id: IdeaId) -> Result<Idea, Error> {
        self.hub().store.idea(id).await?.ok_or_else(|| Error::not_found(format!("No idea {id}")))
    }

    /// Ideas, newest first.
    pub async fn list(&self, filter: IdeaFilter) -> Result<Vec<Idea>, Error> {
        self.hub().store.ideas(filter).await
    }

    /// One trial of an idea.
    pub async fn trial(&self, idea: IdeaId, trial: TrialId) -> Result<Trial, Error> {
        self.hub()
            .store
            .trial(trial)
            .await?
            .filter(|t| t.idea == idea)
            .ok_or_else(|| Error::not_found(format!("Idea {idea} has no trial {trial}")))
    }

    /// The idea's running settings.
    pub async fn spec(&self, id: IdeaId) -> Result<IdeaSpec, Error> {
        let value = self.hub().store.idea_spec(id).await?.ok_or_else(|| Error::not_found(format!("No idea {id}")))?;
        serde_json::from_value(value).map_err(|e| Error::internal(format!("idea {id} has an unreadable spec: {e}")))
    }

    /// A tune idea's settings and SPSA history.
    pub async fn tune(&self, id: IdeaId) -> Result<TuneView, Error> {
        let spec = self.spec(id).await?.tune.ok_or_else(|| Error::not_found(format!("Idea {id} is not a tune")))?;
        let params = self.hub().store.meta::<Vec<String>>(&tune::params_key(id)).await?.unwrap_or_default();
        let steps = self.hub().store.tune_steps(id).await?;
        Ok(TuneView { spec, params, steps })
    }

    /// The work running or queued for an idea's trials, if any.
    async fn active_work(&self, idea: &Idea) -> Result<Option<Work>, Error> {
        let lists = self.scheduler().list(0).await?;
        Ok(lists.running.into_iter().chain(lists.queued).find(|w| {
            matches!(w.subject, Subject::Trial { id } if idea.trials.iter().any(|t| t.id == id))
                && matches!(w.kind, WorkKind::Trial | WorkKind::Tune | WorkKind::Train)
        }))
    }

    /// Stops an idea: its running trial at the next safe point (keeping its rounds), or a queued one at once.
    pub async fn stop(&self, id: IdeaId) -> Result<Idea, Error> {
        let idea = self.get(id).await?;
        if idea.status.is_final() {
            return Ok(idea);
        }
        match self.active_work(&idea).await? {
            // The end of the work stops the idea (see `Funnel`).
            Some(w) => {
                self.scheduler().stop(w.id).await?;
                self.get(id).await
            }
            None => self.end_stopped(id).await,
        }
    }

    /// Carries on a stopped or failed idea from the trial that did not finish.
    pub async fn resume(&self, id: IdeaId) -> Result<Idea, Error> {
        let idea = self.get(id).await?;
        if !matches!(idea.status, IdeaStatus::Stopped | IdeaStatus::Failed) {
            return Err(Error::conflict(format!(
                "Idea {id} is {:?}: only a stopped or failed idea resumes",
                idea.status
            )));
        }
        let store = &self.hub().store;
        let trials = store.trials(id).await?;
        let Some(index) = trials.iter().position(|t| t.status != TrialStatus::Passed) else {
            return Err(Error::conflict(format!("Every trial of idea {id} passed")));
        };
        for t in trials.into_iter().skip(index) {
            let t = Trial { status: TrialStatus::Waiting, ended_at: None, ..t };
            store.save_trial(t.clone()).await?;
            self.emit_trial(&t);
        }
        store.update_idea(id, IdeaUpdate { status: Some(IdeaStatus::Queued), ..Default::default() }).await?;
        // The verdict and end are cleared by writing empty values: the store keeps `None` as "unchanged".
        store.clear_idea_end(id).await?;
        self.queue_trial(id, index).await?;
        self.emit_idea(id).await
    }

    /// Queues the promotion of a kept idea into the fork's `main`.
    pub async fn promote(&self, id: IdeaId) -> Result<Work, Error> {
        let idea = self.get(id).await?;
        if idea.status != IdeaStatus::Kept {
            return Err(Error::conflict(format!("Only a kept idea can be promoted; idea {id} is {:?}", idea.status)));
        }
        if self.spec(id).await?.harness.is_some() {
            return Err(Error::conflict("A harness check is not a change to promote"));
        }
        self.scheduler()
            .submit(NewWork {
                kind: WorkKind::Promote,
                priority: Priority::Interactive,
                title: format!("Promoting {}", idea.branch),
                subject: Subject::Idea { id },
                spec: serde_json::to_value(PromoteWork { idea: id }).map_err(|e| Error::internal(e.to_string()))?,
            })
            .await
    }

    pub(crate) async fn emit_idea(&self, id: IdeaId) -> Result<Idea, Error> {
        let idea = self.get(id).await?;
        self.hub().bus.emit(Payload::IdeaUpserted(Box::new(idea.clone())));
        Ok(idea)
    }

    fn emit_trial(&self, t: &Trial) {
        self.hub().bus.emit(Payload::TrialUpserted(Box::new(t.clone())));
    }

    /// Saves a trial and tells clients.
    pub(crate) async fn save_trial(&self, t: &Trial) -> Result<(), Error> {
        self.hub().store.save_trial(t.clone()).await?;
        self.emit_trial(t);
        Ok(())
    }

    /// Marks an idea running when its first trial starts.
    pub(crate) async fn mark_running(&self, id: IdeaId) -> Result<(), Error> {
        let idea = self.get(id).await?;
        if idea.status == IdeaStatus::Queued {
            self.hub()
                .store
                .update_idea(id, IdeaUpdate { status: Some(IdeaStatus::Running), ..Default::default() })
                .await?;
            self.emit_idea(id).await?;
        }
        Ok(())
    }

    /// After trial `index` ended: the next trial, or the idea's end.
    async fn advance(&self, id: IdeaId, index: usize) -> Result<(), Error> {
        let spec = self.spec(id).await?;
        if let Some(run) = &spec.harness {
            // A harness check runs every trial, then judges them together.
            if index + 1 < spec.funnel.len() {
                self.queue_trial(id, index + 1).await?;
                return Ok(());
            }
            return crate::harness::finish(self, id, run).await;
        }
        let trials = self.hub().store.trials(id).await?;
        let passed = trials.get(index).is_some_and(|t| t.status == TrialStatus::Passed);
        match funnel::next(&spec.funnel, index, passed) {
            Next::Run(next) => {
                self.queue_trial(id, next).await?;
            }
            Next::Kept => self.end(id, IdeaStatus::Kept, Some(index), None).await?,
            Next::Dropped => self.end(id, IdeaStatus::Dropped, Some(index), None).await?,
        }
        Ok(())
    }

    /// Ends an idea: its verdict, the trials it will not run skipped, and (when kept) its version's Elo.
    async fn end(
        &self,
        id: IdeaId,
        status: IdeaStatus,
        index: Option<usize>,
        error: Option<String>,
    ) -> Result<(), Error> {
        let store = &self.hub().store;
        let trials = store.trials(id).await?;
        let deciding = match status {
            IdeaStatus::Kept => deciding(&trials),
            _ => index.and_then(|i| trials.get(i)).map(|t| (t.stage, t.elo, t.detail.clone())),
        };
        let detail = error.or_else(|| deciding.as_ref().and_then(|d| d.2.clone()));
        let verdict = funnel::idea_verdict(status, deciding.as_ref().map(|d| (d.0, d.1)), detail.as_deref());
        if status != IdeaStatus::Stopped {
            for t in trials.iter().filter(|t| t.status == TrialStatus::Waiting) {
                let t = Trial { status: TrialStatus::Skipped, ..t.clone() };
                self.save_trial(&t).await?;
            }
        }
        let idea = store
            .update_idea(
                id,
                IdeaUpdate {
                    status: Some(status),
                    verdict: Some(verdict),
                    ended_at: Some(now()),
                    ..Default::default()
                },
            )
            .await?;
        if status == IdeaStatus::Kept
            && let (Some(version), Some((_, Some(elo), _))) = (idea.version, &deciding)
        {
            store.set_version_elo(version, Some(*elo)).await?;
        }
        self.emit_idea(id).await?;
        Ok(())
    }

    async fn end_stopped(&self, id: IdeaId) -> Result<Idea, Error> {
        for t in self.hub().store.trials(id).await? {
            if t.status == TrialStatus::Running {
                self.save_trial(&Trial { status: TrialStatus::Stopped, ..t }).await?;
            }
        }
        self.end(id, IdeaStatus::Stopped, None, None).await?;
        self.get(id).await
    }
}

/// The trial that decides a kept idea: its last games stage with an estimate (short, long, confirmation or custom),
/// else its last passed trial.
fn deciding(trials: &[Trial]) -> Option<(Stage, Option<Estimate>, Option<String>)> {
    let passed = || trials.iter().rev().filter(|t| t.status == TrialStatus::Passed);
    passed()
        .find(|t| matches!(t.stage, Stage::Short | Stage::Long | Stage::Confirm | Stage::Custom) && t.elo.is_some())
        .or_else(|| passed().next())
        .map(|t| (t.stage, t.elo, t.detail.clone()))
}

/// Moves ideas along their funnel as their trials' work ends.
struct Funnel {
    ideas: Ideas,
}

#[async_trait]
impl EndHook for Funnel {
    async fn ended(&self, work: &Work, _result: Option<&serde_json::Value>) {
        if let Err(e) = self.handle(work).await {
            eprintln!("ideas: after work {}: {e}", work.id);
        }
    }
}

impl Funnel {
    async fn handle(&self, work: &Work) -> Result<(), Error> {
        let store = &self.ideas.hub().store;
        let Some(spec) = store.work_spec(work.id).await? else { return Ok(()) };
        // A training run of no idea has no trial to move along.
        let Ok(w) = serde_json::from_value::<TrialWork>(spec) else { return Ok(()) };
        let idea = self.ideas.get(w.idea).await?;
        if idea.status.is_final() {
            return Ok(());
        }
        match work.status {
            WorkStatus::Done => self.ideas.advance(w.idea, w.index).await,
            WorkStatus::Stopped => self.ideas.end_stopped(w.idea).await.map(drop),
            WorkStatus::Failed => {
                let error = work.error.clone().unwrap_or_else(|| "see the work's log".into());
                if let Some(t) = store.trial(w.trial).await? {
                    let t =
                        Trial { status: TrialStatus::Failed, detail: Some(error.clone()), ended_at: Some(now()), ..t };
                    self.ideas.save_trial(&t).await?;
                }
                self.ideas.end(w.idea, IdeaStatus::Failed, Some(w.index), Some(error)).await
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
