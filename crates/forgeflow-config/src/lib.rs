use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgeFlowPaths {
    pub repo_root: PathBuf,
    pub workitems_dir: PathBuf,
    pub schemas_dir: PathBuf,
}

impl ForgeFlowPaths {
    pub fn discover(repo_root: impl Into<PathBuf>) -> Self {
        let repo_root = repo_root.into();
        Self {
            workitems_dir: repo_root.join("workitems"),
            schemas_dir: repo_root.join("schemas"),
            repo_root,
        }
    }
}
