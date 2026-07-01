//! Rate limiting middleware for API protection
//! Prevents abuse by limiting requests per IP address

use axum::{
    extract::{ConnectInfo, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Rate limit configuration
#[derive(Clone, Debug)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: u32,
    /// Time window duration
    pub window_duration: Duration,
    /// Whether to enable rate limiting
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: std::env::var("RMDADM_RATE_LIMIT_MAX")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            window_duration: Duration::from_secs(
                std::env::var("RMDADM_RATE_LIMIT_WINDOW")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60)
            ),
            enabled: std::env::var("RMDADM_DISABLE_RATE_LIMIT").is_err(),
        }
    }
}

/// Request tracking for an IP address
#[derive(Debug, Clone)]
struct RequestTracker {
    count: u32,
    window_start: Instant,
}

/// Rate limiter state
pub struct RateLimiter {
    config: RateLimitConfig,
    trackers: Arc<RwLock<HashMap<String, RequestTracker>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let limiter = Self {
            config,
            trackers: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Spawn cleanup task to remove old entries
        let trackers = limiter.trackers.clone();
        let window_duration = limiter.config.window_duration;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(window_duration * 2);
            loop {
                interval.tick().await;
                let mut trackers = trackers.write().await;
                let now = Instant::now();
                trackers.retain(|_, tracker| {
                    now.duration_since(tracker.window_start) < window_duration * 2
                });
                debug!("Rate limiter cleanup: {} active trackers", trackers.len());
            }
        });
        
        limiter
    }
    
    /// Check if request should be allowed
    pub async fn check_rate_limit(&self, ip: &str) -> Result<(), RateLimitError> {
        if !self.config.enabled {
            return Ok(());
        }
        
        let mut trackers = self.trackers.write().await;
        let now = Instant::now();
        
        let tracker = trackers.entry(ip.to_string()).or_insert_with(|| {
            RequestTracker {
                count: 0,
                window_start: now,
            }
        });
        
        // Reset window if expired
        if now.duration_since(tracker.window_start) >= self.config.window_duration {
            tracker.count = 0;
            tracker.window_start = now;
        }
        
        // Check limit
        if tracker.count >= self.config.max_requests {
            let retry_after = self.config.window_duration
                .saturating_sub(now.duration_since(tracker.window_start))
                .as_secs();
            
            warn!("Rate limit exceeded for IP: {} ({} requests)", ip, tracker.count);
            
            return Err(RateLimitError {
                retry_after,
                limit: self.config.max_requests,
            });
        }
        
        tracker.count += 1;
        debug!("Rate limit check for {}: {}/{}", ip, tracker.count, self.config.max_requests);
        
        Ok(())
    }
}

/// Rate limit error
#[derive(Debug)]
pub struct RateLimitError {
    pub retry_after: u64,
    pub limit: u32,
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "error": "Rate limit exceeded",
            "retry_after": self.retry_after,
            "limit": self.limit,
        });
        
        (
            StatusCode::TOO_MANY_REQUESTS,
            [
                ("X-RateLimit-Limit", self.limit.to_string()),
                ("X-RateLimit-Retry-After", self.retry_after.to_string()),
            ],
            axum::Json(body),
        )
            .into_response()
    }
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    limiter: Arc<RateLimiter>,
    request: Request,
    next: Next,
) -> Result<Response, RateLimitError> {
    let ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    
    limiter.check_rate_limit(&ip).await?;
    
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter() {
        let config = RateLimitConfig {
            max_requests: 5,
            window_duration: Duration::from_secs(1),
            enabled: true,
        };
        
        let limiter = RateLimiter::new(config);
        let ip = "127.0.0.1";
        
        // First 5 requests should succeed
        for _ in 0..5 {
            assert!(limiter.check_rate_limit(ip).await.is_ok());
        }
        
        // 6th request should fail
        assert!(limiter.check_rate_limit(ip).await.is_err());
        
        // Wait for window to reset
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // Should succeed again
        assert!(limiter.check_rate_limit(ip).await.is_ok());
    }
    
    #[tokio::test]
    async fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            max_requests: 1,
            window_duration: Duration::from_secs(1),
            enabled: false,
        };
        
        let limiter = RateLimiter::new(config);
        let ip = "127.0.0.1";
        
        // All requests should succeed when disabled
        for _ in 0..100 {
            assert!(limiter.check_rate_limit(ip).await.is_ok());
        }
    }
}
