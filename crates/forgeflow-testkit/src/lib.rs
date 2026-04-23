use forgeflow_core::new_id;
use forgeflow_domain::{
    ExecutionEvent, ExecutionStatus, Guidance, Priority, Severity,
    WorkItem, WorkItemType, WorkStage,
};
use forgeflow_workflows::review_stage_guidance;
use std::fmt::Write;

// Re-export for convenience
pub use forgeflow_runtime::FakeRuntime;

// --- Fixture builders ---

pub struct WorkItemBuilder {
    id: String,
    title: String,
    r#type: WorkItemType,
    priority: Priority,
    stage: WorkStage,
    artifacts: Vec<String>,
    owner: Option<String>,
}

impl WorkItemBuilder {
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.to_string(),
            title: title.to_string(),
            r#type: WorkItemType::Feature,
            priority: Priority::Medium,
            stage: WorkStage::Intake,
            artifacts: vec![],
            owner: None,
        }
    }

    pub fn stage(mut self, stage: WorkStage) -> Self {
        self.stage = stage;
        self
    }

    pub fn kind(mut self, r#type: WorkItemType) -> Self {
        self.r#type = r#type;
        self
    }

    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    pub fn artifact(mut self, artifact: &str) -> Self {
        self.artifacts.push(artifact.to_string());
        self
    }

    pub fn owner(mut self, owner: &str) -> Self {
        self.owner = Some(owner.to_string());
        self
    }

    pub fn build(self) -> WorkItem {
        WorkItem {
            id: self.id,
            title: self.title,
            r#type: self.r#type,
            priority: self.priority,
            repo: "test-repo".to_string(),
            stage: self.stage,
            owner: self.owner,
            linked_issue: None,
            linked_branch: None,
            artifacts: self.artifacts,
            checkpoints: vec![],
        }
    }
}

pub struct EventBuilder {
    workitem_id: String,
    stage: String,
    actor: String,
    action: String,
    status: ExecutionStatus,
    output_refs: Vec<String>,
}

impl EventBuilder {
    pub fn new(workitem_id: &str) -> Self {
        Self {
            workitem_id: workitem_id.to_string(),
            stage: "Implement".to_string(),
            actor: "system".to_string(),
            action: "did something".to_string(),
            status: ExecutionStatus::Completed,
            output_refs: vec![],
        }
    }

    pub fn stage(mut self, stage: &str) -> Self {
        self.stage = stage.to_string();
        self
    }

    pub fn actor(mut self, actor: &str) -> Self {
        self.actor = actor.to_string();
        self
    }

    pub fn action(mut self, action: &str) -> Self {
        self.action = action.to_string();
        self
    }

    pub fn status(mut self, status: ExecutionStatus) -> Self {
        self.status = status;
        self
    }

    pub fn output_ref(mut self, r#ref: &str) -> Self {
        self.output_refs.push(r#ref.to_string());
        self
    }

    pub fn build(self) -> ExecutionEvent {
        ExecutionEvent {
            event_id: new_id("evt"),
            workitem_id: self.workitem_id,
            stage: self.stage,
            actor: self.actor,
            action: self.action,
            timestamp: forgeflow_core::now(),
            input_refs: vec![],
            output_refs: self.output_refs,
            status: self.status,
        }
    }
}

// --- Scenario runner ---

pub struct ScenarioResult {
    pub events: Vec<ExecutionEvent>,
    pub guidances: Vec<Guidance>,
}

pub fn run_dry_run_scenario(workitem: &WorkItem) -> ScenarioResult {
    let runtime = FakeRuntime;
    let events = forgeflow_workflows::dry_run_workflow(&runtime, workitem);
    let guidances = review_stage_guidance(workitem, &events, &[]);
    ScenarioResult { events, guidances }
}

// --- Assertions ---

pub fn assert_stage_has_completed_events(events: &[ExecutionEvent], stage: &str) -> Result<(), String> {
    let completed: Vec<_> = events
        .iter()
        .filter(|e| e.stage == stage && matches!(e.status, ExecutionStatus::Completed))
        .collect();

    if completed.is_empty() {
        Err(format!("No completed events for stage '{stage}'"))
    } else {
        Ok(())
    }
}

pub fn assert_no_failed_events(events: &[ExecutionEvent]) -> Result<(), String> {
    let failed: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.status, ExecutionStatus::Failed))
        .collect();

    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} failed event(s) found", failed.len()))
    }
}

pub fn assert_events_from_actor(events: &[ExecutionEvent], actor: &str, min_count: usize) -> Result<(), String> {
    let count = events.iter().filter(|e| e.actor == actor).count();
    if count >= min_count {
        Ok(())
    } else {
        Err(format!(
            "Expected at least {min_count} events from '{actor}', found {count}"
        ))
    }
}

pub fn assert_guidance_severity(guidances: &[Guidance], expected_severity: Severity) -> Result<(), String> {
    if guidances.iter().any(|g| g.severity == expected_severity) {
        Ok(())
    } else {
        Err(format!(
            "No guidance with severity {:?} found",
            expected_severity
        ))
    }
}

// --- Report formatting ---

pub fn format_scenario_report(result: &ScenarioResult) -> String {
    let mut report = String::new();
    let _ = writeln!(report, "Scenario Report");
    let _ = writeln!(report, "  Events: {}", result.events.len());
    let _ = writeln!(report, "  Guidances: {}", result.guidances.len());

    let completed = result
        .events
        .iter()
        .filter(|e| matches!(e.status, ExecutionStatus::Completed))
        .count();
    let failed = result
        .events
        .iter()
        .filter(|e| matches!(e.status, ExecutionStatus::Failed))
        .count();

    let _ = writeln!(report, "  Completed: {completed}");
    let _ = writeln!(report, "  Failed: {failed}");

    if !result.guidances.is_empty() {
        let _ = writeln!(report, "  Guidance details:");
        for g in &result.guidances {
            let _ = writeln!(
                report,
                "    [{:?}] {:?} - stage {:?}",
                g.severity, g.assessment, g.stage
            );
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workitem_builder_defaults() {
        let wi = WorkItemBuilder::new("wi-1", "test").build();
        assert_eq!(wi.id, "wi-1");
        assert_eq!(wi.title, "test");
        assert_eq!(wi.stage, WorkStage::Intake);
        assert!(wi.artifacts.is_empty());
    }

    #[test]
    fn test_workitem_builder_custom() {
        let wi = WorkItemBuilder::new("wi-2", "bug fix")
            .kind(WorkItemType::Bugfix)
            .priority(Priority::High)
            .stage(WorkStage::Implement)
            .artifact("src/main.rs")
            .owner("coder")
            .build();
        assert_eq!(wi.r#type, WorkItemType::Bugfix);
        assert_eq!(wi.priority, Priority::High);
        assert_eq!(wi.stage, WorkStage::Implement);
        assert_eq!(wi.artifacts.len(), 1);
        assert_eq!(wi.owner.unwrap(), "coder");
    }

    #[test]
    fn test_event_builder() {
        let evt = EventBuilder::new("wi-1")
            .stage("Test")
            .actor("Tester")
            .action("ran unit tests")
            .status(ExecutionStatus::Completed)
            .output_ref("TestReport-001")
            .build();
        assert_eq!(evt.stage, "Test");
        assert_eq!(evt.actor, "Tester");
        assert_eq!(evt.output_refs.len(), 1);
    }

    #[test]
    fn test_run_dry_run_scenario() {
        let wi = WorkItemBuilder::new("wi-sc", "scenario test")
            .stage(WorkStage::Implement)
            .build();
        let result = run_dry_run_scenario(&wi);
        assert_eq!(result.events.len(), 6);
    }

    #[test]
    fn test_assert_stage_has_completed_events_ok() {
        let events = vec![EventBuilder::new("wi-1")
            .stage("Implement")
            .status(ExecutionStatus::Completed)
            .build()];
        assert!(assert_stage_has_completed_events(&events, "Implement").is_ok());
    }

    #[test]
    fn test_assert_stage_has_completed_events_fail() {
        let events = vec![EventBuilder::new("wi-1")
            .stage("Implement")
            .status(ExecutionStatus::Failed)
            .build()];
        assert!(assert_stage_has_completed_events(&events, "Implement").is_err());
    }

    #[test]
    fn test_assert_no_failed_events() {
        let events = vec![EventBuilder::new("wi-1")
            .status(ExecutionStatus::Completed)
            .build()];
        assert!(assert_no_failed_events(&events).is_ok());

        let events_with_fail = vec![EventBuilder::new("wi-1")
            .status(ExecutionStatus::Failed)
            .build()];
        assert!(assert_no_failed_events(&events_with_fail).is_err());
    }

    #[test]
    fn test_assert_events_from_actor() {
        let events = vec![
            EventBuilder::new("wi-1").actor("Coder").build(),
            EventBuilder::new("wi-1").actor("Coder").build(),
            EventBuilder::new("wi-1").actor("Tester").build(),
        ];
        assert!(assert_events_from_actor(&events, "Coder", 2).is_ok());
        assert!(assert_events_from_actor(&events, "Coder", 3).is_err());
    }

    #[test]
    fn test_format_scenario_report() {
        let wi = WorkItemBuilder::new("wi-rpt", "report test").build();
        let result = run_dry_run_scenario(&wi);
        let report = format_scenario_report(&result);
        assert!(report.contains("Scenario Report"));
        assert!(report.contains("Events: 6"));
    }
}
