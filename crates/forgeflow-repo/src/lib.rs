use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoBoundary {
    pub supports_branches: bool,
    pub supports_pull_requests: bool,
}
