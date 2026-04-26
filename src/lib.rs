use dashmap::DashMap;

use crate::middleware::rate_limit::RateLimitStore;

pub mod auth;
pub mod client;
pub mod countries;
pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod parser;
pub mod repo;
pub mod seeder;
pub mod utils;

#[derive(Clone, Debug)]
pub struct RequestId(pub String);

#[derive(Clone, Debug)]
pub struct AppState {
    pub client: crate::client::ReqwestClient,
    pub profile_repo: crate::repo::profile::ProfileRepo,
    pub user_repo: crate::repo::user::UserRepo,
    pub refresh_token_repo: crate::repo::refresh_token::RefreshTokenRepo,
    pub oauth_states: std::sync::Arc<DashMap<String, (String, String)>>,
    pub auth_rate_limit: RateLimitStore,
    pub api_rate_limit: RateLimitStore,
}

pub fn create_app(state: AppState) -> axum::Router {
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let auth_rate_store = state.auth_rate_limit.clone();
    let api_rate_store = state.api_rate_limit.clone();

    let auth_router = axum::Router::new()
        .route(
            "/auth/github",
            axum::routing::get(handlers::auth::github_init),
        )
        .route(
            "/auth/github/callback",
            axum::routing::get(handlers::auth::github_callback),
        )
        .route(
            "/auth/refresh",
            axum::routing::post(handlers::auth::refresh),
        )
        .route("/auth/logout", axum::routing::post(handlers::auth::logout))
        .layer(axum::middleware::from_fn_with_state(
            auth_rate_store,
            middleware::rate_limit::auth_rate_limit,
        ));

    let api_router = axum::Router::new()
        .route(
            "/api/profiles",
            axum::routing::get(handlers::profile::list_profiles)
                .post(handlers::profile::create_profile),
        )
        .route(
            "/api/profiles/search",
            axum::routing::get(handlers::profile::search_profiles),
        )
        .route(
            "/api/profiles/export",
            axum::routing::get(handlers::profile::export_profiles_to_csv),
        )
        .route(
            "/api/profiles/{id}",
            axum::routing::get(handlers::profile::get_profile)
                .delete(handlers::profile::delete_profile),
        )
        .layer(axum::middleware::from_fn_with_state(
            api_rate_store,
            middleware::rate_limit::api_rate_limit,
        ))
        .layer(axum::middleware::from_fn(middleware::auth::require_auth))
        .layer(axum::middleware::from_fn(
            middleware::api_version::require_api_version,
        ));

    axum::Router::new()
        .merge(auth_router)
        .merge(api_router)
        .layer(
            tower_http::trace::TraceLayer::new_for_http().make_span_with(
                |request: &axum::http::Request<_>| {
                    let request_id = request
                        .extensions()
                        .get::<RequestId>()
                        .map(|id| id.0.clone())
                        .unwrap_or_else(|| "unknown".to_string());

                    tracing::info_span!(
                        "request",
                        method = %request.method(),
                        uri = %request.uri(),
                        version = ?request.version(),
                        request_id = %request_id,
                    )
                },
            ),
        )
        .layer(axum::middleware::from_fn(
            middleware::request_id::request_id,
        ))
        .layer(cors)
        .with_state(state)
}
