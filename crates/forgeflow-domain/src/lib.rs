use forgeflow_core::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkItemType {
    Feature,
    Bugfix,
    Review,
    Release,
    Chore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkStage {
    Intake,
    Roundtable,
    Architecture,
    Implement,
    Test,
    Review,
    PR,
    Release,
}

impl std::fmt::Display for WorkStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Intake => "Intake",
            Self::Roundtable => "Roundtable",
            Self::Architecture => "Architecture",
            Self::Implement => "Implement",
            Self::Test => "Test",
            Self::Review => "Review",
            Self::PR => "PR",
            Self::Release => "Release",
        };
        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: String,
    pub title: String,
    pub r#type: WorkItemType,
    pub priority: Priority,
    pub repo: String,
    pub stage: WorkStage,
    pub owner: Option<String>,
    pub linked_issue: Option<String>,
    pub linked_branch: Option<String>,
    pub artifacts: Vec<String>,
    pub checkpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub workitem_id: String,
    pub stage: WorkStage,
    pub summary: String,
    pub artifacts: Vec<String>,
    pub blockers: Vec<String>,
    pub next_step: String,
    pub verification: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEvent {
    pub event_id: String,
    pub workitem_id: String,
    pub stage: String,
    pub actor: String,
    pub action: String,
    pub timestamp: Timestamp,
    pub input_refs: Vec<String>,
    pub output_refs: Vec<String>,
    pub status: ExecutionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Started,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub artifact_id: String,
    pub r#type: String,
    pub producer: String,
    pub version: u32,
    pub path: String,
    pub related_workitem: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub memory_id: String,
    pub scope: MemoryScope,
    pub summary: String,
    pub source: String,
    pub updated_at: Timestamp,
    pub relevance: Relevance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryScope {
    Project,
    Workitem,
    Repo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Relevance {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SupervisionAssessment {
    OnTrack,
    Drifting,
    Stuck,
    AtRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guidance {
    pub guidance_id: String,
    pub workitem_id: String,
    pub stage: WorkStage,
    pub assessment: SupervisionAssessment,
    pub observations: Vec<String>,
    pub suggestions: Vec<String>,
    pub severity: Severity,
    pub should_intervene: bool,
    pub created_at: Timestamp,
}
