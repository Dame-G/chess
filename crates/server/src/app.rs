//! The app: one [`Hub`] and every service wired to it, as their crates document it.
//!
//! ```text
//! hub         = Hub::open(Config::from_env())
//! scheduler   = Scheduler::new(hub)
//! versions    = Versions::new(hub, scheduler)        → registers Build
//! ideas       = Ideas::new(versions)                  → registers Trial, Tune, Promote
//! harness     = Harness::new(ideas); diagnostics = Diagnostics::new(harness)
//! nets        = Nets::new(hub); datasets = Datasets::new(versions, trainer, probes) → registers Datagen, DatasetImport
//! training    = Training::new(ideas, datasets, nets, probes) → registers Train
//! eval        = Eval::new(hub, versions)
//! drills      = Drills::new(hub)
//! reviews     = Reviews::new(eval).with_drills(drills) → registers Review (reviews of your games make drills)
//! profiles    = Profiles::new(hub)
//! games       = Games::new(hub, versions, timing)     → recover(): games left live by the last run
//! connections = Connections::new(hub, games, lichess) → start(): your account and the bot, as they were
//! levels      = games.levels(), anchored on your Lichess rating
//! calibrate   = Calibrate::new(games)                 → registers Calibrate
//! import      = ImportExecutor::new(connections)      → registers Import
//! scheduler.start()                                   → requeues interrupted work, starts the loop
//! nets.ensure_shipped(versions); datasets.scan()      → the champion's shipped net and datasets already on disk
//! ```
//!
//! [`serve`] binds the address first (so a second app started by mistake stops before touching anything), then
//! wires, then serves until Ctrl-C (or SIGTERM), and shuts down: no new requests, live evaluation stopped, the
//! scheduler stopped so running work resumes at the next start. Games being played are recorded as they stood; local
//! ones are closed as aborted at the next start, Lichess ones taken up again by their connection.

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use dameless_analysis::{Drills, Eval, Profiles, Reviews};
use dameless_core::ConnectionKey;
use dameless_hub::{Config, Hub};
use dameless_play::{Calibrate, Connections, Games, ImportExecutor, Timing, your_anchor};
use dameless_work::{
    Datasets, Diagnostics, Harness, Ideas, Nets, Scheduler, Toolchain, TrainerBuild, Training, Versions,
};
use tokio_util::sync::CancellationToken;

/// What can differ between the real app and a test's.
#[derive(Default)]
pub struct Options {
    /// The engine toolchain: cargo, `reckless bench` and fastchess when `None`; a fake in tests.
    pub toolchain: Option<Arc<dyn Toolchain>>,
    /// The Lichess the connections talk to: the hub config's when `None`; a mock with short waits in tests.
    pub lichess: Option<dameless_lichess::Config>,
    /// The waits of games and Lichess connections: the real ones by default, shortened in tests.
    pub timing: Timing,
}

/// Every service, wired. Shared behind an `Arc`; each service is itself cheap to clone.
pub struct App {
    pub hub: Arc<Hub>,
    pub scheduler: Scheduler,
    pub versions: Versions,
    pub ideas: Ideas,
    pub harness: Harness,
    pub diagnostics: Diagnostics,
    pub nets: Nets,
    pub datasets: Datasets,
    pub training: Training,
    pub eval: Eval,
    pub reviews: Reviews,
    pub drills: Drills,
    pub profiles: Profiles,
    pub games: Games,
    pub connections: Connections,
    /// Cancelled when the app shuts down: open event streams end with it.
    pub shutdown: CancellationToken,
}

impl App {
    /// Opens the hub on `config` and wires every service. Games left live by the last run are recovered, the Lichess
    /// connections pick up their stored tokens, and the scheduler starts.
    pub async fn start(config: Config, options: Options) -> Result<Arc<Self>> {
        let lichess = options.lichess.unwrap_or_else(|| lichess_config(&config));
        let hub = Hub::open(config)?;
        let scheduler = Scheduler::new(hub.clone());
        let versions = match options.toolchain {
            Some(toolchain) => Versions::with_toolchain(hub.clone(), scheduler.clone(), toolchain),
            None => Versions::new(hub.clone(), scheduler.clone()),
        };
        scheduler.register(versions.executor());
        let ideas = Ideas::new(versions.clone());
        ideas.register();
        let harness = Harness::new(ideas.clone());
        let diagnostics = Diagnostics::new(harness.clone());
        let probes = harness.probes().clone();
        let trainer = Arc::new(TrainerBuild::new(TrainerBuild::default_source(), hub.config.trainer_tools_dir()));
        let nets = Nets::new(hub.clone());
        let datasets = Datasets::new(versions.clone(), trainer, probes.clone());
        datasets.register();
        let training = Training::new(ideas.clone(), datasets.clone(), nets.clone(), probes);
        training.register();

        let binaries = Arc::new(versions.clone());
        let eval = Eval::new(hub.clone(), binaries.clone());
        let drills = Drills::new(hub.clone());
        let reviews = Reviews::new(eval.clone()).with_drills(drills.clone());
        scheduler.register(Arc::new(reviews.clone()));
        let profiles = Profiles::new(hub.clone());

        let games = Games::new(hub.clone(), binaries, options.timing);
        let recovered = games.recover().await.context("recover the games left live")?;
        if recovered > 0 {
            tracing::info!("{recovered} games were left live by the last run");
        }
        let connections = Connections::new(hub.clone(), games.clone(), lichess);
        connections.start().await.context("start the Lichess connections")?;
        let anchored = connections.clone();
        games.levels().set_anchor(Arc::new(move || your_anchor(&anchored.status(ConnectionKey::You))));
        scheduler.register(Arc::new(Calibrate::new(games.clone())));
        scheduler.register(Arc::new(ImportExecutor::new(connections.clone())));
        scheduler.start().await.context("start the scheduler")?;
        if let Err(e) = nets.ensure_shipped(&versions).await {
            tracing::warn!("register the champion's net: {e}");
        }
        if let Err(e) = datasets.scan().await {
            tracing::warn!("scan the datasets: {e}");
        }

        Ok(Arc::new(Self {
            hub,
            scheduler,
            versions,
            ideas,
            harness,
            diagnostics,
            nets,
            datasets,
            training,
            eval,
            reviews,
            drills,
            profiles,
            games,
            connections,
            shutdown: CancellationToken::new(),
        }))
    }

    /// The whole HTTP app: the API, the OAuth callback and the web UI.
    pub fn router(self: &Arc<Self>) -> axum::Router {
        crate::api::router(self.clone())
    }

    /// Stops what must not outlive the app: event streams end, live evaluation stops, and the scheduler stops its
    /// running work without recording it as stopped, so it resumes at the next start.
    pub async fn stop(&self) {
        self.shutdown.cancel();
        self.eval.stop_all().await;
        // Warm engines would otherwise outlive the app (and hold up a runtime shutting down).
        self.eval.pool().kill_idle();
        self.scheduler.shutdown();
    }
}

/// The Lichess the hub's config names (`LICHESS_URL` and its explorer and tablebase).
pub fn lichess_config(config: &Config) -> dameless_lichess::Config {
    let mut c = dameless_lichess::Config::new(&config.lichess.url);
    c.explorer = config.lichess.explorer.clone();
    c.tablebase = config.lichess.tablebase.clone();
    c
}

/// `dameless serve`: binds, wires the app and serves until Ctrl-C or SIGTERM.
pub async fn serve(config: Config, options: Options) -> Result<()> {
    let listen = config.listen;
    // Bound first: a second app started by mistake stops here, before it requeues the first one's work or brings a
    // second bot online.
    let listener = tokio::net::TcpListener::bind(listen).await.with_context(|| format!("listen on {listen}"))?;
    let data = config.data_dir.clone();
    let reckless = config.reckless_dir.clone();
    let lichess = config.lichess.url.clone();
    let app = App::start(config, options).await?;
    let router = app.router().layer(tower_http::trace::TraceLayer::new_for_http());

    tracing::info!("Dameless on http://{listen} (data in {})", data.display());
    tracing::info!("engine checkout: {}; Lichess: {lichess}", reckless.display());
    let stopping = app.shutdown.clone();
    let served = axum::serve(listener, router).with_graceful_shutdown(async move {
        shutdown_signal().await;
        tracing::info!("stopping");
        stopping.cancel();
    });
    let result = served.await;
    // The signal handler now swallows a second Ctrl-C, so a shutdown that hangs still ends.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(10));
        std::process::exit(1);
    });
    app.stop().await;
    result.context("serve")
}

/// Ctrl-C, or SIGTERM on Unix (systemd, `kill`).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            },
            Err(_) => drop(tokio::signal::ctrl_c().await),
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
