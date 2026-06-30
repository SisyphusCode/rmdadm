//! Logging and tracing configuration
//! Provides structured logging with multiple output formats

#![allow(dead_code)]

use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use std::path::Path;

/// Initialize logging with console and file output
pub fn init_logging(log_dir: Option<&Path>, _json_format: bool) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("rmdadm=info,warn"));
    
    let fmt_layer = fmt::layer()
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true);
    
    if let Some(dir) = log_dir {
        // Create rolling file appender
        let file_appender = RollingFileAppender::new(
            Rotation::DAILY,
            dir,
            "rmdadm.log"
        );
        
        let file_layer = fmt::layer()
            .with_writer(file_appender)
            .with_ansi(false)
            .with_span_events(FmtSpan::CLOSE);
        
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(file_layer)
            .try_init()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .try_init()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    }
    
    Ok(())
}

/// Initialize logging for daemon mode with file output only
pub fn init_daemon_logging(log_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("rmdadm=info,warn"));
    
    let file_appender = RollingFileAppender::new(
        Rotation::DAILY,
        log_dir,
        "rmdadm-daemon.log"
    );
    
    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_span_events(FmtSpan::CLOSE)
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true);
    
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .try_init()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    
    Ok(())
}

/// Initialize minimal logging for testing
#[cfg(test)]
pub fn init_test_logging() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_init_logging() {
        let dir = tempdir().unwrap();
        let _ = init_logging(Some(dir.path()), false);
    }
    
    #[test]
    fn test_init_daemon_logging() {
        let dir = tempdir().unwrap();
        let _ = init_daemon_logging(dir.path());
    }
}
