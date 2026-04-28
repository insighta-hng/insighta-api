use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dashmap::DashMap;
use insighta_api::{
    AppState,
    client::ReqwestClient,
    config::AppConfig,
    create_app,
    errors::{AppError, Result},
    middleware::rate_limit::RateLimitStore,
    repo, seeder,
};
use mongodb::bson::doc;
use mongodb::options::{ClientOptions, ServerApi, ServerApiVersion};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    initialize_tracing()?;

    let config = AppConfig::from_env()?;
    let server_port = config.server_port;

    let reqwest_client = ReqwestClient::init()?;

    let mut client_options = ClientOptions::parse(&config.database_url)
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("Failed to parse MongoDB URI: {e}")))?;

    let server_api = ServerApi::builder().version(ServerApiVersion::V1).build();
    client_options.server_api = Some(server_api);

    let mongo_client = mongodb::Client::with_options(client_options).map_err(|e| {
        AppError::ServiceUnavailable(format!("Failed to initialize MongoDB client: {e}"))
    })?;

    mongo_client
        .database("admin")
        .run_command(doc! {"ping": 1})
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("Failed to ping MongoDB: {e}")))?;

    tracing::info!("Successfully connected to MongoDB Atlas");

    let db = mongo_client.database(&config.database_name);

    let profile_repo = repo::profile::ProfileRepo::new(&db);
    profile_repo.create_indexes().await?;

    let user_repo = repo::user::UserRepo::new(&db);
    user_repo.create_indexes().await?;

    let refresh_token_repo = repo::refresh_token::RefreshTokenRepo::new(&db);
    refresh_token_repo.create_indexes().await?;

    tokio::spawn(seeder::run(profile_repo.clone()));

    let state = AppState {
        config,
        client: reqwest_client,
        profile_repo,
        user_repo,
        refresh_token_repo,
        oauth_states: Arc::new(DashMap::new()),
        auth_rate_limit: RateLimitStore::new(),
        api_rate_limit: RateLimitStore::new(),
    };

    // Prune OAuth state entries that were never completed (e.g. user closed the browser).
    {
        let states = state.oauth_states.clone();
        tokio::spawn(async move {
            let ttl = Duration::from_secs(300); // 5 minutes
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let now = Instant::now();
                states.retain(|_, (_, _, created)| now.duration_since(*created) < ttl);
            }
        });
    }

    let app = create_app(state);

    let listener = TcpListener::bind(format!("0.0.0.0:{server_port}")).await?;

    tracing::info!("Server running on port {}", server_port);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("All connections drained. Shutting down.");

    Ok(())
}

fn initialize_tracing() -> Result<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,stage1=debug,tower_http=debug".into());

    Ok(tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .flatten_event(true)
                .with_current_span(true)
                .with_span_list(false)
                .with_file(false)
                .with_target(true)
                .with_line_number(false)
                .with_thread_ids(false)
                .with_thread_names(false),
        )
        .try_init()?)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install SIGINT handler");
    };

    #[cfg(unix)]
    let sigterm = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {tracing::info!("Received SIGINT, shutting down");},
        _ = sigterm => {tracing::info!("Received SIGTERM, shutting down");}
    }
}
