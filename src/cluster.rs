use crate::error::MdError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tracing::info;

const DEFAULT_CLUSTER_DIR: &str = "/var/lib/rmdadm/cluster";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub address: SocketAddr,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterMetadata {
    pub node_id: String,
    pub generated_at: String,
    pub mdstat: String,
}

pub struct ClusterManager {
    nodes: HashMap<String, NodeInfo>,
    state_dir: PathBuf,
}

impl ClusterManager {
    pub fn new() -> Result<Self, MdError> {
        Self::with_state_dir(PathBuf::from(DEFAULT_CLUSTER_DIR))
    }

    pub fn with_state_dir(state_dir: PathBuf) -> Result<Self, MdError> {
        fs::create_dir_all(&state_dir).map_err(MdError::Io)?;
        let nodes = load_nodes(&state_dir)?;
        Ok(Self { nodes, state_dir })
    }

    pub fn join_cluster(&mut self, node: NodeInfo) -> Result<(), MdError> {
        info!("Node {} joining cluster at {}", node.id, node.address);
        self.nodes.insert(node.id.clone(), node);
        self.save_nodes()
    }

    pub fn leave_cluster(&mut self, node_id: &str) -> Result<(), MdError> {
        info!("Node {} leaving cluster", node_id);
        self.nodes.remove(node_id);
        self.save_nodes()
    }

    pub fn list_nodes(&self) -> Vec<NodeInfo> {
        let mut nodes: Vec<_> = self.nodes.values().cloned().collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        nodes
    }

    pub fn sync_metadata(&self) -> Result<ClusterMetadata, MdError> {
        info!("Synchronizing metadata across {} cluster nodes", self.nodes.len());
        let metadata = ClusterMetadata {
            node_id: hostname::get()
                .map(|host| host.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            generated_at: chrono::Utc::now().to_rfc3339(),
            mdstat: fs::read_to_string("/proc/mdstat").unwrap_or_default(),
        };

        let path = self.state_dir.join("metadata.json");
        let mut file = File::create(path).map_err(MdError::Io)?;
        serde_json::to_writer_pretty(&mut file, &metadata)?;
        file.write_all(b"\n").map_err(MdError::Io)?;
        Ok(metadata)
    }

    fn save_nodes(&self) -> Result<(), MdError> {
        let path = self.nodes_path();
        let mut file = File::create(path).map_err(MdError::Io)?;
        serde_json::to_writer_pretty(&mut file, &self.nodes)?;
        file.write_all(b"\n").map_err(MdError::Io)
    }

    fn nodes_path(&self) -> PathBuf {
        self.state_dir.join("nodes.json")
    }
}

fn load_nodes(state_dir: &Path) -> Result<HashMap<String, NodeInfo>, MdError> {
    let path = state_dir.join("nodes.json");
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let file = File::open(path).map_err(MdError::Io)?;
    serde_json::from_reader(file).map_err(MdError::Serialization)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_cluster_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = ClusterManager::with_state_dir(dir.path().to_path_buf()).unwrap();
        manager
            .join_cluster(NodeInfo {
                id: "node-a".to_string(),
                address: "127.0.0.1:8080".parse().unwrap(),
                is_active: true,
            })
            .unwrap();

        let loaded = ClusterManager::with_state_dir(dir.path().to_path_buf()).unwrap();
        assert_eq!(loaded.list_nodes().len(), 1);
        assert_eq!(loaded.list_nodes()[0].id, "node-a");
    }
}
