//! The HTTP side of the runner. Every route under `/api` is in the OpenAPI document, which is the one schema the CLI,
//! the web client's types and any later MCP endpoint are built from. Everything else is the embedded UI.

mod analysis;
mod builds;
mod error;
mod events;
mod experiments;
mod guard;
mod jobs;
mod lichess;
mod matches;
mod status;
mod tunes;
mod versions;
mod web;

use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method, header},
    middleware,
};
use tower_http::{cors::CorsLayer, set_header::SetResponseHeaderLayer};
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    config::{Config, HOSTED_ORIGIN},
    events::Events,
    jobs::Queue,
    store::Store,
};

pub use error::ApiError;

pub struct AppState {
    pub config: Config,
    pub store: Store,
    pub events: Events,
    pub machine: status::Machine,
    pub queue: Queue,
    pub live: crate::analysis::Live,
    pub matches: crate::matches::Matches,
    pub lichess: crate::lichess::Bot,
}

impl AppState {
    pub fn new(config: Config, store: Store, brains: crate::lichess::Brains) -> Self {
        Self {
            lichess: crate::lichess::Bot::new(&config.lichess_url, brains),
            config,
            store,
            events: Events::new(),
            machine: status::Machine::detect(),
            queue: Queue::new(),
            live: crate::analysis::Live::new(),
            matches: crate::matches::Matches::new(),
        }
    }
}

#[derive(OpenApi)]
#[openapi(info(
    title = "chess runner",
    description = "Builds, tests and analyses engine variants. Every command of the `chess` CLI maps onto one of these routes."
))]
struct ApiDoc;

fn api() -> (Router<Arc<AppState>>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(status::status))
        .routes(routes!(events::events))
        .routes(routes!(builds::list, builds::create))
        .routes(routes!(builds::get, versions::delete_build))
        .routes(routes!(versions::prune))
        .routes(routes!(builds::branches))
        .routes(routes!(jobs::list))
        .routes(routes!(jobs::get, jobs::cancel))
        .routes(routes!(jobs::log))
        .routes(routes!(analysis::analyse))
        .routes(routes!(analysis::start, analysis::stop))
        .routes(routes!(matches::list, matches::create))
        .routes(routes!(matches::get, matches::stop))
        .routes(routes!(matches::pgn))
        .routes(routes!(matches::play_move))
        .routes(routes!(matches::takeback))
        .routes(routes!(matches::offer_draw))
        .routes(routes!(lichess::status))
        .routes(routes!(lichess::set_token, lichess::clear_token))
        .routes(routes!(lichess::settings, lichess::put_settings))
        .routes(routes!(lichess::start))
        .routes(routes!(lichess::stop))
        .routes(routes!(lichess::upgrade))
        .routes(routes!(lichess::challenge))
        .routes(routes!(lichess::bots))
        .routes(routes!(lichess::seek, lichess::stop_seek))
        .routes(routes!(experiments::list, experiments::create))
        .routes(routes!(experiments::batch))
        .routes(routes!(experiments::get, experiments::cancel))
        .routes(routes!(experiments::test))
        .routes(routes!(experiments::games))
        .routes(routes!(experiments::game))
        .routes(routes!(tunes::list, tunes::create))
        .routes(routes!(tunes::get, tunes::cancel))
        .routes(routes!(tunes::apply))
        .routes(routes!(tunes::resume))
        .routes(routes!(tunes::declared))
        .routes(routes!(versions::list))
        .routes(routes!(versions::hide))
        .routes(routes!(versions::delete))
        .split_for_parts()
}

pub fn openapi() -> utoipa::openapi::OpenApi {
    api().1
}

pub async fn serve(config: Config) -> Result<()> {
    let listen = config.listen;
    // Bound first: a second runner started by mistake stops here, before it marks the first one's work as stopped or
    // brings a second Lichess bot online.
    let listener = tokio::net::TcpListener::bind(listen).await.with_context(|| format!("listen on {listen}"))?;
    let store = Store::open(&config.data.join("runner.db"))?;
    let state = Arc::new(AppState::new(config, store, crate::lichess::engines()));
    crate::matches::recover(&state).await?;
    tokio::spawn(crate::jobs::worker(state.clone()));
    // Back online if it was when the runner stopped; games still running on Lichess resume in their records.
    crate::lichess::resume(&state).await?;

    let (api, _) = api();
    let api = api.layer(middleware::from_fn_with_state(state.clone(), guard::guard));
    // Requests are logged at debug level (`RUST_LOG=debug`); a 500 is logged as an error.
    let app = protect(Router::new().merge(api).fallback(web::web))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state.clone());

    // Clients following the stream see restarts, and drop state that only lived in the old process.
    state.events.emit("runner.started", serde_json::json!({ "version": env!("CARGO_PKG_VERSION") }));

    tracing::info!("runner on http://{listen} (data in {})", state.config.data.display());
    tracing::info!("reckless checkout: {}", state.config.reckless.display());
    // Returning ends the runtime, which drops every task and with them every child process tree (see `proc`). Open
    // event streams never end, so this does not wait for connections to close.
    tokio::select! {
        served = axum::serve(listener, app) => served?,
        () = shutdown_signal() => {
            tracing::info!("stopping");
            // The signal handler now swallows a second Ctrl-C, so a shutdown that hangs still ends.
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(10));
                std::process::exit(1);
            });
        }
    }
    Ok(())
}

/// What every response carries, around the routes and the UI:
///
/// - CORS for the hosted UI only. `Authorization` is named: a `*` in `Access-Control-Allow-Headers` never covers it,
///   so browsers refuse the token's preflight. Preflights are cached for an hour (Chrome keeps them at most two).
/// - `Access-Control-Allow-Private-Network`, which older Chromes ask of a public page calling a loopback address.
/// - No framing (the UI on its own origin needs no token, so a page that framed it could click its buttons), no MIME
///   sniffing and no referrer.
fn protect<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    let fixed = |name, value| SetResponseHeaderLayer::if_not_present(name, HeaderValue::from_static(value));
    // The last layer is the outermost: CORS answers a preflight itself, and the headers go on that answer too.
    router
        .layer(
            CorsLayer::new()
                .allow_origin(HeaderValue::from_static(HOSTED_ORIGIN))
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, HeaderName::from_static("last-event-id")])
                .max_age(Duration::from_secs(3600))
                .allow_private_network(true),
        )
        .layer(fixed(header::X_FRAME_OPTIONS, "DENY"))
        .layer(fixed(header::CONTENT_SECURITY_POLICY, "frame-ancestors 'none'"))
        .layer(fixed(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .layer(fixed(header::REFERRER_POLICY, "no-referrer"))
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

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn the_hosted_ui_may_send_its_token() {
        let app = protect(Router::new().route("/api/status", get(|| async { "ok" })));
        let preflight = Request::options("/api/status")
            .header(header::ORIGIN, HOSTED_ORIGIN)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
            .header("access-control-request-private-network", "true")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(preflight).await.unwrap();
        let h = |name: &str| res.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
        assert_eq!(h("access-control-allow-origin"), HOSTED_ORIGIN);
        assert!(h("access-control-allow-headers").contains("authorization"), "{}", h("access-control-allow-headers"));
        assert_eq!(h("access-control-allow-private-network"), "true");
        assert_eq!(h("x-frame-options"), "DENY");
    }
}
