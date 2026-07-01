use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskHealthStatus {
    pub device_name: String,
    pub is_healthy: bool,
    pub predictive_failure: bool,
    pub read_error_rate: u64,
    pub write_error_rate: u64,
    pub temperature_celsius: Option<i64>,
    pub smart_available: bool,
    pub details: Vec<String>,
}

pub struct FailureDetector {
    threshold: u64,
}

impl FailureDetector {
    pub fn new(threshold: u64) -> Self {
        Self { threshold }
    }

    pub fn analyze_disk(&self, disk_path: &str) -> DiskHealthStatus {
        info!("Analyzing SMART data and heuristics for {}", disk_path);

        match Command::new("smartctl").arg("-a").arg(disk_path).output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}\n{}", stdout, stderr);
                parse_smartctl_output(disk_path, &combined, output.status.success(), self.threshold)
            }
            Err(e) => {
                warn!("smartctl unavailable for {}: {}", disk_path, e);
                DiskHealthStatus {
                    device_name: disk_path.to_string(),
                    is_healthy: true,
                    predictive_failure: false,
                    read_error_rate: 0,
                    write_error_rate: 0,
                    temperature_celsius: None,
                    smart_available: false,
                    details: vec![format!("smartctl unavailable: {}", e)],
                }
            }
        }
    }

    pub fn analyze_devices<I, P>(&self, devices: I) -> Vec<DiskHealthStatus>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        devices
            .into_iter()
            .map(|device| self.analyze_disk(&device.as_ref().to_string_lossy()))
            .collect()
    }
}

fn parse_smartctl_output(device: &str, output: &str, command_success: bool, threshold: u64) -> DiskHealthStatus {
    let lower = output.to_lowercase();
    let read_error_rate = parse_attribute(output, "Raw_Read_Error_Rate").unwrap_or(0);
    let write_error_rate = parse_attribute(output, "Write_Error_Rate")
        .or_else(|| parse_attribute(output, "Total_LBAs_Written"))
        .unwrap_or(0);
    let reallocated = parse_attribute(output, "Reallocated_Sector_Ct").unwrap_or(0);
    let pending = parse_attribute(output, "Current_Pending_Sector").unwrap_or(0);
    let offline_uncorrectable = parse_attribute(output, "Offline_Uncorrectable").unwrap_or(0);
    let temperature_celsius = parse_temperature(output);

    let health_failed = lower.contains("smart overall-health self-assessment test result: failed")
        || lower.contains("smart health status: failed")
        || lower.contains("predicted failure")
        || lower.contains("prefail");
    let predictive_failure = health_failed
        || read_error_rate > threshold
        || reallocated > 0
        || pending > 0
        || offline_uncorrectable > 0;

    let mut details = Vec::new();
    if !command_success {
        details.push("smartctl returned a non-zero status".to_string());
    }
    if health_failed {
        details.push("SMART health reports failure or pre-fail indicators".to_string());
    }
    if reallocated > 0 {
        details.push(format!("{} reallocated sectors", reallocated));
    }
    if pending > 0 {
        details.push(format!("{} pending sectors", pending));
    }
    if offline_uncorrectable > 0 {
        details.push(format!("{} offline uncorrectable sectors", offline_uncorrectable));
    }

    DiskHealthStatus {
        device_name: device.to_string(),
        is_healthy: !predictive_failure,
        predictive_failure,
        read_error_rate,
        write_error_rate,
        temperature_celsius,
        smart_available: true,
        details,
    }
}

fn parse_attribute(output: &str, name: &str) -> Option<u64> {
    output
        .lines()
        .find(|line| line.contains(name))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|raw| raw.parse::<u64>().ok())
}

fn parse_temperature(output: &str) -> Option<i64> {
    output
        .lines()
        .find(|line| line.contains("Temperature_Celsius") || line.contains("Airflow_Temperature_Cel"))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|raw| raw.parse::<i64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smartctl_failure_indicators() {
        let output = r#"
SMART overall-health self-assessment test result: PASSED
  1 Raw_Read_Error_Rate     0x000f   100   099   006    Pre-fail  Always       -       1
  5 Reallocated_Sector_Ct   0x0033   100   100   010    Pre-fail  Always       -       2
194 Temperature_Celsius     0x0022   067   050   000    Old_age   Always       -       33
197 Current_Pending_Sector  0x0012   100   100   000    Old_age   Always       -       1
"#;

        let status = parse_smartctl_output("/dev/sda", output, true, 100);

        assert!(status.predictive_failure);
        assert!(!status.is_healthy);
        assert_eq!(status.read_error_rate, 1);
        assert_eq!(status.temperature_celsius, Some(33));
    }
}
