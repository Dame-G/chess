//! The HTTP side of the runner. Every route under `/api` is in the OpenAPI document, which is the one schema the CLI,
//! the web client's types and any later MCP endpoint are built from. Everything else is the embedded UI.

mod analysis;
mod builds;
mod error;
mod events;
mod guard;
mod jobs;
mod status;
mod web;

use std::sync::Arc;

use anyhow::Result;
use axum::{Router, http::HeaderValue, middleware};
use tower_http::cors::CorsLayer;
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
        .routes(routes!(builds::get))
        .routes(routes!(jobs::list))
        .routes(routes!(jobs::get, jobs::cancel))
        .routes(routes!(jobs::log))
        .routes(routes!(analysis::analyse))
        .routes(routes!(analysis::start, analysis::stop))
        .split_for_parts()
}

pub fn openapi() -> utoipa::openapi::OpenApi {
    api().1
}

pub async fn serve(config: Config) -> Result<()> {
    let store = Store::open(&config.data.join("runner.db"))?;
    let listen = config.listen;
    let state = Arc::new(AppState {
        config,
        store,
        events: Events::new(),
        machine: status::Machine::detect(),
        queue: Queue::new(),
        live: crate::analysis::Live::new(),
    });
    tokio::spawn(crate::jobs::worker(state.clone()));

    let (api, _) = api();
    let api = api.layer(middleware::from_fn_with_state(state.clone(), guard::guard));
    let app = Router::new()
        .merge(api)
        .fallback(web::web)
        .layer(
            CorsLayer::new()
                .allow_origin(HeaderValue::from_static(HOSTED_ORIGIN))
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
                .allow_private_network(true),
        )
        .with_state(state.clone());

    // Clients following the stream see restarts, and drop state that only lived in the old process.
    state.events.emit("runner.started", serde_json::json!({ "version": env!("CARGO_PKG_VERSION") }));

    let listener = tokio::net::TcpListener::bind(listen).await?;
    tracing::info!("runner on http://{listen} (data in {})", state.config.data.display());
    tracing::info!("reckless checkout: {}", state.config.reckless.display());
    axum::serve(listener, app).await?;
    Ok(())
}
