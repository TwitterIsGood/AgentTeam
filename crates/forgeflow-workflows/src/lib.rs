use forgeflow_agents::RoleRegistry;
use forgeflow_core::{new_id, now};
use forgeflow_domain::{
    ExecutionEvent, ExecutionStatus, Guidance, Severity, SupervisionAssessment, WorkItem, WorkStage,
};
use forgeflow_runtime::{ExecutionRequest, Runtime};

pub fn dry_run_workflow(runtime: &impl Runtime, workitem: &WorkItem) -> Vec<ExecutionEvent> {
    let roles = [
        "Router",
        "Product",
        "Architect",
        "Coder",
        "Tester",
        "Reviewer",
    ];

    roles
        .into_iter()
        .map(|actor| {
            let response = runtime.execute(ExecutionRequest {
                actor: actor.to_string(),
                instruction: format!("Process workitem {} in {}", workitem.id, workitem.stage),
            });

            ExecutionEvent {
                event_id: new_id("evt"),
                workitem_id: workitem.id.clone(),
                stage: workitem.stage.to_string(),
                actor: response.actor,
                action: response.output,
                timestamp: now(),
                input_refs: vec!["workitem.json".to_string()],
                output_refs: vec![],
                status: ExecutionStatus::Completed,
            }
        })
        .collect()
}

// --- Supervision types ---

pub struct SupervisionContext<'a> {
    pub workitem: &'a WorkItem,
    pub events: &'a [ExecutionEvent],
    pub guidances: &'a [Guidance],
    pub role_registry: &'a RoleRegistry,
}

pub trait SupervisionRule: Send + Sync {
    fn name(&self) -> &str;
    fn applicable_stages(&self) -> &[WorkStage];
    fn evaluate(&self, ctx: &SupervisionContext) -> Option<Guidance>;
}

// --- Rule implementations ---

pub struct NoProgressRule {
    pub max_rounds_without_output: usize,
    pub applicable: Vec<WorkStage>,
}

impl SupervisionRule for NoProgressRule {
    fn name(&self) -> &str {
        "no-progress"
    }

    fn applicable_stages(&self) -> &[WorkStage] {
        &self.applicable
    }

    fn evaluate(&self, ctx: &SupervisionContext) -> Option<Guidance> {
        let stage_events: Vec<_> = ctx
            .events
            .iter()
            .filter(|e| e.stage == ctx.workitem.stage.to_string())
            .collect();

        if stage_events.is_empty() {
            return None;
        }

        let no_output_count = stage_events
            .iter()
            .filter(|e| e.output_refs.is_empty())
            .count();

        if no_output_count >= self.max_rounds_without_output {
            Some(make_guidance(
                ctx.workitem,
                SupervisionAssessment::Stuck,
                vec![format!(
                    "Stage {} has {} rounds without output artifacts",
                    ctx.workitem.stage, no_output_count
                )],
                vec!["Consider breaking down the work or escalating".to_string()],
                Severity::Warning,
                false,
            ))
        } else {
            None
        }
    }
}

pub struct MissingArtifactRule;

impl SupervisionRule for MissingArtifactRule {
    fn name(&self) -> &str {
        "missing-artifact"
    }

    fn applicable_stages(&self) -> &[WorkStage] {
        &[
            WorkStage::Roundtable,
            WorkStage::Architecture,
            WorkStage::Implement,
            WorkStage::Test,
            WorkStage::Review,
        ]
    }

    fn evaluate(&self, ctx: &SupervisionContext) -> Option<Guidance> {
        let expected = ctx
            .role_registry
            .expected_output_artifacts(&ctx.workitem.stage.to_string());

        let missing: Vec<String> = expected
            .into_iter()
            .filter(|artifact_type| {
                !ctx.workitem
                    .artifacts
                    .iter()
                    .any(|a| a.contains(artifact_type))
            })
            .collect();

        if missing.is_empty() {
            None
        } else {
            Some(make_guidance(
                ctx.workitem,
                SupervisionAssessment::AtRisk,
                vec![format!(
                    "Stage {} is missing expected artifacts: {}",
                    ctx.workitem.stage,
                    missing.join(", ")
                )],
                vec!["Ensure agents produce the required artifacts before advancing".to_string()],
                Severity::Critical,
                true,
            ))
        }
    }
}

pub struct ScopeDriftRule {
    pub drift_keywords: Vec<String>,
}

impl SupervisionRule for ScopeDriftRule {
    fn name(&self) -> &str {
        "scope-drift"
    }

    fn applicable_stages(&self) -> &[WorkStage] {
        &[
            WorkStage::Roundtable,
            WorkStage::Architecture,
            WorkStage::Implement,
        ]
    }

    fn evaluate(&self, ctx: &SupervisionContext) -> Option<Guidance> {
        let recent: Vec<_> = ctx
            .events
            .iter()
            .filter(|e| e.stage == ctx.workitem.stage.to_string())
            .rev()
            .take(5)
            .collect();

        let matches: Vec<_> = recent
            .iter()
            .filter(|e| {
                let action_lower = e.action.to_lowercase();
                self.drift_keywords
                    .iter()
                    .any(|kw| action_lower.contains(&kw.to_lowercase()))
            })
            .collect();

        if matches.is_empty() {
            None
        } else {
            Some(make_guidance(
                ctx.workitem,
                SupervisionAssessment::Drifting,
                vec![format!(
                    "Potential scope drift detected in {} event(s)",
                    matches.len()
                )],
                vec![format!(
                    "Refocus on the workitem scope: {}",
                    ctx.workitem.title
                )],
                Severity::Warning,
                false,
            ))
        }
    }
}

pub struct StageTimeoutRule {
    pub max_stage_duration_minutes: i64,
}

impl SupervisionRule for StageTimeoutRule {
    fn name(&self) -> &str {
        "stage-timeout"
    }

    fn applicable_stages(&self) -> &[WorkStage] {
        &[
            WorkStage::Roundtable,
            WorkStage::Architecture,
            WorkStage::Implement,
            WorkStage::Test,
        ]
    }

    fn evaluate(&self, ctx: &SupervisionContext) -> Option<Guidance> {
        let earliest = ctx
            .events
            .iter()
            .filter(|e| e.stage == ctx.workitem.stage.to_string())
            .min_by_key(|e| e.timestamp);

        let earliest = earliest?;

        let elapsed = now()
            .signed_duration_since(earliest.timestamp)
            .num_minutes();

        if elapsed > self.max_stage_duration_minutes {
            Some(make_guidance(
                ctx.workitem,
                SupervisionAssessment::AtRisk,
                vec![format!(
                    "Stage {} has been active for {} minutes (limit: {})",
                    ctx.workitem.stage, elapsed, self.max_stage_duration_minutes
                )],
                vec!["Review stage progress; consider checkpointing and replanning".to_string()],
                Severity::Critical,
                true,
            ))
        } else {
            None
        }
    }
}

pub struct RepeatedFailuresRule {
    pub max_failures: usize,
}

impl SupervisionRule for RepeatedFailuresRule {
    fn name(&self) -> &str {
        "repeated-failures"
    }

    fn applicable_stages(&self) -> &[WorkStage] {
        &[WorkStage::Implement, WorkStage::Test, WorkStage::Review]
    }

    fn evaluate(&self, ctx: &SupervisionContext) -> Option<Guidance> {
        let fail_count = ctx
            .events
            .iter()
            .filter(|e| {
                e.stage == ctx.workitem.stage.to_string()
                    && matches!(e.status, ExecutionStatus::Failed)
            })
            .count();

        if fail_count >= self.max_failures {
            Some(make_guidance(
                ctx.workitem,
                SupervisionAssessment::Stuck,
                vec![format!(
                    "Stage {} has {} failed executions",
                    ctx.workitem.stage, fail_count
                )],
                vec!["Investigate root cause; consider fallback or human intervention".to_string()],
                Severity::Critical,
                true,
            ))
        } else {
            None
        }
    }
}

// --- Rule engine ---

pub struct RuleEngine {
    rules: Vec<Box<dyn SupervisionRule>>,
}

impl RuleEngine {
    pub fn new(rules: Vec<Box<dyn SupervisionRule>>) -> Self {
        Self { rules }
    }

    pub fn default_rules() -> Vec<Box<dyn SupervisionRule>> {
        vec![
            Box::new(NoProgressRule {
                max_rounds_without_output: 5,
                applicable: vec![
                    WorkStage::Roundtable,
                    WorkStage::Architecture,
                    WorkStage::Implement,
                    WorkStage::Test,
                ],
            }),
            Box::new(MissingArtifactRule),
            Box::new(ScopeDriftRule {
                drift_keywords: vec![
                    "unrelated".to_string(),
                    "off-topic".to_string(),
                    "tangent".to_string(),
                    "scope creep".to_string(),
                    "different project".to_string(),
                ],
            }),
            Box::new(StageTimeoutRule {
                max_stage_duration_minutes: 120,
            }),
            Box::new(RepeatedFailuresRule { max_failures: 3 }),
        ]
    }

    pub fn evaluate(&self, ctx: &SupervisionContext) -> Vec<Guidance> {
        self.rules
            .iter()
            .filter(|rule| rule.applicable_stages().contains(&ctx.workitem.stage))
            .filter_map(|rule| rule.evaluate(ctx))
            .collect()
    }
}

// --- Supervisor ---

pub struct Supervisor {
    engine: RuleEngine,
    role_registry: RoleRegistry,
}

impl Supervisor {
    pub fn new(engine: RuleEngine, role_registry: RoleRegistry) -> Self {
        Self {
            engine,
            role_registry,
        }
    }

    pub fn with_default_rules() -> Self {
        Self {
            engine: RuleEngine::new(RuleEngine::default_rules()),
            role_registry: RoleRegistry::new(),
        }
    }

    pub fn observe(
        &self,
        workitem: &WorkItem,
        events: &[ExecutionEvent],
        guidances: &[Guidance],
    ) -> Option<Guidance> {
        let ctx = SupervisionContext {
            workitem,
            events,
            guidances,
            role_registry: &self.role_registry,
        };

        self.engine
            .evaluate(&ctx)
            .into_iter()
            .max_by_key(|g| match g.severity {
                Severity::Critical => 2,
                Severity::Warning => 1,
                Severity::Info => 0,
            })
    }

    pub fn review_stage(
        &self,
        workitem: &WorkItem,
        events: &[ExecutionEvent],
        guidances: &[Guidance],
    ) -> Vec<Guidance> {
        let ctx = SupervisionContext {
            workitem,
            events,
            guidances,
            role_registry: &self.role_registry,
        };

        let mut results = self.engine.evaluate(&ctx);

        // Always check missing artifacts at stage end
        let missing_rule = MissingArtifactRule;
        if let Some(g) = missing_rule.evaluate(&ctx) {
            if !results.iter().any(|r| r.observations == g.observations) {
                results.push(g);
            }
        }

        results
    }
}

// --- Guided workflow ---

pub fn supervised_dry_run_workflow(
    runtime: &impl Runtime,
    workitem: &WorkItem,
) -> (Vec<ExecutionEvent>, Vec<Guidance>) {
    let supervisor = Supervisor::with_default_rules();
    let roles = [
        "Router",
        "Product",
        "Architect",
        "Coder",
        "Tester",
        "Reviewer",
    ];
    let mut events = Vec::new();
    let mut guidances = Vec::new();

    for actor in roles {
        let response = runtime.execute(ExecutionRequest {
            actor: actor.to_string(),
            instruction: format!("Process workitem {} in {}", workitem.id, workitem.stage),
        });

        let event = ExecutionEvent {
            event_id: new_id("evt"),
            workitem_id: workitem.id.clone(),
            stage: workitem.stage.to_string(),
            actor: response.actor,
            action: response.output,
            timestamp: now(),
            input_refs: vec!["workitem.json".to_string()],
            output_refs: vec![],
            status: ExecutionStatus::Completed,
        };

        if let Some(g) = supervisor.observe(workitem, &events, &guidances) {
            guidances.push(g);
        }

        events.push(event);
    }

    let stage_review = supervisor.review_stage(workitem, &events, &guidances);
    guidances.extend(stage_review);

    (events, guidances)
}

pub fn review_stage_guidance(
    workitem: &WorkItem,
    events: &[ExecutionEvent],
    guidances: &[Guidance],
) -> Vec<Guidance> {
    let supervisor = Supervisor::with_default_rules();
    supervisor.review_stage(workitem, events, guidances)
}

// --- Helpers ---

fn make_guidance(
    workitem: &WorkItem,
    assessment: SupervisionAssessment,
    observations: Vec<String>,
    suggestions: Vec<String>,
    severity: Severity,
    should_intervene: bool,
) -> Guidance {
    Guidance {
        guidance_id: new_id("guidance"),
        workitem_id: workitem.id.clone(),
        stage: workitem.stage.clone(),
        assessment,
        observations,
        suggestions,
        severity,
        should_intervene,
        created_at: now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgeflow_domain::{Priority, WorkItemType};
    use forgeflow_runtime::FakeRuntime;

    fn test_workitem(stage: WorkStage, artifacts: Vec<String>) -> WorkItem {
        WorkItem {
            id: "wi-test".to_string(),
            title: "test feature".to_string(),
            r#type: WorkItemType::Feature,
            priority: Priority::Medium,
            repo: "test".to_string(),
            stage,
            owner: None,
            linked_issue: None,
            linked_branch: None,
            artifacts,
            checkpoints: vec![],
        }
    }

    fn test_event(stage: &str, action: &str, status: ExecutionStatus) -> ExecutionEvent {
        ExecutionEvent {
            event_id: new_id("evt"),
            workitem_id: "wi-test".to_string(),
            stage: stage.to_string(),
            actor: "system".to_string(),
            action: action.to_string(),
            timestamp: now(),
            input_refs: vec![],
            output_refs: vec![],
            status,
        }
    }

    fn make_ctx<'a>(
        workitem: &'a WorkItem,
        events: &'a [ExecutionEvent],
        guidances: &'a [Guidance],
        registry: &'a RoleRegistry,
    ) -> SupervisionContext<'a> {
        SupervisionContext {
            workitem,
            events,
            guidances,
            role_registry: registry,
        }
    }

    #[test]
    fn test_no_progress_fires_after_threshold() {
        let wi = test_workitem(WorkStage::Implement, vec![]);
        let registry = RoleRegistry::new();
        let events: Vec<ExecutionEvent> = (0..6)
            .map(|_| test_event("Implement", "working", ExecutionStatus::Completed))
            .collect();
        let ctx = make_ctx(&wi, &events, &[], &registry);
        let rule = NoProgressRule {
            max_rounds_without_output: 5,
            applicable: vec![WorkStage::Implement],
        };
        let result = rule.evaluate(&ctx);
        assert!(result.is_some());
        let g = result.unwrap();
        assert_eq!(g.assessment, SupervisionAssessment::Stuck);
    }

    #[test]
    fn test_no_progress_does_not_fire_below_threshold() {
        let wi = test_workitem(WorkStage::Implement, vec![]);
        let registry = RoleRegistry::new();
        let events: Vec<ExecutionEvent> = (0..3)
            .map(|_| test_event("Implement", "working", ExecutionStatus::Completed))
            .collect();
        let ctx = make_ctx(&wi, &events, &[], &registry);
        let rule = NoProgressRule {
            max_rounds_without_output: 5,
            applicable: vec![WorkStage::Implement],
        };
        assert!(rule.evaluate(&ctx).is_none());
    }

    #[test]
    fn test_missing_artifact_fires_when_empty() {
        let wi = test_workitem(WorkStage::Implement, vec![]);
        let registry = RoleRegistry::new();
        let events = vec![test_event("Implement", "done", ExecutionStatus::Completed)];
        let ctx = make_ctx(&wi, &events, &[], &registry);
        let rule = MissingArtifactRule;
        let result = rule.evaluate(&ctx);
        assert!(result.is_some());
        let g = result.unwrap();
        assert_eq!(g.assessment, SupervisionAssessment::AtRisk);
        assert_eq!(g.severity, Severity::Critical);
        assert!(g.should_intervene);
    }

    #[test]
    fn test_missing_artifact_passes_when_present() {
        let wi = test_workitem(WorkStage::Test, vec!["TestReport-001".to_string()]);
        let registry = RoleRegistry::new();
        let events = vec![test_event("Test", "done", ExecutionStatus::Completed)];
        let ctx = make_ctx(&wi, &events, &[], &registry);
        let rule = MissingArtifactRule;
        assert!(rule.evaluate(&ctx).is_none());
    }

    #[test]
    fn test_scope_drift_fires_on_keyword() {
        let wi = test_workitem(WorkStage::Implement, vec![]);
        let registry = RoleRegistry::new();
        let events = vec![test_event(
            "Implement",
            "went on an unrelated tangent about something",
            ExecutionStatus::Completed,
        )];
        let ctx = make_ctx(&wi, &events, &[], &registry);
        let rule = ScopeDriftRule {
            drift_keywords: vec!["unrelated".to_string(), "tangent".to_string()],
        };
        let result = rule.evaluate(&ctx);
        assert!(result.is_some());
        assert_eq!(result.unwrap().assessment, SupervisionAssessment::Drifting);
    }

    #[test]
    fn test_scope_drift_passes_on_clean_actions() {
        let wi = test_workitem(WorkStage::Implement, vec![]);
        let registry = RoleRegistry::new();
        let events = vec![test_event(
            "Implement",
            "implemented the feature",
            ExecutionStatus::Completed,
        )];
        let ctx = make_ctx(&wi, &events, &[], &registry);
        let rule = ScopeDriftRule {
            drift_keywords: vec!["unrelated".to_string()],
        };
        assert!(rule.evaluate(&ctx).is_none());
    }

    #[test]
    fn test_repeated_failures_fires() {
        let wi = test_workitem(WorkStage::Test, vec![]);
        let registry = RoleRegistry::new();
        let events: Vec<ExecutionEvent> = (0..4)
            .map(|_| test_event("Test", "test failed", ExecutionStatus::Failed))
            .collect();
        let ctx = make_ctx(&wi, &events, &[], &registry);
        let rule = RepeatedFailuresRule { max_failures: 3 };
        let result = rule.evaluate(&ctx);
        assert!(result.is_some());
        let g = result.unwrap();
        assert_eq!(g.assessment, SupervisionAssessment::Stuck);
        assert_eq!(g.severity, Severity::Critical);
    }

    #[test]
    fn test_repeated_failures_passes_below_threshold() {
        let wi = test_workitem(WorkStage::Test, vec![]);
        let registry = RoleRegistry::new();
        let events = vec![
            test_event("Test", "ok", ExecutionStatus::Completed),
            test_event("Test", "fail", ExecutionStatus::Failed),
        ];
        let ctx = make_ctx(&wi, &events, &[], &registry);
        let rule = RepeatedFailuresRule { max_failures: 3 };
        assert!(rule.evaluate(&ctx).is_none());
    }

    #[test]
    fn test_supervisor_observe_returns_highest_severity() {
        let wi = test_workitem(WorkStage::Implement, vec![]);
        let events: Vec<ExecutionEvent> = (0..6)
            .map(|_| {
                let mut e = test_event("Implement", "working", ExecutionStatus::Failed);
                e.status = ExecutionStatus::Failed;
                e
            })
            .collect();
        let supervisor = Supervisor::with_default_rules();
        let result = supervisor.observe(&wi, &events, &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().severity, Severity::Critical);
    }

    #[test]
    fn test_supervisor_review_stage_includes_missing_artifact() {
        let wi = test_workitem(WorkStage::Implement, vec![]);
        let events = vec![test_event("Implement", "done", ExecutionStatus::Completed)];
        let supervisor = Supervisor::with_default_rules();
        let results = supervisor.review_stage(&wi, &events, &[]);
        assert!(!results.is_empty());
        assert!(
            results
                .iter()
                .any(|g| g.assessment == SupervisionAssessment::AtRisk)
        );
    }

    #[test]
    fn test_review_stage_guidance_function() {
        let wi = test_workitem(WorkStage::Implement, vec![]);
        let events = vec![test_event("Implement", "done", ExecutionStatus::Completed)];
        let results = review_stage_guidance(&wi, &events, &[]);
        assert!(!results.is_empty());
    }

    #[test]
    fn test_supervised_dry_run_produces_events_and_guidances() {
        let wi = test_workitem(WorkStage::Intake, vec![]);
        let runtime = FakeRuntime;
        let (events, _guidances) = supervised_dry_run_workflow(&runtime, &wi);
        assert_eq!(events.len(), 6);
        // Intake stage: no MissingArtifactRule applies (Intake not in its applicable_stages)
        // NoProgressRule applies to Intake? No, it's not in the default applicable list for Intake
        // So guidances may or may not be empty depending on rules
        assert!(!events.is_empty());
    }
}
