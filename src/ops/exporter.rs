use crate::error::MdError;
use axum::{routing::get, Router};
use std::fs;

async fn metrics_handler() -> String {
    let mut metrics = String::new();
    metrics.push_str("# HELP md_array_state The state of the MD array (1=active/clean, 0=inactive/degraded)\n");
    metrics.push_str("# TYPE md_array_state gauge\n");
    
    // Scan sysfs for MD arrays to generate metrics dynamically
    if let Ok(entries) = fs::read_dir("/sys/block") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("md") {
                let sys = crate::sysfs::MdSysfs::new(&name_str);
                let state_val = match sys.get_array_state() {
                    Ok(crate::sysfs::ArrayState::Active) | Ok(crate::sysfs::ArrayState::Clean) => 1,
                    _ => 0,
                };
                metrics.push_str(&format!("md_array_state{{device=\"{}\"}} {}\n", name_str, state_val));
            }
        }
    }
    
    metrics
}

pub async fn run() -> Result<(), MdError> {
    println!("Starting Prometheus exporter on 0.0.0.0:9090...");
    let app = Router::new().route("/metrics", get(metrics_handler));
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:9090")
        .await
        .map_err(MdError::Io)?;
        
    axum::serve(listener, app)
        .await
        .map_err(MdError::Io)?;
        
    Ok(())
}
