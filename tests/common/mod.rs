//! Common test utilities and helpers

use axum::Router;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Create a test application instance with all routes configured
pub async fn create_test_app() -> Router {
    // Initialize authentication state
    let auth_state = Arc::new(RwLock::new(rmdadm::api::auth::AuthConfig::default()));
    
    // Initialize rate limiter with test-friendly settings
    let rate_limit_config = rmdadm::api::rate_limit::RateLimitConfig {
        max_requests: std::env::var("RMDADM_RATE_LIMIT_MAX")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100),
        window_duration: std::time::Duration::from_secs(
            std::env::var("RMDADM_RATE_LIMIT_WINDOW")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60)
        ),
        enabled: std::env::var("RMDADM_DISABLE_RATE_LIMIT").is_err(),
    };
    let rate_limiter = Arc::new(rmdadm::api::rate_limit::RateLimiter::new(rate_limit_config));
    
    Router::new()
        .merge(rmdadm::api::routes::auth_routes(auth_state.clone()))
        .merge(rmdadm::api::routes::array_routes(auth_state.clone()))
        .merge(rmdadm::api::routes::health_routes())
        .merge(rmdadm::api::routes::metrics_routes())
        .layer(axum::middleware::from_fn(move |addr, req, next| {
            let limiter = rate_limiter.clone();
            rmdadm::api::rate_limit::rate_limit_middleware(addr, limiter, req, next)
        }))
}
