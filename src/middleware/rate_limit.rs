use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{errors::AppError, models::auth::AuthenticatedUser};

const AUTH_LIMIT: u32 = 10;
const API_LIMIT: u32 = 60;
const WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
struct Window {
    count: u32,
    started_at: Instant,
}

/// Shared in-memory rate limit store.
/// Key is either an IP string (auth routes) or a user ID string (api routes).
#[derive(Debug, Clone)]
pub struct RateLimitStore {
    inner: Arc<Mutex<HashMap<String, Window>>>,
}

impl RateLimitStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn check_limit_status(&self, key: &str, limit: u32) -> bool {
        let mut map = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();

        match map.get_mut(key) {
            Some(window) => {
                if now.duration_since(window.started_at) >= WINDOW {
                    // Window expired — reset.
                    *window = Window {
                        count: 1,
                        started_at: now,
                    };
                    true
                } else if window.count >= limit {
                    false
                } else {
                    window.count += 1;
                    true
                }
            }
            None => {
                map.insert(
                    key.to_string(),
                    Window {
                        count: 1,
                        started_at: now,
                    },
                );
                true
            }
        }
    }
}

pub async fn auth_rate_limit(
    axum::extract::State(store): axum::extract::State<RateLimitStore>,
    req: Request,
    next: Next,
) -> Response {
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    if !store.check_limit_status(&ip, AUTH_LIMIT) {
        return AppError::TooManyRequests("Too many requests".to_string()).into_response();
    }

    next.run(req).await
}

pub async fn api_rate_limit(
    axum::extract::State(store): axum::extract::State<RateLimitStore>,
    req: Request,
    next: Next,
) -> Response {
    let user_id = req
        .extensions()
        .get::<AuthenticatedUser>()
        .map(|u| u.id.to_string())
        .unwrap_or_else(|| "anonymous".to_string());

    if !store.check_limit_status(&user_id, API_LIMIT) {
        return AppError::TooManyRequests("Too many requests".to_string()).into_response();
    }

    next.run(req).await
}
