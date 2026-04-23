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

// --- Prompt assembly ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledPrompt {
    pub system_prompt: String,
    pub user_message: String,
}

// --- Artifact extraction ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedArtifact {
    pub artifact_id: String,
    pub artifact_type: String,
    pub content: String,
    pub producer: String,
}

// --- Self-learning ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub lesson_id: String,
    pub scope: LessonScope,
    pub observation: String,
    pub recommendation: String,
    pub evidence: String,
    pub learned_at: Timestamp,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonScope {
    pub variant: MemoryScope,
    pub stage: Option<WorkStage>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdAdjustment {
    pub rule_name: String,
    pub parameter: String,
    pub original_value: f64,
    pub adjusted_value: f64,
    pub reason: String,
    pub adjusted_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageDurationStats {
    pub stage: WorkStage,
    pub sample_count: usize,
    pub avg_iterations: f64,
    pub avg_cost_usd: f64,
    pub success_rate: f64,
}

// --- Loop types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopConfig {
    pub max_iterations: usize,
    pub max_cost_usd: f64,
    pub max_stage_retries: usize,
    pub pause_on_critical: bool,
    pub goal: String,
    pub runtime: String,
    pub model: String,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            max_cost_usd: 5.0,
            max_stage_retries: 3,
            pause_on_critical: true,
            goal: String::new(),
            runtime: "openai".to_string(),
            model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoopOutcome {
    Completed {
        workitem_id: String,
        stages_completed: Vec<String>,
        total_cost_usd: f64,
    },
    Exhausted {
        workitem_id: String,
        reason: String,
    },
    BudgetExceeded {
        workitem_id: String,
        cost_usd: f64,
    },
    PausedForGuidance {
        workitem_id: String,
        stage: WorkStage,
        guidance_summary: String,
    },
    PausedForIntervention {
        workitem_id: String,
        stage: WorkStage,
        guidance_summary: String,
    },
    Stuck {
        workitem_id: String,
        stage: WorkStage,
        reason: String,
    },
}
