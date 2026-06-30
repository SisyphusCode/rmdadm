use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};
use std::collections::HashMap;
use std::net::SocketAddr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub address: SocketAddr,
    pub is_active: bool,
}

pub struct ClusterManager {
    nodes: HashMap<String, NodeInfo>,
}

impl ClusterManager {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn join_cluster(&mut self, node: NodeInfo) {
        info!("Node {} joining cluster at {}", node.id, node.address);
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn leave_cluster(&mut self, node_id: &str) {
        info!("Node {} leaving cluster", node_id);
        self.nodes.remove(node_id);
    }

    pub fn sync_metadata(&self) {
        // Placeholder for synchronizing RAID metadata across nodes
        info!("Synchronizing metadata across {} cluster nodes", self.nodes.len());
    }
}
