//! The regression check (the rules are in `core::regression`): after every fifth promotion, the new champion plays
//! the champion five promotions earlier for a fixed number of pairs at STC.
//!
//! It is modelled as the harness check is: a ready-made idea (kind `gain`, tag `regression`) whose spec says what it
//! checks, with one trial of fixed pairs, queued at idea priority. The idea's base is the older champion and its
//! version the newer, so the trial executor plays them like any idea. When the pairs are played the trial is judged
//! on its Elo rather than on having played them, a failure is written loudly into the work's log, and the idea ends
//! kept (passed) or dropped (failed) with the check's sentence as its verdict. The result is kept in `meta`
//! (`regression.checks`) for the logbook and the strength line. Nothing is ever reverted.
//!
//! The check is queued when a promotion or an upstream sync ends (both make a new champion); a champion whose check
//! exists already is not checked twice.

use async_trait::async_trait;
use dameless_core::{
    Error, IdeaId, VersionId,
    ideas::{self as funnel, Author, Idea, IdeaKind, IdeaStatus, Stage, StagePlan, StageVerdict, Trial},
    regression::{self, RegressionCheck},
    version::{Line, Version},
    work::{Work, WorkKind, WorkStatus},
};
use dameless_store::{IdeaFilter, IdeaUpdate, now};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    ideas::{IdeaSpec, Ideas, NewIdeaRequest},
    scheduler::EndHook,
};

/// What a regression check idea carries in its spec: the two champions it plays, and how many pairs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegressionRun {
    pub champion: VersionId,
    pub champion_name: String,
    pub against: VersionId,
    pub against_name: String,
    pub pairs: u32,
}

/// Regression checks. Cheap to clone.
#[derive(Clone)]
pub struct Regression {
    ideas: Ideas,
    pairs: u32,
}

impl Regression {
    /// Checks of [`regression::PAIRS`] pairs.
    pub fn new(ideas: Ideas) -> Self {
        Self::with_pairs(ideas, regression::PAIRS)
    }

    /// Checks of another length (short ones, in tests).
    pub fn with_pairs(ideas: Ideas, pairs: u32) -> Self {
        Self { ideas, pairs: pairs.max(1) }
    }

    /// Queues a check, when one is due, after every promotion and upstream sync that ends.
    pub fn register(&self) {
        let hook: Arc<dyn EndHook> = Arc::new(AfterPromotion { regression: self.clone() });
        let scheduler = self.ideas.versions().scheduler();
        scheduler.on_end(WorkKind::Promote, hook.clone());
        scheduler.on_end(WorkKind::Upstream, hook);
    }

    /// Finished checks, newest first.
    pub async fn checks(&self) -> Result<Vec<RegressionCheck>, Error> {
        let mut checks = checks(&self.ideas).await?;
        checks.reverse();
        Ok(checks)
    }

    /// Queues the check the champion is due, if it is due one and has none yet. Answers the idea queued.
    pub async fn queue_if_due(&self) -> Result<Option<Idea>, Error> {
        let versions = self.ideas.versions();
        let champion = versions.champion().await?;
        let Line::Champion { number } = champion.line else { return Ok(None) };
        let Some(earlier) = regression::due(number) else { return Ok(None) };
        let line = versions.list(Some("champion")).await?;
        let Some(against) = line.into_iter().find(|v| v.line == Line::Champion { number: earlier }) else {
            return Ok(None);
        };
        if self.exists_for(champion.id).await? {
            return Ok(None);
        }
        self.queue(&champion, &against).await.map(Some)
    }

    /// Whether a check of this champion was ever queued.
    async fn exists_for(&self, champion: VersionId) -> Result<bool, Error> {
        let filter = IdeaFilter { tag: Some(regression::TAG.to_owned()), ..Default::default() };
        for idea in self.ideas.list(filter).await? {
            if self.ideas.spec(idea.id).await?.regression.is_some_and(|r| r.champion == champion) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Queues `champion` against `against`.
    async fn queue(&self, champion: &Version, against: &Version) -> Result<Idea, Error> {
        let mut plan: Vec<StagePlan> = funnel::custom(regression::PRESET, None, Some(self.pairs), None);
        plan[0].stage = Stage::Regression;
        let run = RegressionRun {
            champion: champion.id,
            champion_name: champion.name.clone(),
            against: against.id,
            against_name: against.name.clone(),
            pairs: self.pairs,
        };
        let request = NewIdeaRequest {
            title: format!("Regression check: {} against {}", champion.name, against.name),
            hypothesis: format!(
                "Over {} pairs at STC, {} is not measurably worse than {}, five promotions earlier.",
                self.pairs, champion.name, against.name
            ),
            kind: IdeaKind::Gain,
            branch: champion.name.clone(),
            base: Some(format!("#{}", against.id.0)),
            author: Author::You,
            tags: vec![regression::TAG.into()],
            notes: None,
            long: false,
            tune: None,
            train: None,
            custom: None,
            fix_bench: false,
        };
        let spec = IdeaSpec {
            funnel: plan,
            long: false,
            tune: None,
            train: None,
            custom: None,
            harness: None,
            regression: Some(run),
        };
        self.ideas.insert_spec(request, spec).await
    }
}

/// Finished checks, oldest first.
async fn checks(ideas: &Ideas) -> Result<Vec<RegressionCheck>, Error> {
    Ok(ideas.hub().store.meta::<Vec<RegressionCheck>>(regression::META_KEY).await?.unwrap_or_default())
}

/// The check's trial verdict once its pairs are played: passed unless the newer champion is measurably worse.
pub(crate) fn verdict(run: &RegressionRun, trial: &Trial) -> StageVerdict {
    let (passed, _, detail) = regression::judge(&run.champion_name, &run.against_name, &trial.penta);
    StageVerdict { passed, detail }
}

/// Ends a regression check idea once its trial ended: judged and stored when its pairs were all played, else failed
/// (something broke, not the champion) with nothing stored.
pub(crate) async fn finish(ideas: &Ideas, id: IdeaId, run: &RegressionRun) -> Result<(), Error> {
    let store = &ideas.hub().store;
    let trials = store.trials(id).await?;
    let trial = trials.first().ok_or_else(|| Error::internal(format!("Regression check {id} has no trial")))?;
    let update = if trial.penta.pairs() < u64::from(run.pairs) {
        IdeaUpdate {
            status: Some(IdeaStatus::Failed),
            verdict: Some(format!(
                "Failed: the regression check did not finish ({})",
                trial.detail.clone().unwrap_or_default()
            )),
            ended_at: Some(now()),
            ..Default::default()
        }
    } else {
        let (passed, elo, summary) = regression::judge(&run.champion_name, &run.against_name, &trial.penta);
        let check = RegressionCheck {
            idea: id,
            champion: run.champion,
            champion_name: run.champion_name.clone(),
            against: run.against,
            against_name: run.against_name.clone(),
            passed,
            elo,
            pairs: trial.penta.pairs(),
            summary: summary.clone(),
            at: now(),
        };
        let mut all = checks(ideas).await?;
        all.retain(|c| c.idea != id);
        all.push(check);
        store.put_meta(regression::META_KEY, &all).await?;
        if !passed {
            eprintln!("regression: {summary}");
        }
        IdeaUpdate {
            status: Some(if passed { IdeaStatus::Kept } else { IdeaStatus::Dropped }),
            verdict: Some(summary),
            ended_at: Some(now()),
            ..Default::default()
        }
    };
    store.update_idea(id, update).await?;
    ideas.emit_idea(id).await?;
    Ok(())
}

/// Queues a check after a promotion or an upstream sync that made a new champion.
struct AfterPromotion {
    regression: Regression,
}

#[async_trait]
impl EndHook for AfterPromotion {
    async fn ended(&self, work: &Work, _result: Option<&serde_json::Value>) {
        if work.status != WorkStatus::Done {
            return;
        }
        match self.regression.queue_if_due().await {
            Ok(_) => {}
            Err(e) => eprintln!("regression: queue a check after work {}: {e}", work.id),
        }
    }
}

#[cfg(test)]
mod tests;
