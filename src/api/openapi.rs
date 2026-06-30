//! OpenAPI documentation configuration
//! Provides Swagger UI and OpenAPI spec generation

use utoipa::OpenApi;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};

use super::models::*;
use super::auth::{LoginRequest, LoginResponse};

/// OpenAPI documentation structure
#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::list_arrays,
        super::handlers::get_array_detail,
        super::handlers::create_array,
        super::handlers::stop_array,
        super::handlers::manage_array,
        super::handlers::scrub_array,
        super::handlers::health_check,
        super::handlers::metrics,
        super::auth::login,
    ),
    components(
        schemas(
            ArrayInfo,
            ArrayListResponse,
            ArraySummary,
            CreateArrayRequest,
            ManageArrayRequest,
            OperationResponse,
            HealthResponse,
            ScrubRequest,
            ScrubResponse,
            LoginRequest,
            LoginResponse,
        )
    ),
    tags(
        (name = "arrays", description = "RAID array management endpoints"),
        (name = "auth", description = "Authentication endpoints"),
        (name = "health", description = "Health and monitoring endpoints"),
    ),
    info(
        title = "rmdadm API",
        version = "0.1.0",
        description = "Modern Rust implementation of mdadm with REST API",
        contact(
            name = "rmdadm",
            url = "https://github.com/yourusername/rmdadm"
        ),
        license(
            name = "MIT",
            url = "https://opensource.org/licenses/MIT"
        )
    ),
    servers(
        (url = "http://localhost:8080", description = "Local development server"),
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

/// Add security schemes to OpenAPI spec
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer
                    )
                ),
            );
            
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(
                    ApiKey::Header(
                        ApiKeyValue::new("X-API-Key")
                    )
                ),
            );
        }
    }
}
