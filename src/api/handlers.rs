//! API request handlers

use axum::{
    extract::{Path, State},
    Json,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, instrument};
use utoipa;

use super::{ApiError, models::*};
use crate::sysfs::MdSysfs;

/// Shared application state
#[derive(Clone, Debug)]
pub struct AppState {
    pub start_time: std::time::Instant,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            start_time: std::time::Instant::now(),
        }
    }
}

/// List all MD arrays
#[utoipa::path(
    get,
    path = "/api/v1/arrays",
    tag = "arrays",
    responses(
        (status = 200, description = "List of arrays", body = ArrayListResponse),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
#[instrument]
pub async fn list_arrays() -> Result<Json<ArrayListResponse>, ApiError> {
    info!("Listing all arrays");
    
    let mut arrays = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            
            if name_str.starts_with("md") {
                let sys = MdSysfs::new(&name_str);
                let state = sys.get_array_state()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                
                let health = match state.as_str() {
                    "active" | "clean" | "active-idle" => "healthy",
                    "degraded" | "recovering" => "degraded",
                    "failed" | "inactive" => "failed",
                    _ => "unknown",
                };
                
                arrays.push(ArraySummary {
                    name: name_str.to_string(),
                    device: format!("/dev/{}", name_str),
                    level: "unknown".to_string(), // Would need to read from sysfs
                    state,
                    health: health.to_string(),
                });
            }
        }
    }
    
    let total = arrays.len();
    info!("Found {} arrays", total);
    
    Ok(Json(ArrayListResponse { arrays, total }))
}

/// Get detailed information about a specific array
#[utoipa::path(
    get,
    path = "/api/v1/arrays/{name}",
    tag = "arrays",
    params(
        ("name" = String, Path, description = "Array name (e.g., md0)")
    ),
    responses(
        (status = 200, description = "Array details", body = ArrayInfo),
        (status = 404, description = "Array not found"),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
#[instrument]
pub async fn get_array_detail(
    Path(array_name): Path<String>,
) -> Result<Json<ArrayInfo>, ApiError> {
    info!("Getting details for array: {}", array_name);
    
    let device_path = std::path::PathBuf::from(format!("/dev/{}", array_name));
    
    if !device_path.exists() {
        return Err(ApiError {
            status: axum::http::StatusCode::NOT_FOUND,
            message: format!("Array {} not found", array_name),
        });
    }
    
    let sys = MdSysfs::new(&array_name);
    let state = sys.get_array_state()
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    
    // Read array info using ioctl
    use std::fs::File;
    use std::os::fd::AsRawFd;
    
    let file = File::open(&device_path)
        .map_err(|e| ApiError {
            status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("Failed to open device: {}", e),
        })?;
    
    let mut info = crate::ioctl::MduArrayInfo::default();
    
    unsafe {
        crate::ioctl::get_array_info(file.as_raw_fd(), &mut info as *mut _)
            .map_err(|e| ApiError {
                status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                message: format!("Failed to get array info: {}", e),
            })?;
    }
    
    let array_info = ArrayInfo {
        name: array_name.clone(),
        device: device_path.display().to_string(),
        level: format!("raid{}", info.level),
        state,
        size: info.size as u64,
        raid_disks: info.raid_disks,
        total_disks: info.nr_disks,
        active_disks: info.active_disks,
        working_disks: info.working_disks,
        failed_disks: info.failed_disks,
        spare_disks: info.spare_disks,
        uuid: "unknown".to_string(), // Would need to read from superblock
        chunk_size: info.chunk_size,
        layout: info.layout,
        created: None,
        updated: None,
    };
    
    Ok(Json(array_info))
}

/// Create a new array
#[utoipa::path(
    post,
    path = "/api/v1/arrays",
    tag = "arrays",
    request_body = CreateArrayRequest,
    responses(
        (status = 200, description = "Array created successfully", body = OperationResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - requires admin role")
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
#[instrument(skip(request))]
pub async fn create_array(
    Json(request): Json<CreateArrayRequest>,
) -> Result<Json<OperationResponse>, ApiError> {
    info!("Creating array: {}", request.device);
    
    let md_device = std::path::PathBuf::from(&request.device);
    let components: Vec<std::path::PathBuf> = request.components
        .iter()
        .map(std::path::PathBuf::from)
        .collect();
    
    let metadata = request.metadata.unwrap_or_else(|| "1.2".to_string());
    
    crate::ops::create::run(
        &md_device,
        request.level,
        request.raid_devices,
        &metadata,
        components,
        request.chunk_size,
        false, // Not a dry run
    )?;
    
    Ok(Json(OperationResponse {
        success: true,
        message: format!("Array {} created successfully", request.device),
        details: None,
    }))
}

/// Stop an array
#[utoipa::path(
    delete,
    path = "/api/v1/arrays/{name}",
    tag = "arrays",
    params(
        ("name" = String, Path, description = "Array name (e.g., md0)")
    ),
    responses(
        (status = 200, description = "Array stopped successfully", body = OperationResponse),
        (status = 404, description = "Array not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - requires admin role")
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
#[instrument]
pub async fn stop_array(
    Path(array_name): Path<String>,
) -> Result<Json<OperationResponse>, ApiError> {
    info!("Stopping array: {}", array_name);
    
    let device_path = std::path::PathBuf::from(format!("/dev/{}", array_name));
    
    crate::ops::manage::stop(&device_path, false, false)?;
    
    Ok(Json(OperationResponse {
        success: true,
        message: format!("Array {} stopped successfully", array_name),
        details: None,
    }))
}

/// Manage array (add/remove/fail disks)
#[utoipa::path(
    post,
    path = "/api/v1/arrays/{name}/manage",
    tag = "arrays",
    params(
        ("name" = String, Path, description = "Array name (e.g., md0)")
    ),
    request_body = ManageArrayRequest,
    responses(
        (status = 200, description = "Array managed successfully", body = OperationResponse),
        (status = 404, description = "Array not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - requires operator role")
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
#[instrument(skip(request))]
pub async fn manage_array(
    Path(array_name): Path<String>,
    Json(request): Json<ManageArrayRequest>,
) -> Result<Json<OperationResponse>, ApiError> {
    info!("Managing array: {}", array_name);
    
    let device_path = std::path::PathBuf::from(format!("/dev/{}", array_name));
    
    let add = request.add.map(|v| v.into_iter().map(std::path::PathBuf::from).collect());
    let remove = request.remove.map(|v| v.into_iter().map(std::path::PathBuf::from).collect());
    let fail = request.fail.map(|v| v.into_iter().map(std::path::PathBuf::from).collect());
    
    crate::ops::manage::manage(&device_path, add, remove, fail, false, false)?;
    
    Ok(Json(OperationResponse {
        success: true,
        message: format!("Array {} managed successfully", array_name),
        details: None,
    }))
}

/// Start a scrub operation
#[utoipa::path(
    post,
    path = "/api/v1/arrays/{name}/scrub",
    tag = "arrays",
    params(
        ("name" = String, Path, description = "Array name (e.g., md0)")
    ),
    request_body = ScrubRequest,
    responses(
        (status = 200, description = "Scrub started successfully", body = ScrubResponse),
        (status = 404, description = "Array not found"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden - requires operator role")
    ),
    security(
        ("bearer_auth" = []),
        ("api_key" = [])
    )
)]
#[instrument(skip(request))]
pub async fn scrub_array(
    Path(array_name): Path<String>,
    Json(request): Json<ScrubRequest>,
) -> Result<Json<ScrubResponse>, ApiError> {
    info!("Starting scrub for array: {} (repair: {})", array_name, request.repair);
    
    let sys = MdSysfs::new(&array_name);
    
    // Check if array exists and is active
    let _state = sys.get_array_state().map_err(|e| ApiError {
        status: axum::http::StatusCode::NOT_FOUND,
        message: format!("Array {} not found or inactive: {}", array_name, e),
    })?;
    
    // Start scrub
    sys.start_scrub(request.repair).map_err(|e| ApiError {
        status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("Failed to initiate scrub: {}", e),
    })?;
    
    // In a real system we'd calculate estimated duration based on sync_speed and remaining size
    Ok(Json(ScrubResponse {
        started: true,
        estimated_duration: None,
    }))
}

/// Health check endpoint
#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "health",
    responses(
        (status = 200, description = "Service health status", body = HealthResponse)
    )
)]
#[instrument]
pub async fn health_check(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<Json<HealthResponse>, ApiError> {
    let state = state.read().await;
    let uptime = state.start_time.elapsed().as_secs();
    
    let arrays_monitored = if let Ok(entries) = std::fs::read_dir("/sys/block") {
        entries.filter(|e| {
            e.as_ref()
                .ok()
                .and_then(|e| e.file_name().to_str().map(|s| s.starts_with("md")))
                .unwrap_or(false)
        }).count()
    } else {
        0
    };
    
    Ok(Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime,
        arrays_monitored,
    }))
}

/// Prometheus metrics endpoint
#[utoipa::path(
    get,
    path = "/api/v1/metrics",
    tag = "health",
    responses(
        (status = 200, description = "Prometheus metrics in text format", body = String, content_type = "text/plain")
    )
)]
#[instrument]
pub async fn metrics() -> Result<String, ApiError> {
    let mut metrics = String::new();
    metrics.push_str("# HELP md_array_state The state of the MD array (1=active/clean, 0=inactive/degraded)\n");
    metrics.push_str("# TYPE md_array_state gauge\n");
    
    if let Ok(entries) = std::fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("md") {
                let sys = MdSysfs::new(&name_str);
                let state_val = match sys.get_array_state() {
                    Ok(crate::sysfs::ArrayState::Active) | Ok(crate::sysfs::ArrayState::Clean) => 1,
                    _ => 0,
                };
                metrics.push_str(&format!("md_array_state{{device=\"{}\"}} {}\n", name_str, state_val));
            }
        }
    }
    
    Ok(metrics)
}
