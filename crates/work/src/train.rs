//! Training runs: the `Train` executor, bullet on the GPU (`trainer/README.md`).
//!
//! A run holds the ledger's GPU for its whole length and two to four cores for bullet's data loader, so a trial can
//! play beside it. It runs `bullet-train train` on the datasets' viriformat files (from their manifests): a fine-tune
//! of a parent net (the champion's by default; for a net idea, its base's) or a run from scratch. bullet writes JSON
//! lines on stdout:
//!
//! - `progress` lines become `train.progress` events (at most one a second, on `idea:<id>` for a net idea, else
//!   `work`), the work's progress, and rows of `train_steps`, so the loss chart has the whole history from its first
//!   load;
//! - `checkpoint` lines are resumable points: the last one is the work's checkpoint, and older ones beyond
//!   `keep_checkpoints` are deleted (bullet deletes its own within a run; a resumed run's leftovers are deleted here);
//! - `done` gives the net, which is registered ([`crate::nets::Nets`]); the run's directory (its checkpoints, 0.72 GB
//!   each) is then removed;
//! - `error` (exit code 1) fails the run.
//!
//! Stopping or pausing kills the process; resuming runs it again with `--resume <last checkpoint>` (without
//! `--parent`). A net idea's run then records the idea's version: its base's code with the new net (`v14 + net-3`).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use dameless_core::{
    Error, IdeaId, TrialId, WorkId,
    events::{Payload, TrainProgress},
    ideas::{Stage, TrialStatus},
    training::{self, Net, NetId, NetRun, TrainSpec, TrainStep},
    work::{Priority, Progress, Subject, Work, WorkKind},
};
use dameless_hub::{Executor, Hub, WorkContext};
use dameless_store::{IdeaUpdate, NewWork, now};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    data::Datasets,
    health::Probes,
    ideas::Ideas,
    nets::{NetSource, Nets},
    trainer::{Line, Running},
};

/// What a `Train` work item runs. For a net idea's stage, `idea`, `trial` and `index` name it (the same fields as a
/// trial's work, so the funnel moves the idea along when it ends).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrainWork {
    pub train: TrainSpec,
    #[serde(default)]
    pub idea: Option<IdeaId>,
    #[serde(default)]
    pub trial: Option<TrialId>,
    #[serde(default)]
    pub index: Option<usize>,
}

/// `GET /api/ideas/{id}/train`: a net idea's training: its settings, every reading so far and the net it made.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrainView {
    pub spec: TrainSpec,
    pub steps: Vec<TrainStep>,
    pub net: Option<Net>,
}

/// Where a run stands: the work's checkpoint.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct TrainState {
    /// The last checkpoint directory bullet wrote.
    last_checkpoint: Option<PathBuf>,
}

/// One of bullet's JSON lines.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BulletLine {
    Started {
        #[serde(default)]
        start_superbatch: u32,
        #[serde(default)]
        superbatches: u32,
    },
    Progress {
        superbatch: u32,
        batch: u32,
        loss: f64,
        lr: f64,
        #[serde(default)]
        positions_per_sec: f64,
        #[serde(default)]
        grad_norm: Option<f64>,
        #[serde(default)]
        elapsed_seconds: f64,
    },
    Superbatch {
        superbatch: u32,
        loss: f64,
    },
    Checkpoint {
        superbatch: u32,
        path: String,
    },
    Done {
        net: String,
        #[serde(default)]
        positions: u64,
        #[serde(default)]
        dropped_inputs: u64,
    },
    Error {
        message: String,
    },
    Warning {
        #[serde(default)]
        message: String,
    },
}

/// Reads one line of bullet's stdout: `None` for anything that is not one of its JSON objects.
pub fn parse_line(line: &str) -> Option<BulletLine> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    serde_json::from_str(line).ok()
}

/// The checkpoints of a run, newest first: `(superbatch, directory)` of each `<name>-<sb>` under `checkpoints/`.
pub fn checkpoints(out: &Path, name: &str) -> Vec<(u32, PathBuf)> {
    let prefix = format!("{name}-");
    let mut found: Vec<(u32, PathBuf)> = std::fs::read_dir(out.join("checkpoints"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            let sb = file.strip_prefix(&prefix)?.parse().ok()?;
            Some((sb, e.path()))
        })
        .collect();
    found.sort_by_key(|c| std::cmp::Reverse(c.0));
    found
}

/// Deletes checkpoints beyond the newest `keep`.
fn prune(out: &Path, name: &str, keep: u32) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    for (_, dir) in checkpoints(out, name).into_iter().skip(keep.max(1) as usize) {
        if std::fs::remove_dir_all(&dir).is_ok() {
            removed.push(dir);
        }
    }
    removed
}

/// Training runs. Cheap to clone.
#[derive(Clone)]
pub struct Training {
    inner: Arc<Inner>,
}

struct Inner {
    ideas: Ideas,
    data: Datasets,
    nets: Nets,
    probes: Arc<dyn Probes>,
}

impl Training {
    pub fn new(ideas: Ideas, data: Datasets, nets: Nets, probes: Arc<dyn Probes>) -> Self {
        Self { inner: Arc::new(Inner { ideas, data, nets, probes }) }
    }

    /// Registers the `Train` executor with the scheduler.
    pub fn register(&self) {
        self.inner.ideas.versions().scheduler().register(Arc::new(TrainExecutor { training: self.clone() }));
    }

    fn hub(&self) -> &Arc<Hub> {
        self.inner.ideas.versions().hub()
    }

    /// Checks a run's settings: the schedule, the parent and the datasets exist, a GPU is there.
    pub async fn check(&self, spec: &TrainSpec) -> Result<(), Error> {
        if let Some(p) = spec.schedule.problem() {
            return Err(Error::bad_request(p));
        }
        if spec.save_rate == 0 {
            return Err(Error::bad_request("save_rate must be at least 1"));
        }
        if spec.from_scratch && spec.parent.is_some() {
            return Err(Error::bad_request("A run from scratch has no parent net"));
        }
        if let Some(name) = &spec.name
            && !training::valid_name(name)
        {
            return Err(Error::bad_request(format!("{name:?} is not a net name")));
        }
        if let Some(p) = spec.parent {
            self.inner.nets.get(p).await?;
        }
        for d in &spec.datasets {
            self.inner.data.get(*d).await?;
        }
        if self.hub().ledger.gpus() == 0 {
            return Err(Error::conflict("Training needs an NVIDIA GPU, and none was found (DAMELESS_GPUS)"));
        }
        Ok(())
    }

    /// Queues a run that belongs to no idea (`POST /api/train`): its net is registered, nothing is tested.
    pub async fn submit(&self, spec: TrainSpec) -> Result<Work, Error> {
        self.check(&spec).await?;
        let champion = self.inner.ideas.versions().champion().await?;
        self.inner
            .ideas
            .versions()
            .scheduler()
            .submit(NewWork {
                kind: WorkKind::Train,
                priority: Priority::Idea,
                title: format!("Training {}", spec.name.as_deref().unwrap_or("a net")),
                subject: Subject::Version { id: champion.id },
                spec: serde_json::to_value(TrainWork { train: spec, idea: None, trial: None, index: None })
                    .map_err(|e| Error::internal(e.to_string()))?,
            })
            .await
    }

    /// A net idea's training: settings, readings and the net.
    pub async fn idea(&self, id: IdeaId) -> Result<TrainView, Error> {
        let ideas = &self.inner.ideas;
        let spec = ideas.spec(id).await?.train.ok_or_else(|| Error::not_found(format!("Idea {id} is not a net")))?;
        let steps = self.hub().store.idea_train_steps(id).await?;
        let idea = ideas.get(id).await?;
        let net = match idea.version {
            Some(v) => match self.hub().store.version(v).await?.and_then(|v| v.net) {
                Some(n) => self.hub().store.net(n).await?,
                None => None,
            },
            None => None,
        };
        Ok(TrainView { spec, steps, net })
    }

    /// A run's readings (`GET /api/work/{id}/train`).
    pub async fn steps(&self, work: WorkId) -> Result<Vec<TrainStep>, Error> {
        self.hub().store.train_steps(work).await
    }
}

struct TrainExecutor {
    training: Training,
}

/// How one run of bullet came out.
enum End {
    Done { net: PathBuf },
    Paused,
    Stopped,
}

#[async_trait]
impl Executor for TrainExecutor {
    fn kind(&self) -> WorkKind {
        WorkKind::Train
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let w: TrainWork = cx.spec()?;
        let t = &self.training.inner;
        let hub = self.training.hub().clone();
        let ideas = &t.ideas;
        let versions = ideas.versions();
        let spec = w.train.clone();
        if let Some(p) = spec.schedule.problem() {
            return Err(Error::bad_request(p));
        }
        if hub.ledger.gpus() == 0 {
            return Err(Error::conflict("Training needs an NVIDIA GPU, and none was found (DAMELESS_GPUS)"));
        }

        // A net idea's stage: its trial runs, and its base is what the net is made for.
        let idea = match w.idea {
            Some(id) => Some(ideas.get(id).await?),
            None => None,
        };
        if let (Some(idea), Some(trial)) = (&idea, w.trial) {
            let mut tr = ideas.trial(idea.id, trial).await?;
            if tr.status != TrialStatus::Running {
                tr.status = TrialStatus::Running;
                tr.started_at.get_or_insert_with(now);
                ideas.save_trial(&tr).await?;
            }
            ideas.mark_running(idea.id).await?;
            cx.set_title(format!("Training a net for {}", idea.title)).await?;
        }
        let base = match &idea {
            Some(i) => {
                hub.store.version(i.base).await?.ok_or_else(|| Error::not_found("The idea's base version is gone"))?
            }
            None => versions.champion().await?,
        };

        let mut log = |l: &str| cx.log(l);
        let bullet = t
            .data
            .trainer()
            .bullet(&cx.cancel, &mut log)
            .await
            .map_err(|e| if cx.cancel.is_cancelled() { cx.stopped() } else { Error::conflict(format!("{e:#}")) })?;
        let parent = match (spec.from_scratch, spec.parent) {
            (true, _) => None,
            (false, Some(p)) => Some(t.nets.get(p).await?),
            (false, None) => Some(t.nets.version_net(versions, &base).await?),
        };
        let (files, dataset) = t.data.training_files(&spec.datasets).await?;
        let (out, name) = match &idea {
            Some(i) => (hub.config.runs_dir().join(format!("idea-{}", i.id)), format!("net-{}", i.id)),
            None => (hub.config.runs_dir().join(format!("work-{}", cx.id)), format!("net-w{}", cx.id)),
        };
        let name = spec.name.clone().unwrap_or(name);
        std::fs::create_dir_all(&out).map_err(|e| Error::internal(format!("{}: {e}", out.display())))?;
        let mut state = cx.checkpoint::<TrainState>().unwrap_or_default();
        if state.last_checkpoint.as_ref().is_none_or(|p| !p.is_dir()) {
            // A run carried on by a new work item (a resumed idea) picks up the checkpoints on disk.
            state.last_checkpoint = checkpoints(&out, &name).into_iter().next().map(|(_, p)| p);
        }
        let existing: u64 = checkpoints(&out, &name).len() as u64 * training::CHECKPOINT_BYTES;
        let need = training::run_disk_bytes(&spec).saturating_sub(existing);
        training::check_room(need, 0, 0, u64::MAX, t.probes.disk(&hub.config.data_dir).map(|d| d.free_bytes))
            .map_err(Error::conflict)?;
        cx.log(format!(
            "{} on {} files, {} positions in {} superbatches{}",
            name,
            files.len(),
            spec.schedule.positions(),
            spec.schedule.superbatches,
            match &parent {
                Some(p) => format!(", fine-tuning {}", p.name),
                None => ", from scratch".into(),
            }
        ));

        let bps = spec.schedule.batches_per_superbatch;
        let total_batches = u64::from(spec.schedule.superbatches) * u64::from(bps);
        let mut final_loss = None;
        let net_file = loop {
            if !cx.safe_point(None).await {
                return Err(cx.stopped());
            }
            let Some(_gpu) = cx.gpu().await else { return Err(cx.stopped()) };
            let Some(cores) = cx.cores(2, 4).await else { return Err(cx.stopped()) };
            let threads = cores.cores().max(1).to_string();
            let s = &spec.schedule;
            let mut args: Vec<std::ffi::OsString> = vec!["train".into(), "--data".into()];
            args.extend(files.iter().map(|f| f.as_os_str().to_owned()));
            args.extend(["--format".into(), "viriformat".into(), "--out".into(), out.as_os_str().to_owned()]);
            args.extend(["--name".into(), name.clone().into()]);
            match (&state.last_checkpoint, &parent) {
                (Some(ck), _) => args.extend(["--resume".into(), ck.as_os_str().to_owned()]),
                (None, Some(p)) => args.extend(["--parent".into(), p.path.clone().into()]),
                (None, None) => {}
            }
            for (flag, value) in [
                ("--superbatches", s.superbatches.to_string()),
                ("--batches-per-superbatch", s.batches_per_superbatch.to_string()),
                ("--batch-size", s.batch_size.to_string()),
                ("--lr", s.lr.to_string()),
                ("--final-lr", s.final_lr.to_string()),
                ("--lr-schedule", s.lr_schedule.as_str().to_owned()),
                ("--wdl", s.wdl.to_string()),
                ("--eval-scale", "380".to_owned()),
                ("--save-rate", spec.save_rate.to_string()),
                ("--keep-checkpoints", spec.keep_checkpoints.max(1).to_string()),
                ("--read-threads", threads.clone()),
                ("--map-threads", threads.clone()),
            ] {
                args.extend([flag.into(), value.into()]);
            }
            if let Some(f) = s.final_wdl {
                args.extend(["--final-wdl".into(), f.to_string().into()]);
            }
            let command = bullet.command(args);
            cx.log(command.display());
            let mut run = Running::start(&command).map_err(|e| Error::internal(format!("{e:#}")))?;
            let (mut error, mut done, mut tail) = (None::<String>, None::<PathBuf>, Vec::<String>::new());
            let mut last_event: Option<Instant> = None;
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            let end = loop {
                tokio::select! {
                    line = run.next_line() => match line {
                        Some(Line::Stdout(l)) => match parse_line(&l) {
                            Some(BulletLine::Progress { superbatch, batch, loss, lr, positions_per_sec, grad_norm, elapsed_seconds }) => {
                                let step = TrainStep { seq: 0, superbatch, batch, loss, lr, grad_norm, positions_per_sec, elapsed_seconds };
                                hub.store.add_train_steps(cx.id, w.idea, vec![step]).await?;
                                if last_event.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
                                    last_event = Some(Instant::now());
                                    hub.bus.emit(Payload::TrainProgress(TrainProgress {
                                        work: cx.id,
                                        idea: w.idea,
                                        superbatch,
                                        superbatches: spec.schedule.superbatches,
                                        batch,
                                        batches_per_superbatch: bps,
                                        loss,
                                        lr,
                                        positions_per_sec,
                                        grad_norm,
                                        elapsed_seconds,
                                    }));
                                }
                                let done_batches = u64::from(superbatch.saturating_sub(1)) * u64::from(bps) + u64::from(batch);
                                let _ = cx.progress(Progress {
                                    done: done_batches,
                                    total: Some(total_batches),
                                    unit: "batches".into(),
                                    eta_seconds: (positions_per_sec > 0.0).then(|| {
                                        (total_batches.saturating_sub(done_batches) * u64::from(spec.schedule.batch_size)) as f64 / positions_per_sec
                                    } as u64),
                                    text: Some(format!(
                                        "superbatch {superbatch} of {}, loss {loss:.6}, {:.0}k positions/s",
                                        spec.schedule.superbatches,
                                        positions_per_sec / 1000.0
                                    )),
                                }).await;
                            }
                            Some(BulletLine::Superbatch { superbatch, loss }) => {
                                final_loss = Some(loss);
                                cx.log(format!("superbatch {superbatch}: loss {loss:.6}"));
                            }
                            Some(BulletLine::Checkpoint { superbatch, path }) => {
                                cx.log(format!("checkpoint at superbatch {superbatch}: {path}"));
                                state.last_checkpoint = Some(PathBuf::from(path));
                                cx.save_checkpoint(&state).await?;
                                for gone in prune(&out, &name, spec.keep_checkpoints) {
                                    cx.log(format!("deleted the old checkpoint {}", gone.display()));
                                }
                            }
                            Some(BulletLine::Done { net, positions, dropped_inputs }) => {
                                cx.log(format!("done: {net}, {positions} positions, {dropped_inputs} threat inputs dropped"));
                                done = Some(PathBuf::from(net));
                            }
                            Some(BulletLine::Error { message }) => error = Some(message),
                            Some(BulletLine::Warning { message }) => cx.log(format!("warning: {message}")),
                            Some(BulletLine::Started { start_superbatch, superbatches }) => {
                                cx.log(format!("started at superbatch {start_superbatch} of {superbatches}"));
                            }
                            None => cx.log(&l),
                        },
                        Some(Line::Stderr(l)) => {
                            cx.log(&l);
                            tail.push(l);
                            if tail.len() > 20 {
                                tail.remove(0);
                            }
                        }
                        None => {
                            let status = run.wait().await.map_err(|e| Error::internal(format!("{e:#}")))?;
                            if let Some(message) = error {
                                return Err(Error::internal(format!("bullet: {message}")));
                            }
                            match done {
                                Some(net) if status.success() => break End::Done { net },
                                _ => {
                                    let tail = tail.iter().rev().take(5).rev().cloned().collect::<Vec<_>>().join(" / ");
                                    return Err(Error::internal(format!("bullet-train failed ({status}): {tail}")));
                                }
                            }
                        }
                    },
                    _ = cx.cancel.cancelled() => {
                        run.kill().await;
                        break End::Stopped;
                    }
                    _ = tick.tick() => if cx.pause_asked() {
                        run.kill().await;
                        break End::Paused;
                    },
                }
            };
            match end {
                End::Done { net } => break net,
                End::Stopped => return Err(cx.stopped()),
                End::Paused => cx.log(format!(
                    "Paused: training resumes from {}",
                    state.last_checkpoint.as_ref().map_or("the start".into(), |p| p.display().to_string())
                )),
            }
        };

        let net_file = if net_file.is_absolute() { net_file } else { out.join(net_file) };
        let net = t
            .nets
            .register_file(
                &net_file,
                NetSource {
                    name: name.clone(),
                    parent: parent.as_ref().map(|p| p.id),
                    dataset,
                    run: Some(NetRun {
                        idea: w.idea,
                        work: Some(cx.id),
                        superbatches: spec.schedule.superbatches,
                        positions: spec.schedule.positions(),
                        final_loss,
                    }),
                },
            )
            .await?;
        cx.log(format!("Registered net {} ({})", net.name, &net.sha256[..12.min(net.sha256.len())]));
        // The net is in nets/: the run's checkpoints are not needed any more.
        let _ = std::fs::remove_dir_all(&out);

        let mut result =
            serde_json::json!({ "net": net.id, "name": net.name, "sha256": net.sha256, "final_loss": final_loss });
        if let (Some(idea), Some(trial)) = (&idea, w.trial) {
            let version = versions.net_version(idea.id, &base, net.id, &net.name).await?;
            hub.store.update_idea(idea.id, IdeaUpdate { version: Some(version.id), ..Default::default() }).await?;
            ideas.emit_idea(idea.id).await?;
            let mut tr = ideas.trial(idea.id, trial).await?;
            debug_assert_eq!(tr.stage, Stage::Train);
            tr.status = TrialStatus::Passed;
            tr.detail = Some(format!(
                "{}: {} superbatches, {:.1} M positions{}",
                net.name,
                spec.schedule.superbatches,
                spec.schedule.positions() as f64 / 1e6,
                final_loss.map(|l| format!(", loss {l:.6}")).unwrap_or_default()
            ));
            tr.ended_at = Some(now());
            ideas.save_trial(&tr).await?;
            result["version"] = serde_json::json!(version.id);
        }
        Ok(result)
    }
}

/// The net of a version, for the routes: none for the commit's own.
pub async fn version_net(hub: &Hub, net: Option<NetId>) -> Result<Option<Net>, Error> {
    match net {
        Some(n) => hub.store.net(n).await,
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bullets_lines() {
        let p = parse_line(
            r#"{"type":"progress","superbatch":2,"batch":32,"loss":0.0012,"lr":0.0001,"wdl":0.3,"positions_per_sec":760000.0,"grad_norm":0.5,"grad_norms":{"l1w":0.1},"positions":1,"elapsed_seconds":3.5}"#,
        );
        assert_eq!(
            p,
            Some(BulletLine::Progress {
                superbatch: 2,
                batch: 32,
                loss: 0.0012,
                lr: 0.0001,
                positions_per_sec: 760000.0,
                grad_norm: Some(0.5),
                elapsed_seconds: 3.5
            })
        );
        assert!(matches!(
            parse_line(
                r#"{"type":"done","net":"x.nnue","sha256":"ab","superbatches":1,"positions":5,"elapsed_seconds":1,"dropped_inputs":0}"#
            ),
            Some(BulletLine::Done { .. })
        ));
        assert_eq!(
            parse_line(r#"{"type":"error","message":"no data"}"#),
            Some(BulletLine::Error { message: "no data".into() })
        );
        assert_eq!(parse_line("loading data..."), None);
        assert_eq!(parse_line(r#"{"type":"something-new"}"#), None);
    }

    #[test]
    fn keeps_the_newest_checkpoints() {
        let dir = tempfile::tempdir().unwrap();
        for sb in [10, 20, 30] {
            std::fs::create_dir_all(dir.path().join("checkpoints").join(format!("net-1-{sb}"))).unwrap();
        }
        std::fs::create_dir_all(dir.path().join("checkpoints").join("other-5")).unwrap();
        assert_eq!(checkpoints(dir.path(), "net-1").iter().map(|c| c.0).collect::<Vec<_>>(), [30, 20, 10]);
        assert_eq!(prune(dir.path(), "net-1", 2).len(), 1);
        assert_eq!(checkpoints(dir.path(), "net-1").iter().map(|c| c.0).collect::<Vec<_>>(), [30, 20]);
    }
}
