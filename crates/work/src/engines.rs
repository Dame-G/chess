//! The engines list as a service: the built-ins (installed or not) and the engines you added, which one analyses, and
//! turning an engine's id into something a seat or a review can start.
//!
//! `play` and `analysis` ask through [`EngineSource`]: the server hands them [`Engines`], and tests a [`FixedEngines`]
//! that points at a fake engine.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chess_core::{
    Error,
    engines::{BUILTINS, DEFAULT_ANALYSIS, Engine, EngineKind, EnginesOverview, builtin, slug},
    events::Payload,
    ids::valid_engine_id,
};
use chess_engine::{proc, uci};
use chess_hub::Hub;
use chess_store::{EngineRecord, engines::ANALYSIS_KEY, now};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// How long an engine may take to answer `uci` when it is added or installed.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// An engine ready to start: the binary, its arguments and its own UCI options.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineBinary {
    pub engine: Engine,
    pub path: PathBuf,
    pub args: Vec<String>,
    /// Its own options, set before the app's (`Threads`, `Hash`, `MultiPV`).
    pub options: Vec<(String, String)>,
}

/// Engines by id, for the services that start them.
#[async_trait]
pub trait EngineSource: Send + Sync + 'static {
    /// The engine with this id, ready to start; `None` is the analysis engine. Not found is a 404, a built-in that is
    /// not installed a 409.
    async fn ready(&self, id: Option<&str>) -> Result<EngineBinary, Error>;
}

#[async_trait]
impl<T: EngineSource + ?Sized> EngineSource for Arc<T> {
    async fn ready(&self, id: Option<&str>) -> Result<EngineBinary, Error> {
        (**self).ready(id).await
    }
}

/// `POST /api/engines`: an engine you add by path.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct NewEngineRequest {
    /// As shown: `Reckless dev`.
    pub name: String,
    /// The binary.
    pub path: String,
    /// Its id (a slug); made from the name when left out.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// UCI options set after the handshake.
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    /// Nodes it searches a move whatever the clock (Maia-like engines); none plays on the clock.
    #[serde(default)]
    pub nodes: Option<u64>,
    /// The rating it plays at by design, when it has one.
    #[serde(default)]
    pub rating: Option<u32>,
}

/// The engines list. Cheap to clone.
#[derive(Clone)]
pub struct Engines {
    hub: Arc<Hub>,
}

impl Engines {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }

    pub fn hub(&self) -> &Arc<Hub> {
        &self.hub
    }

    /// The analysis engine's id: the one chosen, else Stockfish.
    pub async fn analysis_id(&self) -> Result<String, Error> {
        Ok(self.hub.store.meta::<String>(ANALYSIS_KEY).await?.unwrap_or_else(|| DEFAULT_ANALYSIS.to_owned()))
    }

    /// Every engine: the built-ins first (installed or not), then yours by name.
    pub async fn overview(&self) -> Result<EnginesOverview, Error> {
        let analysis = self.analysis_id().await?;
        let stored = self.hub.store.engines().await?;
        let mut engines: Vec<Engine> = BUILTINS
            .iter()
            .map(|b| match stored.iter().find(|r| r.id == b.id) {
                Some(r) => engine(r, &analysis),
                None => Engine {
                    id: b.id.to_owned(),
                    name: b.name.to_owned(),
                    kind: EngineKind::Builtin,
                    path: None,
                    args: Vec::new(),
                    options: BTreeMap::new(),
                    id_name: None,
                    nodes: b.nodes(),
                    rating: b.rating(),
                    installed: false,
                    analysis: b.id == analysis,
                    levels: false,
                    created_at: None,
                },
            })
            .collect();
        engines.extend(stored.iter().filter(|r| builtin(&r.id).is_none()).map(|r| engine(r, &analysis)));
        Ok(EnginesOverview { analysis, engines })
    }

    /// One engine by id (404 when there is none, not even a built-in to install).
    pub async fn get(&self, id: &str) -> Result<Engine, Error> {
        self.overview()
            .await?
            .engines
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| Error::not_found(format!("No engine {id}: see `chess engines`")))
    }

    /// Adds an engine by path. It must answer UCI's handshake: what it calls itself is kept.
    pub async fn add(&self, req: NewEngineRequest) -> Result<Engine, Error> {
        let name = req.name.trim().to_owned();
        if name.is_empty() {
            return Err(Error::bad_request("an engine needs a name"));
        }
        let id = req.id.as_deref().map(str::trim).map(str::to_owned).unwrap_or_else(|| slug(&name));
        if !valid_engine_id(&id) {
            return Err(Error::bad_request(format!(
                "{id:?} is not an engine id: lower-case letters, digits and -, as in my-engine"
            )));
        }
        if id == "you" || builtin(&id).is_some() || self.hub.store.engine(&id).await?.is_some() {
            return Err(Error::conflict(format!("There is already an engine {id}")));
        }
        let path = PathBuf::from(req.path.trim());
        if !path.is_file() {
            return Err(Error::bad_request(format!("{} is not a file", path.display())));
        }
        let id_name = probe(&path, &req.args).await?;
        let mut record = EngineRecord::new(&id, &name, EngineKind::Custom, path.display().to_string());
        record.args = req.args;
        record.options = req.options;
        record.id_name = Some(id_name);
        record.nodes = req.nodes.filter(|n| *n > 0);
        record.rating = req.rating;
        self.hub.store.put_engine(record).await?;
        self.changed().await;
        self.get(&id).await
    }

    /// Registers an installed built-in (setup does), asking it what it calls itself.
    pub async fn install(&self, mut record: EngineRecord) -> Result<Engine, Error> {
        let path = PathBuf::from(&record.path);
        let args: Vec<String> = record.args.clone();
        record.id_name = Some(probe(&path, &args).await?);
        if let Some(old) = self.hub.store.engine(&record.id).await? {
            record.created_at = old.created_at;
        }
        let id = record.id.clone();
        self.hub.store.put_engine(record).await?;
        self.changed().await;
        self.get(&id).await
    }

    /// Removes an engine from the list (a built-in's files stay in `tools/`, and setup registers it again). The
    /// analysis engine falls back to Stockfish when it was this one.
    pub async fn remove(&self, id: &str) -> Result<EnginesOverview, Error> {
        if !self.hub.store.delete_engine(id).await? {
            return Err(Error::not_found(format!("No engine {id} to remove")));
        }
        if self.analysis_id().await? == id {
            self.hub.store.delete_meta(ANALYSIS_KEY).await?;
        }
        self.changed().await;
        self.overview().await
    }

    /// Makes an installed engine the analysis engine. One with a fixed budget (Maia) cannot analyse.
    pub async fn set_analysis(&self, id: &str) -> Result<EnginesOverview, Error> {
        let engine = self.get(id).await?;
        if engine.nodes.is_some() {
            return Err(Error::bad_request(format!(
                "{} plays a fixed {} node(s) a move: it cannot analyse",
                engine.name,
                engine.nodes.unwrap_or(1)
            )));
        }
        if !engine.installed {
            return Err(not_installed(&engine));
        }
        self.hub.store.put_meta(ANALYSIS_KEY, &engine.id).await?;
        self.changed().await;
        self.overview().await
    }

    async fn changed(&self) {
        if let Ok(overview) = self.overview().await {
            self.hub.bus.emit(Payload::EnginesChanged(Box::new(overview)));
        }
    }
}

#[async_trait]
impl EngineSource for Engines {
    async fn ready(&self, id: Option<&str>) -> Result<EngineBinary, Error> {
        let id = match id {
            Some(id) => id.to_owned(),
            None => self.analysis_id().await?,
        };
        let engine = self.get(&id).await?;
        if !engine.installed {
            return Err(not_installed(&engine));
        }
        let path = PathBuf::from(engine.path.clone().unwrap_or_default());
        let args = engine.args.clone();
        let options = engine.options.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        Ok(EngineBinary { engine, path, args, options })
    }
}

fn not_installed(engine: &Engine) -> Error {
    match engine.kind {
        EngineKind::Builtin => Error::conflict(format!("{} isn't installed yet: download it in Settings › Engines (or run `chess setup`)", engine.name)),
        EngineKind::Custom => Error::conflict(format!(
            "{}'s binary is gone ({}): add it again",
            engine.name,
            engine.path.as_deref().unwrap_or("no path")
        )),
    }
}

/// An engine as the API shows it, from its record.
fn engine(r: &EngineRecord, analysis: &str) -> Engine {
    let installed = Path::new(&r.path).is_file();
    let is_analysis = r.id == analysis;
    Engine {
        id: r.id.clone(),
        name: r.name.clone(),
        kind: r.kind,
        path: Some(r.path.clone()),
        args: r.args.clone(),
        options: r.options.clone(),
        id_name: r.id_name.clone(),
        nodes: r.nodes,
        rating: r.rating,
        installed,
        analysis: is_analysis,
        levels: is_analysis && r.nodes.is_none() && installed,
        created_at: Some(r.created_at.clone()),
    }
}

/// Starts an engine, waits for its handshake and returns what it calls itself (`id name`).
pub async fn probe(path: &Path, args: &[String]) -> Result<String, Error> {
    let command = proc::Command::new(path).args(args);
    let engine = uci::Engine::start_command(&command, PROBE_TIMEOUT)
        .await
        .map_err(|e| Error::bad_request(format!("{} is not a UCI engine: {e:#}", path.display())))?;
    let name = engine.name.trim().to_owned();
    engine.quit().await;
    Ok(if name.is_empty() {
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        name
    })
}

/// Engines for tests and for running without the app's list: each id points at a binary, with arguments; the first
/// one added is the analysis engine.
#[derive(Clone, Default)]
pub struct FixedEngines {
    engines: Vec<EngineBinary>,
}

impl FixedEngines {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an engine that plays on the clock (or analyses, when it is the first).
    pub fn with(self, id: &str, path: impl Into<PathBuf>, args: &[&str]) -> Self {
        self.with_engine(id, path, args, None, None)
    }

    /// Adds an engine with a fixed node budget and a rating, as Maia is.
    pub fn with_engine(
        mut self,
        id: &str,
        path: impl Into<PathBuf>,
        args: &[&str],
        nodes: Option<u64>,
        rating: Option<u32>,
    ) -> Self {
        let analysis = self.engines.is_empty();
        let path = path.into();
        let name = id.split('-').map(capitalise).collect::<Vec<_>>().join(" ");
        let engine = Engine {
            id: id.to_owned(),
            name: name.clone(),
            kind: EngineKind::Custom,
            path: Some(path.display().to_string()),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            options: BTreeMap::new(),
            id_name: Some(format!("{name} 1.0")),
            nodes,
            rating,
            installed: true,
            analysis,
            levels: analysis && nodes.is_none(),
            created_at: Some(now()),
        };
        self.engines.push(EngineBinary { args: engine.args.clone(), engine, path, options: Vec::new() });
        self
    }

    /// The engines as a shared [`EngineSource`].
    pub fn shared(self) -> Arc<dyn EngineSource> {
        Arc::new(self)
    }
}

fn capitalise(word: &str) -> String {
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

#[async_trait]
impl EngineSource for FixedEngines {
    async fn ready(&self, id: Option<&str>) -> Result<EngineBinary, Error> {
        match id {
            None => self.engines.first().cloned().ok_or_else(|| Error::conflict("No analysis engine")),
            Some(id) => self
                .engines
                .iter()
                .find(|e| e.engine.id == id)
                .cloned()
                .ok_or_else(|| Error::not_found(format!("No engine {id}"))),
        }
    }
}
