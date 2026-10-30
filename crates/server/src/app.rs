//! The app: one [`Hub`] and every service wired to it, as their crates document it.
//!
//! ```text
//! hub         = Hub::open(Config::from_env())
//! scheduler   = Scheduler::new(hub)
//! engines     = Engines::new(hub)                     (or the test's engine source)
//! setup       = Setup::new(engines)                   → registers Setup
//! diagnostics = Diagnostics::new(engines, SystemProbes)
//! eval        = Eval::new(hub, engines)
//! drills      = Drills::new(hub)
//! reviews     = Reviews::new(eval).with_drills(drills) → registers Review (reviews of your games make drills)
//! profiles    = Profiles::new(hub)
//! games       = Games::new(hub, engines, timing)      → recover(): games left live by the last run
//! connections = Connections::new(hub, games, lichess) → start(): your account, as it was
//! levels      = games.levels(), anchored on your Lichess rating
//! calibrate   = Calibrate::new(games)                 → registers Calibrate
//! import      = ImportExecutor::new(connections)      → registers Import
//! scheduler.start()                                   → requeues interrupted work, starts the loop
//! ```
//!
//! [`serve`] binds the address first (so a second app started by mistake stops before touching anything), then
//! wires, then serves until Ctrl-C (or SIGTERM), and shuts down: no new requests, live evaluation stopped, the
//! scheduler stopped so running work resumes at the next start. Games being played are recorded as they stood; local
//! ones are closed as aborted at the next start, Lichess ones taken up again by their connection.

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use chess_analysis::{Drills, Eval, Profiles, Reviews};
use chess_core::ConnectionKey;
use chess_hub::{Config, Hub};
use chess_play::{Calibrate, Connections, Games, ImportExecutor, Timing, your_anchor};
use chess_work::{Diagnostics, EngineSource, Engines, Scheduler, Setup, SystemProbes};
use tokio_util::sync::CancellationToken;

/// What can differ between the real app and a test's.
#[derive(Default)]
pub struct Options {
    /// Where engines come from: the engines list when `None`; fixed fake engines in tests.
    pub engines: Option<Arc<dyn EngineSource>>,
    /// The Lichess the connections talk to: the hub config's when `None`; a mock with short waits in tests.
    pub lichess: Option<chess_lichess::Config>,
    /// The waits of games and Lichess connections: the real ones by default, shortened in tests.
    pub timing: Timing,
}

/// Every service, wired. Shared behind an `Arc`; each service is itself cheap to clone.
pub struct App {
    pub hub: Arc<Hub>,
    pub scheduler: Scheduler,
    /// The engines list: what `/api/engines` shows and changes.
    pub engines: Engines,
    /// What play and analysis start engines from: the engines list, or a test's.
    pub engine_source: Arc<dyn EngineSource>,
    pub setup: Setup,
    pub diagnostics: Diagnostics,
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
    /// Opens the hub on `config` and wires every service. Games left live by the last run are recovered, your Lichess
    /// connection picks up its stored token, and the scheduler starts.
    pub async fn start(config: Config, options: Options) -> Result<Arc<Self>> {
        let lichess = options.lichess.unwrap_or_else(|| lichess_config(&config));
        let hub = Hub::open(config)?;
        let scheduler = Scheduler::new(hub.clone());
        let engines = Engines::new(hub.clone());
        let engine_source: Arc<dyn EngineSource> = match options.engines {
            Some(source) => source,
            None => Arc::new(engines.clone()),
        };
        let setup = Setup::new(engines.clone());
        scheduler.register(setup.executor());
        let diagnostics = Diagnostics::new(engines.clone(), Arc::new(SystemProbes));

        let eval = Eval::new(hub.clone(), engine_source.clone());
        let drills = Drills::new(hub.clone());
        let classifying = drills.clone();
        tokio::spawn(async move {
            match classifying.classify_pending().await {
                Ok(0) => {}
                Ok(n) => tracing::info!("found the themes of {n} drills"),
                Err(e) => tracing::warn!("find the drills' themes: {e}"),
            }
        });
        let reviews = Reviews::new(eval.clone()).with_drills(drills.clone());
        scheduler.register(Arc::new(reviews.clone()));
        let profiles = Profiles::new(hub.clone());

        let games = Games::new(hub.clone(), engine_source.clone(), options.timing);
        let recovered = games.recover().await.context("recover the games left live")?;
        if recovered > 0 {
            tracing::info!("{recovered} games were left live by the last run");
        }
        let connections = Connections::new(hub.clone(), games.clone(), lichess);
        connections.start().await.context("start the Lichess connection")?;
        let anchored = connections.clone();
        games.levels().set_anchor(Arc::new(move || your_anchor(&anchored.status(ConnectionKey::You))));
        scheduler.register(Arc::new(Calibrate::new(games.clone())));
        scheduler.register(Arc::new(ImportExecutor::new(connections.clone())));
        scheduler.start().await.context("start the scheduler")?;

        Ok(Arc::new(Self {
            hub,
            scheduler,
            engines,
            engine_source,
            setup,
            diagnostics,
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
pub fn lichess_config(config: &Config) -> chess_lichess::Config {
    let mut c = chess_lichess::Config::new(&config.lichess.url);
    c.explorer = config.lichess.explorer.clone();
    c.tablebase = config.lichess.tablebase.clone();
    c
}

/// `chess serve`: binds, wires the app and serves until Ctrl-C or SIGTERM.
pub async fn serve(config: Config, options: Options) -> Result<()> {
    let listen = config.listen;
    // Bound first: a second app started by mistake stops here, before it requeues the first one's work.
    let listener = tokio::net::TcpListener::bind(listen).await.with_context(|| format!("listen on {listen}"))?;
    let data = config.data_dir.clone();
    let lichess = config.lichess.url.clone();
    let app = App::start(config, options).await?;
    let router = app.router().layer(tower_http::trace::TraceLayer::new_for_http());

    tracing::info!("Chess on http://{listen} (data in {})", data.display());
    tracing::info!("Lichess: {lichess}");
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
