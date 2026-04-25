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
}

pub fn create_app(state: AppState) -> axum::Router {
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    axum::Router::new()
        .route(
            "/api/profiles",
            axum::routing::get(handlers::list_profiles).post(handlers::create_profile),
        )
        .route(
            "/api/profiles/search",
            axum::routing::get(handlers::search_profiles),
        )
        .route(
            "/api/profiles/{id}",
            axum::routing::get(handlers::get_profile).delete(handlers::delete_profile),
        )
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
            crate::middleware::request_id::request_id,
        ))
        .layer(cors)
        .with_state(state)
}
