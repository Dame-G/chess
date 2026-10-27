//! Datasets: the data registry, self-play datagen, imports of other people's data, and trial games as data.
//!
//! A dataset is a directory under the data directory's `data/` with a `manifest.json`, which the trainer's `data` tool
//! writes (`trainer/data/README.md`); the registry reads it ([`Datasets::register_dir`]). Every dataset together stays
//! within the cap (`DAMELESS_DATA_CAP_GB`, 10 GB on Windows), and nothing that writes much starts with less than 15 GB
//! free on the disk ([`training::KEEP_FREE_BYTES`]).
//!
//! - **Datagen** (`WorkKind::Datagen`): Reckless's `datagen` branch (a version, built like any other, with the
//!   champion's net when the champion is code plus a trained net) plays itself through `data datagen`, scores in
//!   Reckless's internal units (what the trainer wants). Progress comes from the tool's stderr. It is stopped by killing
//!   the process and resumed by running it again on the same directory (the tool cuts each part back to its last whole
//!   game), so a pause for live play is a kill and a later rerun. The dataset is registered from the manifest when it
//!   ends and updated by every run.
//! - **Import** (`WorkKind::DatasetImport`): one shard downloaded with progress, its size (and SHA-256, when given)
//!   checked, converted to viriformat with `data convert`, the manifest rebuilt. The route queues it only with your
//!   explicit go-ahead for its size, and it is refused when the cap or the disk would be exceeded.
//! - **Trial games** ([`Datasets::from_trials`]): an idea's trial PGNs converted with `data convert`.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use dameless_core::{
    Error, IdeaId,
    events::Payload,
    training::{self, Dataset, DatasetId, DatasetSource},
    work::{Priority, Progress, Subject, Work, WorkKind},
};
use dameless_hub::{Executor, Hub, WorkContext};
use dameless_store::{NewDataset, NewWork};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use utoipa::ToSchema;

use crate::{
    health::Probes,
    nets::hex,
    trainer::{Line, Running, Tool, Trainer},
    versions::{VersionRef, Versions},
};

/// Reckless's datagen branch, the default engine for datagen.
pub const DATAGEN_BRANCH: &str = "origin/datagen";

/// The units the trainer wants, as the `data` tool names them.
pub const TRAINING_UNITS: &str = "reckless-internal";

/// What the manifest of a dataset says (the part the registry reads).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub name: String,
    pub format: String,
    pub positions: u64,
    pub games: u64,
    pub size_bytes: u64,
    pub source: String,
    pub score_units: Option<String>,
    pub generator: Option<serde_json::Value>,
    pub files: Vec<ManifestFile>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ManifestFile {
    /// Relative to the dataset's directory, with `/`.
    pub path: String,
    pub format: String,
    pub size_bytes: u64,
    pub positions: u64,
}

impl Manifest {
    pub fn read(dir: &Path) -> Result<Self, Error> {
        let path = dir.join("manifest.json");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| Error::not_found(format!("No manifest at {}: {e}", path.display())))?;
        serde_json::from_str(&text).map_err(|e| Error::internal(format!("{} is unreadable: {e}", path.display())))
    }

    /// The viriformat files, absolute, which is what the trainer loads.
    pub fn viriformat_files(&self, dir: &Path) -> Vec<PathBuf> {
        self.files.iter().filter(|f| f.format == "viriformat").map(|f| dir.join(&f.path)).collect()
    }
}

/// `GET /api/datasets`: every dataset, and the room left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DataStatus {
    /// The cap on every dataset together (`DAMELESS_DATA_CAP_GB`).
    pub cap_bytes: u64,
    pub used_bytes: u64,
    /// Free space on the data directory's disk.
    pub disk_free_bytes: Option<u64>,
    /// What training work always leaves free.
    pub keep_free_bytes: u64,
    pub datasets: Vec<Dataset>,
}

/// `POST /api/datasets/datagen`: self-play data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DatagenRequest {
    /// The dataset to write or grow; `datagen` when left out. Running again on the same name resumes it.
    #[serde(default)]
    pub name: Option<String>,
    /// The dataset's total positions to reach (positions already there count).
    pub positions: u64,
    /// Worker threads (one engine each); half the machine's cores when left out, at most what the ledger grants.
    #[serde(default)]
    pub threads: Option<u32>,
    /// Soft nodes per move (the tool's default, 5,000, when left out).
    #[serde(default)]
    pub soft_nodes: Option<u64>,
    /// `dfrc` (default), `chess960` or `startpos`.
    #[serde(default)]
    pub opening: Option<String>,
    /// The engine: a version of Reckless's datagen branch (`origin/datagen` when left out).
    #[serde(default)]
    pub version: Option<String>,
}

/// `POST /api/datasets/import`: one shard to download, with your go-ahead for its size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ImportRequest {
    pub url: String,
    /// The download's size, as shown to you before you confirmed it.
    pub expected_bytes: u64,
    /// Checked after the download, when given.
    #[serde(default)]
    pub sha256: Option<String>,
    /// The dataset it goes into (shards of one set share a name).
    pub name: String,
    /// Your explicit go-ahead for downloading `expected_bytes`: without it the request is refused.
    #[serde(default)]
    pub confirm: bool,
}

/// What an import work item downloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportWork {
    pub url: String,
    pub expected_bytes: u64,
    pub sha256: Option<String>,
    pub name: String,
}

/// The datasets. Cheap to clone.
#[derive(Clone)]
pub struct Datasets {
    inner: Arc<Inner>,
}

struct Inner {
    versions: Versions,
    trainer: Arc<dyn Trainer>,
    probes: Arc<dyn Probes>,
    http: reqwest::Client,
}

fn io_error(path: &Path) -> impl Fn(std::io::Error) -> Error + '_ {
    move |e| Error::internal(format!("{}: {e}", path.display()))
}

/// Total bytes of the files under a directory.
fn dir_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_bytes(&e.path()),
            _ => e.metadata().map(|m| m.len()).unwrap_or(0),
        })
        .sum()
}

impl Datasets {
    pub fn new(versions: Versions, trainer: Arc<dyn Trainer>, probes: Arc<dyn Probes>) -> Self {
        Self { inner: Arc::new(Inner { versions, trainer, probes, http: reqwest::Client::new() }) }
    }

    /// Registers the `Datagen` and `DatasetImport` executors with the scheduler.
    pub fn register(&self) {
        let s = self.inner.versions.scheduler();
        s.register(Arc::new(DatagenExecutor { data: self.clone() }));
        s.register(Arc::new(ImportExecutor { data: self.clone() }));
    }

    fn hub(&self) -> &Arc<Hub> {
        self.inner.versions.hub()
    }

    pub fn trainer(&self) -> &Arc<dyn Trainer> {
        &self.inner.trainer
    }

    /// A dataset's directory.
    pub fn dir(&self, name: &str) -> PathBuf {
        self.hub().config.datasets_dir().join(name)
    }

    /// Free space on the data directory's disk.
    pub fn disk_free(&self) -> Option<u64> {
        self.inner.probes.disk(&self.hub().config.data_dir).map(|d| d.free_bytes)
    }

    pub async fn list(&self) -> Result<Vec<Dataset>, Error> {
        self.hub().store.datasets().await
    }

    pub async fn get(&self, id: DatasetId) -> Result<Dataset, Error> {
        self.hub().store.dataset(id).await?.ok_or_else(|| Error::not_found(format!("No dataset {id}")))
    }

    /// Bytes every dataset holds, as recorded, but `except` (the one being grown).
    async fn used(&self, except: Option<&str>) -> Result<u64, Error> {
        Ok(self.list().await?.iter().filter(|d| Some(d.name.as_str()) != except).map(|d| d.size_bytes).sum())
    }

    pub async fn status(&self) -> Result<DataStatus, Error> {
        let datasets = self.list().await?;
        Ok(DataStatus {
            cap_bytes: self.hub().config.data_cap_bytes,
            used_bytes: datasets.iter().map(|d| d.size_bytes).sum(),
            disk_free_bytes: self.disk_free(),
            keep_free_bytes: training::KEEP_FREE_BYTES,
            datasets,
        })
    }

    /// Reads a dataset directory's manifest and records it (by name; `source` replaces what the manifest says).
    pub async fn register_dir(&self, dir: &Path, source: Option<DatasetSource>) -> Result<Dataset, Error> {
        let m = Manifest::read(dir)?;
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(m.name.clone());
        let source = match source {
            Some(s) => s,
            None => match self.hub().store.dataset_by_name(&name).await? {
                Some(known) => known.source,
                None if m.source == "datagen" => {
                    DatasetSource::Datagen { settings: m.generator.clone().unwrap_or(serde_json::Value::Null) }
                }
                None => DatasetSource::Other { source: m.source.clone() },
            },
        };
        let d = self
            .hub()
            .store
            .upsert_dataset(NewDataset {
                name,
                format: m.format,
                positions: m.positions,
                games: m.games,
                size_bytes: m.size_bytes,
                source,
                score_units: m.score_units,
                path: dir.display().to_string(),
            })
            .await?;
        self.hub().bus.emit(Payload::DatasetUpserted(Box::new(d.clone())));
        Ok(d)
    }

    /// Registers every dataset directory with a manifest (at startup).
    pub async fn scan(&self) -> Result<Vec<Dataset>, Error> {
        let root = self.hub().config.datasets_dir();
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&root).map_err(io_error(&root))?.flatten() {
            if entry.path().join("manifest.json").is_file() {
                out.push(self.register_dir(&entry.path(), None).await?);
            }
        }
        Ok(out)
    }

    /// Work queued or running that writes or reads this dataset.
    async fn users(&self, name: &str, id: DatasetId) -> Result<Vec<Work>, Error> {
        let store = &self.hub().store;
        let lists = store.work_lists(0).await?;
        let mut out = Vec::new();
        for w in lists.running.into_iter().chain(lists.queued) {
            if !matches!(w.kind, WorkKind::Datagen | WorkKind::DatasetImport | WorkKind::Train) {
                continue;
            }
            let Some(spec) = store.work_spec(w.id).await? else { continue };
            let named = spec.get("name").and_then(|n| n.as_str()) == Some(name);
            let trains = spec
                .pointer("/train/datasets")
                .and_then(|d| d.as_array())
                .is_some_and(|d| d.is_empty() || d.iter().any(|x| x.as_i64() == Some(id.0)));
            if (named && w.kind != WorkKind::Train) || (w.kind == WorkKind::Train && trains) {
                out.push(w);
            }
        }
        Ok(out)
    }

    /// Deletes a dataset and its files. Needs `confirm` (it cannot be undone); 409 while work writes or trains on it.
    pub async fn delete(&self, id: DatasetId, confirm: bool) -> Result<Dataset, Error> {
        let d = self.get(id).await?;
        if !confirm {
            return Err(Error::bad_request(format!(
                "Deleting {} removes {} of data for good: confirm it",
                d.name,
                training::gb(d.size_bytes)
            )));
        }
        let users = self.users(&d.name, id).await?;
        if !users.is_empty() {
            let titles: Vec<String> = users.iter().map(|w| w.title.clone()).collect();
            return Err(Error::conflict(format!("{} is in use: {}", d.name, titles.join(", "))));
        }
        let root = self.hub().config.datasets_dir();
        let dir = PathBuf::from(&d.path);
        // Only ever a directory of our own under data/.
        if dir.parent() == Some(root.as_path()) && dir.is_dir() {
            std::fs::remove_dir_all(&dir).map_err(io_error(&dir))?;
        }
        self.hub().store.delete_dataset(id).await?;
        Ok(d)
    }

    /// Queues datagen into a dataset (a new one, or more of one).
    pub async fn datagen(&self, req: DatagenRequest) -> Result<Work, Error> {
        let name = req.name.clone().unwrap_or_else(|| "datagen".into());
        if !training::valid_name(&name) {
            return Err(Error::bad_request(format!("{name:?} is not a dataset name: letters, digits, - _ and .")));
        }
        if req.positions == 0 {
            return Err(Error::bad_request("Datagen needs a number of positions"));
        }
        if let Some(d) = self.hub().store.dataset_by_name(&name).await?
            && !matches!(d.source, DatasetSource::Datagen { .. })
        {
            return Err(Error::conflict(format!("{name} is not a datagen dataset")));
        }
        let own = self.hub().store.dataset_by_name(&name).await?.map_or(0, |d| d.size_bytes);
        let room = training::datagen_room(
            self.used(Some(&name)).await?,
            self.hub().config.data_cap_bytes,
            self.disk_free(),
            own,
        );
        if room <= own {
            return Err(Error::conflict(format!(
                "No room for more data: the datasets' cap is {} and the disk keeps {} free",
                training::gb(self.hub().config.data_cap_bytes),
                training::gb(training::KEEP_FREE_BYTES)
            )));
        }
        let spec = DatagenRequest { name: Some(name.clone()), ..req };
        self.inner
            .versions
            .scheduler()
            .submit(NewWork {
                kind: WorkKind::Datagen,
                priority: Priority::Idea,
                title: format!("Generating data · {name}"),
                subject: Subject::Version { id: self.inner.versions.champion().await?.id },
                spec: serde_json::to_value(spec).map_err(|e| Error::internal(e.to_string()))?,
            })
            .await
    }

    /// Queues the import of one shard, after your go-ahead for its size.
    pub async fn import(&self, req: ImportRequest) -> Result<Work, Error> {
        if !req.confirm {
            return Err(Error::bad_request(format!(
                "Downloading {} needs your go-ahead: confirm the size",
                training::gb(req.expected_bytes)
            )));
        }
        if !(req.url.starts_with("http://") || req.url.starts_with("https://")) {
            return Err(Error::bad_request("The URL must be http or https"));
        }
        if req.expected_bytes == 0 {
            return Err(Error::bad_request("Give the shard's size (expected_bytes)"));
        }
        if !training::valid_name(&req.name) {
            return Err(Error::bad_request(format!("{:?} is not a dataset name", req.name)));
        }
        if let Some(sha) = &req.sha256
            && (sha.len() != 64 || !sha.chars().all(|c| c.is_ascii_hexdigit()))
        {
            return Err(Error::bad_request("sha256 must be 64 hex digits"));
        }
        self.check_import_room(req.expected_bytes, &req.name).await?;
        let file = req.url.rsplit('/').next().unwrap_or("shard").to_owned();
        self.inner
            .versions
            .scheduler()
            .submit(NewWork {
                kind: WorkKind::DatasetImport,
                priority: Priority::Interactive,
                title: format!("Importing {file} into {}", req.name),
                subject: Subject::Version { id: self.inner.versions.champion().await?.id },
                spec: serde_json::to_value(ImportWork {
                    url: req.url,
                    expected_bytes: req.expected_bytes,
                    sha256: req.sha256.map(|s| s.to_ascii_lowercase()),
                    name: req.name,
                })
                .map_err(|e| Error::internal(e.to_string()))?,
            })
            .await
    }

    async fn check_import_room(&self, bytes: u64, name: &str) -> Result<(), Error> {
        let f = training::import_footprint(bytes);
        let used = self.used(None).await?;
        let _ = name;
        training::check_room(f.peak, f.kept, used, self.hub().config.data_cap_bytes, self.disk_free())
            .map_err(Error::conflict)
    }

    /// Runs a `data` command to the end; its stdout, or an error with the last of its stderr.
    async fn run_data(
        &self,
        tool: &Tool,
        args: Vec<std::ffi::OsString>,
        cx: Option<&WorkContext>,
    ) -> Result<String, Error> {
        let mut run = Running::start(&tool.command(args)).map_err(|e| Error::internal(format!("{e:#}")))?;
        let (mut out, mut err) = (String::new(), Vec::new());
        while let Some(line) = run.next_line().await {
            match line {
                Line::Stdout(l) => {
                    out.push_str(&l);
                    out.push('\n');
                }
                Line::Stderr(l) => {
                    if let Some(cx) = cx {
                        cx.log(&l);
                    }
                    err.push(l);
                }
            }
        }
        let status = run.wait().await.map_err(|e| Error::internal(format!("{e:#}")))?;
        if !status.success() {
            let tail = err.iter().rev().take(5).rev().cloned().collect::<Vec<_>>().join(" / ");
            return Err(Error::internal(format!("data failed ({status}): {tail}")));
        }
        Ok(out)
    }

    /// Rebuilds a dataset's manifest with the `data` tool. `units`: the scores' units when known.
    async fn manifest(
        &self,
        tool: &Tool,
        dir: &Path,
        name: &str,
        source: &str,
        units: Option<&str>,
        cx: Option<&WorkContext>,
    ) -> Result<(), Error> {
        let mut args: Vec<std::ffi::OsString> = vec![
            "manifest".into(),
            dir.as_os_str().to_owned(),
            "--name".into(),
            name.into(),
            "--source".into(),
            source.into(),
            "--json".into(),
        ];
        if let Some(u) = units {
            args.extend(["--units".into(), u.into()]);
        }
        self.run_data(tool, args, cx).await.map(drop)
    }

    /// An idea's trial games as a dataset: every trial PGN converted to viriformat with `data convert`. The trials'
    /// scores are UCI centipawns, turned into Reckless's internal units as the trainer wants.
    pub async fn from_trials(&self, idea: IdeaId, name: Option<String>) -> Result<Dataset, Error> {
        let name = name.unwrap_or_else(|| format!("trials-{idea}"));
        if !training::valid_name(&name) {
            return Err(Error::bad_request(format!("{name:?} is not a dataset name")));
        }
        let store = &self.hub().store;
        let mut pgns = Vec::new();
        for t in store.trials(idea).await? {
            let Some((_, Some(path))) = store.trial_rounds(t.id).await? else { continue };
            let path = PathBuf::from(path);
            if path.is_dir() {
                for e in std::fs::read_dir(&path).map_err(io_error(&path))?.flatten() {
                    if e.path().extension().is_some_and(|x| x == "pgn") {
                        pgns.push(e.path());
                    }
                }
            } else if path.is_file() {
                pgns.push(path);
            }
        }
        if pgns.is_empty() {
            return Err(Error::not_found(format!("Idea {idea} has no trial games")));
        }
        pgns.sort();
        let dir = self.dir(&name);
        std::fs::create_dir_all(&dir).map_err(io_error(&dir))?;
        let tool = self
            .inner
            .trainer
            .data(&tokio_util::sync::CancellationToken::new(), &mut |_| {})
            .await
            .map_err(|e| Error::internal(format!("{e:#}")))?;
        let mut args: Vec<std::ffi::OsString> = vec!["convert".into()];
        args.extend(pgns.iter().map(|p| p.as_os_str().to_owned()));
        args.extend(["-o".into(), dir.join(format!("trials-{idea}.vf")).into_os_string()]);
        args.extend(["--units".into(), TRAINING_UNITS.into(), "--json".into()]);
        self.run_data(&tool, args, None).await?;
        self.manifest(&tool, &dir, &name, "trials", Some(TRAINING_UNITS), None).await?;
        self.register_dir(&dir, Some(DatasetSource::Trials { idea })).await
    }

    /// The viriformat files to train on: the datasets named, or every one in Reckless's internal units. Returns them
    /// and the first dataset.
    pub async fn training_files(&self, ids: &[DatasetId]) -> Result<(Vec<PathBuf>, Option<DatasetId>), Error> {
        let sets = if ids.is_empty() {
            self.list().await?.into_iter().filter(|d| d.score_units.as_deref() == Some(TRAINING_UNITS)).collect()
        } else {
            let mut v = Vec::new();
            for id in ids {
                v.push(self.get(*id).await?);
            }
            v
        };
        let mut files = Vec::new();
        for d in &sets {
            let dir = PathBuf::from(&d.path);
            files.extend(Manifest::read(&dir)?.viriformat_files(&dir).into_iter().filter(|f| f.is_file()));
        }
        if files.is_empty() {
            return Err(Error::conflict(
                "No training data: generate some (datagen) or import a shard; datasets must be viriformat in Reckless's \
                 internal units",
            ));
        }
        Ok((files, sets.first().map(|d| d.id)))
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Datagen
// ---------------------------------------------------------------------------------------------------------------------

/// What `data datagen --json` writes on stderr, one object per line.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
struct DatagenProgress {
    positions: u64,
    games: u64,
    bytes: u64,
    target_positions: u64,
    positions_per_second: f64,
    elapsed_seconds: f64,
}

struct DatagenExecutor {
    data: Datasets,
}

/// How a run of a tool came out.
enum RunEnd {
    Exited(std::process::ExitStatus),
    Paused,
    Stopped,
}

#[async_trait]
impl Executor for DatagenExecutor {
    fn kind(&self) -> WorkKind {
        WorkKind::Datagen
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let spec: DatagenRequest = cx.spec()?;
        let data = &self.data;
        let hub = data.hub().clone();
        let name = spec.name.clone().unwrap_or_else(|| "datagen".into());
        let dir = data.dir(&name);
        std::fs::create_dir_all(&dir).map_err(io_error(&dir))?;
        let mut log = |l: &str| cx.log(l);
        let tool = data.inner.trainer.data(&cx.cancel, &mut log).await.map_err(|e| {
            if cx.cancel.is_cancelled() { cx.stopped() } else { Error::internal(format!("the data tool: {e:#}")) }
        })?;

        // The engine: the datagen branch, with the champion's net when the champion has a trained one.
        let versions = &data.inner.versions;
        let mut engine =
            versions.resolve(&VersionRef::parse(spec.version.as_deref().unwrap_or(DATAGEN_BRANCH))).await?;
        if engine.net.is_none() {
            engine.net = versions.champion().await?.net;
        }
        let (build, binary) = versions.binary_of(&engine, &[]).await?;
        let _hold = versions.in_use().hold(build.id, format!("datagen:{}", cx.id));
        cx.log(format!("Engine: {} (build {}), into {}", engine.name, build.id, dir.display()));

        let total = hub.ledger.total();
        let want = spec.threads.unwrap_or((total / 2).max(1)).clamp(1, total);
        let source = DatasetSource::Datagen { settings: serde_json::to_value(&spec).unwrap_or_default() };
        let started = std::time::Instant::now();
        loop {
            if !cx.safe_point(None).await {
                return Err(cx.stopped());
            }
            let own = dir_bytes(&dir);
            let room =
                training::datagen_room(data.used(Some(&name)).await?, hub.config.data_cap_bytes, data.disk_free(), own);
            if room <= own {
                cx.log("The datasets' cap (or the disk's floor) is reached: no more data.");
                data.manifest(&tool, &dir, &name, "datagen", Some(TRAINING_UNITS), Some(&cx)).await?;
                let d = data.register_dir(&dir, Some(source)).await?;
                return Ok(serde_json::json!({ "dataset": d.id, "positions": d.positions, "finished": "disk-cap" }));
            }
            let Some(grant) = cx.cores(1, want).await else { return Err(cx.stopped()) };
            let mut args: Vec<std::ffi::OsString> = vec![
                "datagen".into(),
                "--json".into(),
                "--engine".into(),
                binary.as_os_str().to_owned(),
                "--out".into(),
                dir.as_os_str().to_owned(),
                "--threads".into(),
                grant.cores().to_string().into(),
                "--positions".into(),
                spec.positions.to_string().into(),
                "--max-size".into(),
                room.to_string().into(),
                "--units".into(),
                TRAINING_UNITS.into(),
            ];
            if let Some(n) = spec.soft_nodes {
                args.extend(["--soft-nodes".into(), n.to_string().into()]);
            }
            if let Some(o) = &spec.opening {
                args.extend(["--opening".into(), o.into()]);
            }
            cx.log(format!(
                "data datagen on {} threads, to {} positions, at most {}",
                grant.cores(),
                spec.positions,
                training::gb(room)
            ));
            let mut run = Running::start(&tool.command(args)).map_err(|e| Error::internal(format!("{e:#}")))?;
            let (mut report, mut tail) = (String::new(), Vec::<String>::new());
            let mut tick = tokio::time::interval(Duration::from_millis(250));
            let end = loop {
                tokio::select! {
                    line = run.next_line() => match line {
                        Some(Line::Stderr(l)) => match serde_json::from_str::<DatagenProgress>(&l) {
                            Ok(p) if l.trim_start().starts_with('{') => {
                                let target = p.target_positions.max(spec.positions);
                                let _ = cx.progress(Progress {
                                    done: p.positions,
                                    total: Some(target),
                                    unit: "positions".into(),
                                    eta_seconds: (p.positions_per_second > 0.0)
                                        .then(|| (target.saturating_sub(p.positions) as f64 / p.positions_per_second) as u64),
                                    text: Some(format!(
                                        "{} positions in {} games, {:.1} MB, {:.0}/s",
                                        p.positions,
                                        p.games,
                                        p.bytes as f64 / 1e6,
                                        p.positions_per_second
                                    )),
                                }).await;
                            }
                            _ => {
                                cx.log(&l);
                                tail.push(l);
                            }
                        },
                        Some(Line::Stdout(l)) => {
                            report.push_str(&l);
                            report.push('\n');
                        }
                        None => break RunEnd::Exited(run.wait().await.map_err(|e| Error::internal(format!("{e:#}")))?),
                    },
                    _ = cx.cancel.cancelled() => {
                        run.kill().await;
                        break RunEnd::Stopped;
                    }
                    _ = tick.tick() => if cx.pause_asked() {
                        run.kill().await;
                        break RunEnd::Paused;
                    },
                }
            };
            drop(grant);
            match end {
                RunEnd::Exited(status) if status.success() => {
                    // The tool rebuilt the manifest.
                    let d = data.register_dir(&dir, Some(source)).await?;
                    let finished = serde_json::from_str::<serde_json::Value>(&report)
                        .ok()
                        .and_then(|r| r.get("finished").and_then(|f| f.as_str()).map(str::to_owned))
                        .unwrap_or_else(|| "target".into());
                    cx.log(format!(
                        "{}: {} positions in {} games, {}",
                        finished,
                        d.positions,
                        d.games,
                        training::gb(d.size_bytes)
                    ));
                    return Ok(serde_json::json!({
                        "dataset": d.id,
                        "positions": d.positions,
                        "games": d.games,
                        "size_bytes": d.size_bytes,
                        "finished": finished,
                        "seconds": started.elapsed().as_secs(),
                    }));
                }
                RunEnd::Exited(status) => {
                    let _ = data.manifest(&tool, &dir, &name, "datagen", Some(TRAINING_UNITS), Some(&cx)).await;
                    let _ = data.register_dir(&dir, Some(source.clone())).await;
                    let tail = tail.iter().rev().take(5).rev().cloned().collect::<Vec<_>>().join(" / ");
                    return Err(Error::internal(format!("data datagen failed ({status}): {tail}")));
                }
                RunEnd::Stopped | RunEnd::Paused => {
                    // Killed: the parts end in whole games after the tool's own resume; the manifest is ours to rebuild.
                    data.manifest(&tool, &dir, &name, "datagen", Some(TRAINING_UNITS), Some(&cx)).await?;
                    data.register_dir(&dir, Some(source.clone())).await?;
                    if matches!(end, RunEnd::Stopped) {
                        return Err(cx.stopped());
                    }
                    cx.log("Paused: datagen resumes from its files.");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------------------------------------------------

struct ImportExecutor {
    data: Datasets,
}

#[async_trait]
impl Executor for ImportExecutor {
    fn kind(&self) -> WorkKind {
        WorkKind::DatasetImport
    }

    async fn run(&self, cx: WorkContext) -> Result<serde_json::Value, Error> {
        let spec: ImportWork = cx.spec()?;
        let data = &self.data;
        let hub = data.hub().clone();
        // Checked again: other data may have arrived since it was queued.
        data.check_import_room(spec.expected_bytes, &spec.name).await?;
        let file = spec.url.rsplit('/').next().filter(|f| !f.is_empty()).unwrap_or("shard.binpack").to_owned();
        let file: String =
            file.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
        let downloads = hub.config.downloads_dir();
        std::fs::create_dir_all(&downloads).map_err(io_error(&downloads))?;
        let partial = downloads.join(format!("{}-{file}", cx.id));
        let result = self.download(&cx, &spec, &partial).await;
        let sha = match result {
            Ok(sha) => sha,
            Err(e) => {
                let _ = std::fs::remove_file(&partial);
                return Err(e);
            }
        };
        let outcome = self.convert(&cx, &spec, &partial, &file, &sha).await;
        let _ = std::fs::remove_file(&partial);
        outcome
    }
}

impl ImportExecutor {
    /// Downloads the shard to `path`, checking its size against what you confirmed; returns its SHA-256.
    async fn download(&self, cx: &WorkContext, spec: &ImportWork, path: &Path) -> Result<String, Error> {
        use futures::StreamExt;

        cx.log(format!("Downloading {} ({})", spec.url, training::gb(spec.expected_bytes)));
        let response = self
            .data
            .inner
            .http
            .get(&spec.url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| Error::upstream(format!("download {}: {e}", spec.url)))?;
        if let Some(len) = response.content_length()
            && len != spec.expected_bytes
        {
            return Err(Error::conflict(format!(
                "The server says {len} bytes, not the {} you confirmed: nothing was downloaded",
                spec.expected_bytes
            )));
        }
        let mut out = tokio::fs::File::create(path).await.map_err(io_error(path))?;
        let mut hasher = Sha256::new();
        let mut done: u64 = 0;
        let mut stream = response.bytes_stream();
        let started = std::time::Instant::now();
        loop {
            let chunk = tokio::select! {
                c = stream.next() => c,
                _ = cx.cancel.cancelled() => return Err(cx.stopped()),
            };
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(|e| Error::upstream(format!("download {}: {e}", spec.url)))?;
            done += chunk.len() as u64;
            if done > spec.expected_bytes {
                return Err(Error::conflict(format!(
                    "The download passed the {} bytes you confirmed: stopped",
                    spec.expected_bytes
                )));
            }
            hasher.update(&chunk);
            out.write_all(&chunk).await.map_err(io_error(path))?;
            let secs = started.elapsed().as_secs().max(1);
            let _ = cx
                .progress(Progress {
                    done,
                    total: Some(spec.expected_bytes),
                    unit: "bytes".into(),
                    eta_seconds: Progress::eta(done, spec.expected_bytes, secs),
                    text: Some(format!("{:.1} of {:.1} MB", done as f64 / 1e6, spec.expected_bytes as f64 / 1e6)),
                })
                .await;
        }
        out.flush().await.map_err(io_error(path))?;
        drop(out);
        if done != spec.expected_bytes {
            return Err(Error::conflict(format!("Downloaded {done} bytes, not the {} expected", spec.expected_bytes)));
        }
        let sha = hex(&hasher.finalize());
        if let Some(want) = &spec.sha256
            && *want != sha
        {
            return Err(Error::conflict(format!("The download's SHA-256 is {sha}, not {want}")));
        }
        cx.log(format!("Downloaded {done} bytes, SHA-256 {sha}"));
        Ok(sha)
    }

    /// Turns the download into viriformat in the dataset's directory, rebuilds the manifest and records it.
    async fn convert(
        &self,
        cx: &WorkContext,
        spec: &ImportWork,
        download: &Path,
        file: &str,
        sha: &str,
    ) -> Result<serde_json::Value, Error> {
        let data = &self.data;
        let dir = data.dir(&spec.name);
        std::fs::create_dir_all(&dir).map_err(io_error(&dir))?;
        let mut log = |l: &str| cx.log(l);
        let tool = data
            .inner
            .trainer
            .data(&cx.cancel, &mut log)
            .await
            .map_err(|e| Error::internal(format!("the data tool: {e:#}")))?;
        let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
        let target = dir.join(format!("{stem}.vf"));
        if file.ends_with(".vf") {
            std::fs::copy(download, &target).map_err(io_error(&target))?;
        } else {
            cx.log(format!("Converting {file} to viriformat"));
            let args: Vec<std::ffi::OsString> = vec![
                "convert".into(),
                download.as_os_str().to_owned(),
                "-o".into(),
                target.as_os_str().to_owned(),
                "--units".into(),
                TRAINING_UNITS.into(),
                "--json".into(),
            ];
            let report = data.run_data(&tool, args, Some(cx)).await?;
            cx.log(format!("Converted: {}", report.split_whitespace().collect::<Vec<_>>().join(" ")));
        }
        // PGN scores are UCI centipawns, which `convert --units` turns into Reckless's internal units. Other formats
        // keep their own scores (a Stockfish binpack's are Stockfish's), so their units are left unknown: training on
        // them would need another eval scale, and the trainer's default data leaves them out.
        let units = file.ends_with(".pgn").then_some(TRAINING_UNITS);
        data.manifest(&tool, &dir, &spec.name, &spec.url, units, Some(cx)).await?;
        let d = data.register_dir(&dir, Some(DatasetSource::Import { url: spec.url.clone() })).await?;
        Ok(serde_json::json!({
            "dataset": d.id,
            "positions": d.positions,
            "size_bytes": d.size_bytes,
            "sha256": sha,
            "file": target.display().to_string(),
        }))
    }
}
