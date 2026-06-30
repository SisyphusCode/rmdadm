//! Authentication and authorization module
//! Provides JWT-based authentication for API endpoints

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use utoipa::ToSchema;

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,      // Subject (username)
    pub exp: usize,       // Expiration time
    pub iat: usize,       // Issued at
    pub role: UserRole,   // User role
}

/// User roles for authorization
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,    // Full access
    Operator, // Can manage arrays but not create/delete
    ReadOnly, // Can only view status
}

/// Authentication configuration
#[derive(Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub token_expiry_hours: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: std::env::var("RMDADM_JWT_SECRET")
                .unwrap_or_else(|_| "change-me-in-production".to_string()),
            token_expiry_hours: 24,
        }
    }
}

/// Shared authentication state
pub type AuthState = Arc<RwLock<AuthConfig>>;

/// Login request
#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Username
    #[schema(example = "admin")]
    pub username: String,
    /// Password
    #[schema(example = "changeme")]
    pub password: String,
}

/// Login response
#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    /// JWT token for authentication
    pub token: String,
    /// Token expiration time in seconds
    pub expires_in: u64,
}

/// API key for simple authentication
#[derive(Debug, Clone)]
pub struct ApiKey {
    pub key: String,
    pub role: UserRole,
}

/// Check if API key authentication is enabled
pub fn is_api_key_enabled() -> bool {
    std::env::var("RMDADM_API_KEY").is_ok()
}

/// Validate API key from environment
pub fn validate_api_key(key: &str) -> Option<UserRole> {
    if let Ok(valid_key) = std::env::var("RMDADM_API_KEY") {
        if key == valid_key {
            // Default to admin role for API key auth
            return Some(UserRole::Admin);
        }
    }
    None
}

/// Generate JWT token
pub fn generate_token(username: &str, role: UserRole, config: &AuthConfig) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now();
    let expiry = now + chrono::Duration::hours(config.token_expiry_hours as i64);
    
    let claims = Claims {
        sub: username.to_string(),
        exp: expiry.timestamp() as usize,
        iat: now.timestamp() as usize,
        role,
    };
    
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
}

/// Validate JWT token
pub fn validate_token(token: &str, config: &AuthConfig) -> Result<Claims, jsonwebtoken::errors::Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
}

/// Authentication middleware
pub async fn auth_middleware(
    State(auth_state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Check if authentication is disabled (for development)
    if std::env::var("RMDADM_DISABLE_AUTH").is_ok() {
        debug!("Authentication disabled via RMDADM_DISABLE_AUTH");
        return Ok(next.run(request).await);
    }
    
    // Try API key authentication first
    if is_api_key_enabled() {
        if let Some(api_key) = request
            .headers()
            .get("X-API-Key")
            .and_then(|v| v.to_str().ok())
        {
            if let Some(role) = validate_api_key(api_key) {
                debug!("API key authentication successful");
                request.extensions_mut().insert(role);
                return Ok(next.run(request).await);
            }
        }
    }
    
    // Try JWT authentication
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| {
            warn!("Missing or invalid Authorization header");
            StatusCode::UNAUTHORIZED
        })?;
    
    let config = auth_state.read().await;
    let claims = validate_token(token, &config).map_err(|e| {
        warn!("Token validation failed: {}", e);
        StatusCode::UNAUTHORIZED
    })?;
    
    debug!("JWT authentication successful for user: {}", claims.sub);
    
    // Add claims to request extensions for use in handlers
    request.extensions_mut().insert(claims);
    
    Ok(next.run(request).await)
}

/// Authorization middleware - checks user role
pub fn require_role(required_role: UserRole) -> impl Fn(Request, Next) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, StatusCode>> + Send>> + Clone {
    move |request: Request, next: Next| {
        let required = required_role.clone();
        Box::pin(async move {
            // Get claims from request extensions
            let claims = request
                .extensions()
                .get::<Claims>()
                .cloned();
            
            let role = if let Some(claims) = claims {
                claims.role
            } else if let Some(role) = request.extensions().get::<UserRole>() {
                role.clone()
            } else {
                warn!("No authentication claims found in request");
                return Err(StatusCode::UNAUTHORIZED);
            };
            
            // Check if user has required role
            match (&role, &required) {
                (UserRole::Admin, _) => Ok(next.run(request).await),
                (UserRole::Operator, UserRole::ReadOnly) => Ok(next.run(request).await),
                (UserRole::Operator, UserRole::Operator) => Ok(next.run(request).await),
                (UserRole::ReadOnly, UserRole::ReadOnly) => Ok(next.run(request).await),
                _ => {
                    warn!("User with role {:?} attempted to access {:?} endpoint", role, required);
                    Err(StatusCode::FORBIDDEN)
                }
            }
        })
    }
}

/// Login handler
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials")
    )
)]
pub async fn login(
    State(auth_state): State<AuthState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    debug!("Login attempt for user: {}", request.username);
    
    // In production, validate against a database or LDAP
    // For now, check against environment variables
    let valid_username = std::env::var("RMDADM_ADMIN_USER").unwrap_or_else(|_| "admin".to_string());
    let valid_password = std::env::var("RMDADM_ADMIN_PASSWORD").unwrap_or_else(|_| "changeme".to_string());
    
    // Verify credentials
    if request.username != valid_username {
        warn!("Login failed: invalid username");
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // Use bcrypt for password verification in production
    if request.password != valid_password {
        warn!("Login failed: invalid password");
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    // Generate token
    let config = auth_state.read().await;
    let token = generate_token(&request.username, UserRole::Admin, &config)
        .map_err(|e| {
            warn!("Failed to generate token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    
    debug!("Login successful for user: {}", request.username);
    
    Ok(Json(LoginResponse {
        token,
        expires_in: config.token_expiry_hours * 3600,
    }))
}

/// Refresh token handler
pub async fn refresh_token(
    State(auth_state): State<AuthState>,
    request: Request,
) -> Result<Json<LoginResponse>, StatusCode> {
    let claims = request
        .extensions()
        .get::<Claims>()
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    let config = auth_state.read().await;
    let token = generate_token(&claims.sub, claims.role.clone(), &config)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(LoginResponse {
        token,
        expires_in: config.token_expiry_hours * 3600,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_token() {
        let config = AuthConfig {
            jwt_secret: "test-secret".to_string(),
            token_expiry_hours: 1,
        };
        
        let token = generate_token("testuser", UserRole::Admin, &config).unwrap();
        let claims = validate_token(&token, &config).unwrap();
        
        assert_eq!(claims.sub, "testuser");
        assert_eq!(claims.role, UserRole::Admin);
    }
    
    #[test]
    fn test_invalid_token() {
        let config = AuthConfig {
            jwt_secret: "test-secret".to_string(),
            token_expiry_hours: 1,
        };
        
        let result = validate_token("invalid.token.here", &config);
        assert!(result.is_err());
    }
}
