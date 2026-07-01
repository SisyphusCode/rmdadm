use crate::error::MdError;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tracing::info;

const DEFAULT_STATE_DIR: &str = "/var/lib/rmdadm/migrations";
const COPY_BUFFER_SIZE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MigrationStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationState {
    pub id: String,
    pub source_array: PathBuf,
    pub target_array: PathBuf,
    pub bytes_total: u64,
    pub bytes_copied: u64,
    pub status: MigrationStatus,
    pub error: Option<String>,
}

pub struct MigrationJob {
    pub source_array: PathBuf,
    pub target_array: PathBuf,
    state_dir: PathBuf,
}

impl MigrationJob {
    pub fn new(source: PathBuf, target: PathBuf) -> Self {
        Self::with_state_dir(source, target, PathBuf::from(DEFAULT_STATE_DIR))
    }

    pub fn with_state_dir(source: PathBuf, target: PathBuf, state_dir: PathBuf) -> Self {
        Self {
            source_array: source,
            target_array: target,
            state_dir,
        }
    }

    pub fn start_migration(&self) -> Result<MigrationState, MdError> {
        info!(
            "Starting array migration from {} to {}",
            self.source_array.display(),
            self.target_array.display()
        );

        self.copy_with_resume(0)
    }

    pub fn pause_migration(&self) -> Result<MigrationState, MdError> {
        info!("Pausing array migration");
        let mut state = self.load_state()?;
        if state.status == MigrationStatus::Running {
            state.status = MigrationStatus::Paused;
            self.save_state(&state)?;
        }
        Ok(state)
    }

    pub fn resume_migration(&self) -> Result<MigrationState, MdError> {
        info!("Resuming array migration");
        let state = self.load_state()?;
        self.copy_with_resume(state.bytes_copied)
    }

    pub fn status(&self) -> Result<MigrationState, MdError> {
        self.load_state()
    }

    fn copy_with_resume(&self, start_offset: u64) -> Result<MigrationState, MdError> {
        validate_device_exists(&self.source_array)?;
        validate_device_exists(&self.target_array)?;
        fs::create_dir_all(&self.state_dir).map_err(MdError::Io)?;

        let source_size = device_size(&self.source_array)?;
        let target_size = device_size(&self.target_array)?;
        if target_size < source_size {
            return Err(MdError::DeviceTooSmall(
                self.target_array.clone(),
                target_size,
                source_size,
            ));
        }

        let mut state = MigrationState {
            id: self.id(),
            source_array: self.source_array.clone(),
            target_array: self.target_array.clone(),
            bytes_total: source_size,
            bytes_copied: start_offset,
            status: MigrationStatus::Running,
            error: None,
        };
        self.save_state(&state)?;

        let mut source = OpenOptions::new().read(true).open(&self.source_array).map_err(MdError::Io)?;
        let mut target = OpenOptions::new().read(true).write(true).open(&self.target_array).map_err(MdError::Io)?;
        source.seek(SeekFrom::Start(start_offset)).map_err(MdError::Io)?;
        target.seek(SeekFrom::Start(start_offset)).map_err(MdError::Io)?;

        let mut buffer = vec![0u8; COPY_BUFFER_SIZE];
        loop {
            if self
                .load_state()
                .map(|state| state.status == MigrationStatus::Paused)
                .unwrap_or(false)
            {
                state.status = MigrationStatus::Paused;
                self.save_state(&state)?;
                return Ok(state);
            }

            let read = source.read(&mut buffer).map_err(MdError::Io)?;
            if read == 0 {
                break;
            }

            target.write_all(&buffer[..read]).map_err(MdError::Io)?;
            state.bytes_copied += read as u64;
            self.save_state(&state)?;
        }

        target.sync_all().map_err(MdError::Io)?;
        state.status = MigrationStatus::Completed;
        state.bytes_copied = state.bytes_total;
        self.save_state(&state)?;
        Ok(state)
    }

    fn id(&self) -> String {
        format!(
            "{}-to-{}",
            sanitize_path(&self.source_array),
            sanitize_path(&self.target_array)
        )
    }

    fn state_path(&self) -> PathBuf {
        self.state_dir.join(format!("{}.json", self.id()))
    }

    fn save_state(&self, state: &MigrationState) -> Result<(), MdError> {
        fs::create_dir_all(&self.state_dir).map_err(MdError::Io)?;
        let mut file = File::create(self.state_path()).map_err(MdError::Io)?;
        serde_json::to_writer_pretty(&mut file, state)?;
        file.write_all(b"\n").map_err(MdError::Io)
    }

    fn load_state(&self) -> Result<MigrationState, MdError> {
        let file = File::open(self.state_path()).map_err(MdError::Io)?;
        serde_json::from_reader(file).map_err(MdError::Serialization)
    }
}

fn validate_device_exists(path: &Path) -> Result<(), MdError> {
    if path.exists() {
        Ok(())
    } else {
        Err(MdError::DeviceNotFound(path.display().to_string()))
    }
}

fn device_size(path: &Path) -> Result<u64, MdError> {
    let output = std::process::Command::new("blockdev")
        .arg("--getsize64")
        .arg(path)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let size = String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse::<u64>()
                .map_err(|e| MdError::InvalidMetadata(format!("Failed to parse device size: {}", e)))?;
            Ok(size)
        }
        _ => Ok(fs::metadata(path).map_err(MdError::Io)?.len()),
    }
}

fn sanitize_path(path: &Path) -> String {
    path.display()
        .to_string()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_regular_files_with_state() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.img");
        let target = dir.path().join("target.img");
        fs::write(&source, b"rmdadm migration data").unwrap();
        fs::write(&target, vec![0u8; 64]).unwrap();

        let job = MigrationJob::with_state_dir(source.clone(), target.clone(), dir.path().join("state"));
        let state = job.start_migration().unwrap();

        assert_eq!(state.status, MigrationStatus::Completed);
        assert_eq!(&fs::read(&target).unwrap()[..21], b"rmdadm migration data");
        assert_eq!(job.status().unwrap().status, MigrationStatus::Completed);
    }
}
