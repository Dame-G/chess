//! The HTTP side of the app. Every route under `/api` (but the Lichess login's callback) is in the OpenAPI document,
//! which is the one schema the CLI, the web client's types and later the MCP endpoint are built from. Everything else
//! is the web UI.

pub mod analysis;
pub mod connections;
pub mod engines;
pub mod error;
pub mod events;
pub mod games;
pub mod guard;
pub mod player_side;
pub mod players;
pub mod status;
pub mod web;
pub mod work;

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    http::{HeaderName, HeaderValue, Method, header},
    middleware,
};
use tower_http::{cors::CorsLayer, set_header::SetResponseHeaderLayer};
use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::App;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Dameless",
        description = "Play, review and analyze chess, with any UCI engine. Every command of the `dameless` CLI is \
                       one of these routes. Every request but `/api/health` carries `Authorization: Bearer <token>`."
    ),
    modifiers(&Bearer),
    security(("token" = [])),
    components(schemas(events::Event, dameless_core::error::ErrorBody))
)]
struct ApiDoc;

struct Bearer;

impl Modify for Bearer {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "token",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
    }
}

/// The routes behind the guard, and the open ones, each with their part of the document.
fn routes() -> (OpenApiRouter<Arc<App>>, OpenApiRouter<Arc<App>>) {
    let open = OpenApiRouter::new().routes(routes!(status::health));
    let guarded = OpenApiRouter::new()
        .routes(routes!(status::status))
        .routes(routes!(status::token))
        .routes(routes!(events::events))
        // Games
        .routes(routes!(games::list_games, games::start_game))
        .routes(routes!(games::import_pgn))
        .routes(routes!(games::get_game))
        .routes(routes!(games::get_game_pgn))
        .routes(routes!(games::act_in_game))
        .routes(routes!(games::stop_game))
        .routes(routes!(games::get_review))
        // Analysis
        .routes(routes!(analysis::start_eval, analysis::list_evals))
        .routes(routes!(analysis::stop_eval))
        .routes(routes!(analysis::explorer))
        .routes(routes!(analysis::tablebase))
        .routes(routes!(analysis::review_games))
        // Players
        .routes(routes!(players::top_players))
        .routes(routes!(players::get_player))
        .routes(routes!(players::get_profile))
        .routes(routes!(players::compare_players))
        .routes(routes!(players::import_player))
        // Engines
        .routes(routes!(engines::list_engines, engines::add_engine))
        .routes(routes!(engines::remove_engine))
        .routes(routes!(engines::set_analysis_engine))
        .routes(routes!(engines::setup_plan, engines::setup))
        .routes(routes!(engines::diagnostics))
        // The player side
        .routes(routes!(player_side::list_levels))
        .routes(routes!(player_side::plan_levels))
        .routes(routes!(player_side::calibrate_levels))
        .routes(routes!(player_side::list_drills, player_side::make_drills))
        .routes(routes!(player_side::drills_today))
        .routes(routes!(player_side::hint_drill))
        .routes(routes!(player_side::answer_drill))
        .routes(routes!(player_side::delete_drill))
        .routes(routes!(player_side::drills_trend))
        .routes(routes!(player_side::get_repertoire))
        .routes(routes!(player_side::get_prep))
        .routes(routes!(player_side::get_progress))
        // Work
        .routes(routes!(work::list_work))
        .routes(routes!(work::get_work))
        .routes(routes!(work::stop_work))
        .routes(routes!(work::pause_work))
        .routes(routes!(work::resume_work))
        .routes(routes!(work::work_log))
        // Connections
        .routes(routes!(connections::list_connections))
        .routes(routes!(connections::login))
        .routes(routes!(connections::set_token))
        .routes(routes!(connections::logout))
        .routes(routes!(connections::seek, connections::cancel_seek))
        .routes(routes!(connections::challenge))
        .routes(routes!(connections::accept_challenge))
        .routes(routes!(connections::decline_challenge))
        .routes(routes!(connections::cancel_challenge));
    (open, guarded)
}

/// The OpenAPI document of every route.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let (open, guarded) = routes();
    let (_, mut doc) = OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(guarded).split_for_parts();
    let (_, open_doc) = open.split_for_parts();
    doc.merge(open_doc);
    doc
}

/// The whole app over HTTP.
pub fn router(app: Arc<App>) -> Router {
    let (open, guarded) = routes();
    let (guarded, _) = guarded.split_for_parts();
    let (open, _) = open.split_for_parts();
    let api = guarded.layer(middleware::from_fn_with_state(app.clone(), guard::guard));
    let router = Router::new()
        .merge(api)
        .merge(open)
        .merge(connections::callback_router())
        .merge(crate::mcp::router(app.clone()))
        .fallback(web::web)
        .layer(middleware::from_fn_with_state(app.clone(), guard::host))
        .with_state(app);
    protect(router)
}

/// What every response carries, around the routes and the UI:
///
/// - CORS for the hosted UI only. `Authorization` is named: a `*` in `Access-Control-Allow-Headers` never covers it,
///   so browsers refuse the token's preflight. Preflights are cached for an hour.
/// - `Access-Control-Allow-Private-Network`, which Chrome asks of a public page calling a loopback address.
/// - No framing, no MIME sniffing and no referrer.
pub fn protect<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    let fixed = |name, value| SetResponseHeaderLayer::if_not_present(name, HeaderValue::from_static(value));
    // The last layer is the outermost: CORS answers a preflight itself, and the headers go on that answer too.
    router
        .layer(
            CorsLayer::new()
                .allow_origin(HeaderValue::from_static(dameless_hub::config::HOSTED_ORIGIN))
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

/// A JSON body that may be left out (empty, or `null`): its default then.
pub(crate) fn optional_body<T: serde::de::DeserializeOwned + Default>(body: &[u8]) -> error::ApiResult<T> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice::<Option<T>>(body)
        .map(Option::unwrap_or_default)
        .map_err(|e| error::ApiError(dameless_core::Error::bad_request(format!("the body does not read: {e}"))))
}
