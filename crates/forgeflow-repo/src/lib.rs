use forgeflow_domain::WorkItem;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// --- Repo operations trait ---

pub trait RepoOps {
    fn current_branch(&self) -> anyhow::Result<String>;
    fn list_branches(&self) -> anyhow::Result<Vec<String>>;
    fn create_branch(&self, name: &str) -> anyhow::Result<()>;
    fn switch_branch(&self, name: &str) -> anyhow::Result<()>;
    fn has_uncommitted_changes(&self) -> anyhow::Result<bool>;
    fn status_summary(&self) -> anyhow::Result<RepoStatus>;
}

// --- Git-based implementation ---

pub struct GitRepo {
    root: PathBuf,
}

impl GitRepo {
    pub fn open(root: &Path) -> anyhow::Result<Self> {
        let git_dir = root.join(".git");
        if !git_dir.exists() {
            anyhow::bail!("not a git repository: {}", root.display());
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    fn git(&self, args: &[&str]) -> anyhow::Result<String> {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git {}: {}", args.join(" "), stderr.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl RepoOps for GitRepo {
    fn current_branch(&self) -> anyhow::Result<String> {
        let branch = self.git(&["rev-parse", "--abbrev-ref", "HEAD"])?;
        Ok(branch)
    }

    fn list_branches(&self) -> anyhow::Result<Vec<String>> {
        let output = self.git(&["branch", "--list"])?;
        let branches: Vec<String> = output
            .lines()
            .map(|l| l.trim_start_matches("* ").trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        Ok(branches)
    }

    fn create_branch(&self, name: &str) -> anyhow::Result<()> {
        self.git(&["checkout", "-b", name])?;
        Ok(())
    }

    fn switch_branch(&self, name: &str) -> anyhow::Result<()> {
        self.git(&["checkout", name])?;
        Ok(())
    }

    fn has_uncommitted_changes(&self) -> anyhow::Result<bool> {
        let output = self.git(&["status", "--porcelain"])?;
        Ok(!output.is_empty())
    }

    fn status_summary(&self) -> anyhow::Result<RepoStatus> {
        let branch = self.current_branch()?;
        let dirty = self.has_uncommitted_changes()?;
        let branches = self.list_branches()?;

        Ok(RepoStatus {
            root: self.root.clone(),
            current_branch: branch,
            has_uncommitted_changes: dirty,
            branches,
        })
    }
}

// --- Repo status ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStatus {
    pub root: PathBuf,
    pub current_branch: String,
    pub has_uncommitted_changes: bool,
    pub branches: Vec<String>,
}

// --- Branch naming conventions ---

pub fn workitem_branch_name(workitem: &WorkItem) -> String {
    format!(
        "{}/{}",
        match workitem.r#type {
            forgeflow_domain::WorkItemType::Feature => "feat",
            forgeflow_domain::WorkItemType::Bugfix => "fix",
            forgeflow_domain::WorkItemType::Review => "review",
            forgeflow_domain::WorkItemType::Release => "release",
            forgeflow_domain::WorkItemType::Chore => "chore",
        },
        workitem.id
    )
}

// --- PR abstraction ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestSpec {
    pub workitem_id: String,
    pub title: String,
    pub body: String,
    pub head_branch: String,
    pub base_branch: String,
    pub labels: Vec<String>,
}

impl PullRequestSpec {
    pub fn from_workitem(workitem: &WorkItem, base_branch: &str) -> Self {
        let branch = workitem
            .linked_branch
            .clone()
            .unwrap_or_else(|| workitem_branch_name(workitem));

        let labels = vec![
            format!("{:?}", workitem.r#type),
            format!("priority-{:?}", workitem.priority).to_lowercase(),
        ];

        let body = format!(
            "## WorkItem: {}\n\n- Stage: {}\n- Artifacts: {}\n- Linked Issue: {}",
            workitem.id,
            workitem.stage,
            if workitem.artifacts.is_empty() {
                "none".to_string()
            } else {
                workitem.artifacts.join(", ")
            },
            workitem
                .linked_issue
                .as_deref()
                .unwrap_or("none"),
        );

        Self {
            workitem_id: workitem.id.clone(),
            title: format!("[{}] {}", workitem.id, workitem.title),
            body,
            head_branch: branch,
            base_branch: base_branch.to_string(),
            labels,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestResult {
    pub url: String,
    pub number: u64,
    pub head_branch: String,
}

// --- Issue abstraction ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueSpec {
    pub workitem_id: String,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
}

impl IssueSpec {
    pub fn from_workitem(workitem: &WorkItem) -> Self {
        let labels = vec![
            format!("{:?}", workitem.r#type),
            format!("priority-{:?}", workitem.priority).to_lowercase(),
        ];

        Self {
            workitem_id: workitem.id.clone(),
            title: format!("[{}] {}", workitem.id, workitem.title),
            body: format!("Automatically created from ForgeFlow WorkItem {}", workitem.id),
            labels,
        }
    }
}

// --- CLI integration helpers ---

pub fn create_branch_for_workitem(repo: &GitRepo, workitem: &WorkItem) -> anyhow::Result<String> {
    let branch_name = workitem_branch_name(workitem);
    repo.create_branch(&branch_name)?;
    Ok(branch_name)
}

pub fn repo_status_for_workitem(
    repo: &GitRepo,
    workitem: &WorkItem,
) -> anyhow::Result<WorkItemRepoStatus> {
    let status = repo.status_summary()?;
    let expected_branch = workitem_branch_name(workitem);

    Ok(WorkItemRepoStatus {
        repo_status: status,
        on_expected_branch: repo.current_branch()? == expected_branch,
        expected_branch,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemRepoStatus {
    pub repo_status: RepoStatus,
    pub expected_branch: String,
    pub on_expected_branch: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgeflow_domain::{Priority, WorkItemType};

    fn test_workitem(r#type: WorkItemType, id: &str) -> WorkItem {
        WorkItem {
            id: id.to_string(),
            title: "test".to_string(),
            r#type,
            priority: Priority::Medium,
            repo: "test".to_string(),
            stage: forgeflow_domain::WorkStage::Implement,
            owner: None,
            linked_issue: None,
            linked_branch: None,
            artifacts: vec![],
            checkpoints: vec![],
        }
    }

    #[test]
    fn test_branch_name_feature() {
        let wi = test_workitem(WorkItemType::Feature, "wi-001");
        assert_eq!(workitem_branch_name(&wi), "feat/wi-001");
    }

    #[test]
    fn test_branch_name_bugfix() {
        let wi = test_workitem(WorkItemType::Bugfix, "wi-002");
        assert_eq!(workitem_branch_name(&wi), "fix/wi-002");
    }

    #[test]
    fn test_branch_name_release() {
        let wi = test_workitem(WorkItemType::Release, "wi-003");
        assert_eq!(workitem_branch_name(&wi), "release/wi-003");
    }

    #[test]
    fn test_pr_spec_from_workitem() {
        let wi = test_workitem(WorkItemType::Feature, "wi-001");
        let spec = PullRequestSpec::from_workitem(&wi, "main");
        assert_eq!(spec.head_branch, "feat/wi-001");
        assert_eq!(spec.base_branch, "main");
        assert!(spec.title.contains("wi-001"));
        assert!(spec.labels.contains(&"Feature".to_string()));
    }

    #[test]
    fn test_pr_spec_uses_linked_branch() {
        let mut wi = test_workitem(WorkItemType::Feature, "wi-001");
        wi.linked_branch = Some("custom-branch".to_string());
        let spec = PullRequestSpec::from_workitem(&wi, "main");
        assert_eq!(spec.head_branch, "custom-branch");
    }

    #[test]
    fn test_issue_spec_from_workitem() {
        let wi = test_workitem(WorkItemType::Bugfix, "wi-004");
        let spec = IssueSpec::from_workitem(&wi);
        assert!(spec.title.contains("wi-004"));
        assert!(spec.labels.contains(&"Bugfix".to_string()));
    }
}
