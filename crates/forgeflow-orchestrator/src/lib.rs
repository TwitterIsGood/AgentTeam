use forgeflow_core::{Result, new_id, now};
use forgeflow_domain::{ExecutionEvent, ExecutionStatus, WorkItem, WorkStage};
use forgeflow_memory::WorkItemStore;
use forgeflow_policy::{GateEvaluation, GateEvaluator, PolicyGate};
use std::collections::HashSet;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StageTransition {
    pub from_stage: WorkStage,
    pub to_stage: WorkStage,
    pub entry_gates: Vec<PolicyGate>,
    pub required_input_artifacts: Vec<String>,
    pub expected_output_artifacts: Vec<String>,
    pub failure_fallback: WorkStage,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TransitionResult {
    Ok {
        new_stage: WorkStage,
        event: ExecutionEvent,
    },
    Blocked {
        target_stage: WorkStage,
        evaluation: GateEvaluation,
        event: ExecutionEvent,
    },
    Failed {
        fallback_stage: WorkStage,
        reason: String,
        event: ExecutionEvent,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResumeContext {
    pub workitem: WorkItem,
    pub latest_checkpoint: Option<forgeflow_domain::Checkpoint>,
    pub completed_stages: HashSet<String>,
    pub next_action: String,
}

pub struct StateMachine {
    transitions: Vec<StageTransition>,
    evaluator: GateEvaluator,
}

impl StateMachine {
    pub fn new() -> Self {
        Self {
            transitions: Self::default_transitions(),
            evaluator: GateEvaluator::new(),
        }
    }

    pub fn with_transitions(transitions: Vec<StageTransition>) -> Self {
        Self {
            transitions,
            evaluator: GateEvaluator::new(),
        }
    }

    pub fn find_transition(&self, from: &WorkStage) -> Option<&StageTransition> {
        self.transitions.iter().find(|t| t.from_stage == *from)
    }

    pub fn try_advance(
        &self,
        workitem: &WorkItem,
        completed_stages: &HashSet<String>,
        actor: &str,
    ) -> TransitionResult {
        let transition = match self.find_transition(&workitem.stage) {
            Some(t) => t,
            None => {
                return TransitionResult::Failed {
                    fallback_stage: workitem.stage.clone(),
                    reason: "no outgoing transition from this stage".to_string(),
                    event: make_event(
                        &workitem.id,
                        &workitem.stage,
                        actor,
                        "transition attempted but no outgoing transition",
                        ExecutionStatus::Failed,
                    ),
                };
            }
        };

        let evaluation =
            self.evaluator
                .evaluate(&transition.entry_gates, workitem, completed_stages);

        if !evaluation.allowed {
            return TransitionResult::Blocked {
                target_stage: transition.to_stage.clone(),
                evaluation: GateEvaluation {
                    target_stage: transition.to_stage.to_string(),
                    ..evaluation
                },
                event: make_event(
                    &workitem.id,
                    &transition.to_stage,
                    actor,
                    "gate blocked transition",
                    ExecutionStatus::Failed,
                ),
            };
        }

        TransitionResult::Ok {
            new_stage: transition.to_stage.clone(),
            event: make_event(
                &workitem.id,
                &transition.to_stage,
                actor,
                "stage advanced",
                ExecutionStatus::Started,
            ),
        }
    }

    pub fn fallback_for(&self, stage: &WorkStage) -> WorkStage {
        self.transitions
            .iter()
            .find(|t| t.to_stage == *stage)
            .map(|t| t.failure_fallback.clone())
            .unwrap_or_else(|| stage.clone())
    }

    fn default_transitions() -> Vec<StageTransition> {
        use forgeflow_policy::GateType;
        vec![
            StageTransition {
                from_stage: WorkStage::Intake,
                to_stage: WorkStage::Roundtable,
                entry_gates: vec![],
                required_input_artifacts: vec![],
                expected_output_artifacts: vec!["Position".to_string()],
                failure_fallback: WorkStage::Intake,
            },
            StageTransition {
                from_stage: WorkStage::Roundtable,
                to_stage: WorkStage::Architecture,
                entry_gates: vec![PolicyGate {
                    name: "has-position".to_string(),
                    required: true,
                    gate_type: GateType::ArtifactExists {
                        artifact_type: "Position".to_string(),
                    },
                }],
                required_input_artifacts: vec!["Position".to_string()],
                expected_output_artifacts: vec!["Decision".to_string(), "Architecture".to_string()],
                failure_fallback: WorkStage::Intake,
            },
            StageTransition {
                from_stage: WorkStage::Architecture,
                to_stage: WorkStage::Implement,
                entry_gates: vec![
                    PolicyGate {
                        name: "has-decision".to_string(),
                        required: true,
                        gate_type: GateType::ArtifactExists {
                            artifact_type: "Decision".to_string(),
                        },
                    },
                    PolicyGate {
                        name: "has-architecture".to_string(),
                        required: true,
                        gate_type: GateType::ArtifactExists {
                            artifact_type: "Architecture".to_string(),
                        },
                    },
                ],
                required_input_artifacts: vec!["Decision".to_string(), "Architecture".to_string()],
                expected_output_artifacts: vec!["ChangeSet".to_string()],
                failure_fallback: WorkStage::Roundtable,
            },
            StageTransition {
                from_stage: WorkStage::Implement,
                to_stage: WorkStage::Test,
                entry_gates: vec![PolicyGate {
                    name: "has-changeset".to_string(),
                    required: true,
                    gate_type: GateType::ArtifactExists {
                        artifact_type: "ChangeSet".to_string(),
                    },
                }],
                required_input_artifacts: vec!["ChangeSet".to_string()],
                expected_output_artifacts: vec!["TestReport".to_string()],
                failure_fallback: WorkStage::Architecture,
            },
            StageTransition {
                from_stage: WorkStage::Test,
                to_stage: WorkStage::Review,
                entry_gates: vec![PolicyGate {
                    name: "has-test-report".to_string(),
                    required: true,
                    gate_type: GateType::ArtifactExists {
                        artifact_type: "TestReport".to_string(),
                    },
                }],
                required_input_artifacts: vec!["TestReport".to_string()],
                expected_output_artifacts: vec!["Review".to_string()],
                failure_fallback: WorkStage::Implement,
            },
            StageTransition {
                from_stage: WorkStage::Review,
                to_stage: WorkStage::PR,
                entry_gates: vec![PolicyGate {
                    name: "has-review".to_string(),
                    required: true,
                    gate_type: GateType::ArtifactExists {
                        artifact_type: "Review".to_string(),
                    },
                }],
                required_input_artifacts: vec!["Review".to_string()],
                expected_output_artifacts: vec![],
                failure_fallback: WorkStage::Test,
            },
            StageTransition {
                from_stage: WorkStage::PR,
                to_stage: WorkStage::Release,
                entry_gates: vec![PolicyGate {
                    name: "review-completed".to_string(),
                    required: true,
                    gate_type: GateType::StageCompleted {
                        stage: "Review".to_string(),
                    },
                }],
                required_input_artifacts: vec![],
                expected_output_artifacts: vec![],
                failure_fallback: WorkStage::Review,
            },
        ]
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

fn make_event(
    workitem_id: &str,
    target_stage: &WorkStage,
    actor: &str,
    action: &str,
    status: ExecutionStatus,
) -> ExecutionEvent {
    ExecutionEvent {
        event_id: new_id("evt"),
        workitem_id: workitem_id.to_string(),
        stage: target_stage.to_string(),
        actor: actor.to_string(),
        action: action.to_string(),
        timestamp: now(),
        input_refs: vec![],
        output_refs: vec![],
        status,
    }
}

pub fn resume(store: &WorkItemStore, id: &str) -> Result<ResumeContext> {
    let workitem = store.load_workitem(id)?;
    let latest_checkpoint = store.load_latest_checkpoint(id)?;
    let events = store.load_events(id)?;

    let completed_stages: HashSet<String> = events
        .iter()
        .filter(|e| matches!(e.status, ExecutionStatus::Completed))
        .map(|e| e.stage.clone())
        .collect();

    let next_action = latest_checkpoint
        .as_ref()
        .map(|cp| cp.next_step.clone())
        .unwrap_or_else(|| "advance to next stage".to_string());

    Ok(ResumeContext {
        workitem,
        latest_checkpoint,
        completed_stages,
        next_action,
    })
}

pub fn complete_stage(
    store: &WorkItemStore,
    workitem: &mut WorkItem,
    stage: &WorkStage,
    actor: &str,
    output_refs: Vec<String>,
) -> Result<ExecutionEvent> {
    let event = ExecutionEvent {
        event_id: new_id("evt"),
        workitem_id: workitem.id.clone(),
        stage: stage.to_string(),
        actor: actor.to_string(),
        action: format!("stage {stage} completed"),
        timestamp: now(),
        input_refs: vec![],
        output_refs,
        status: ExecutionStatus::Completed,
    };

    let file_name = format!("{}-complete.json", now().format("%Y%m%dT%H%M%SZ"));
    let json = serde_json::to_string_pretty(&event)?;
    store.append_event_json(&workitem.id, &file_name, &json)?;

    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgeflow_domain::{Priority, WorkItemType};

    fn test_workitem(stage: WorkStage, artifacts: Vec<String>) -> WorkItem {
        WorkItem {
            id: "wi-test".to_string(),
            title: "test".to_string(),
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

    #[test]
    fn test_new_state_machine_has_7_transitions() {
        let sm = StateMachine::new();
        assert_eq!(sm.transitions.len(), 7);
    }

    #[test]
    fn test_find_transition_from_intake() {
        let sm = StateMachine::new();
        let t = sm.find_transition(&WorkStage::Intake).unwrap();
        assert_eq!(t.to_stage, WorkStage::Roundtable);
        assert!(t.entry_gates.is_empty());
    }

    #[test]
    fn test_find_transition_from_architecture() {
        let sm = StateMachine::new();
        let t = sm.find_transition(&WorkStage::Architecture).unwrap();
        assert_eq!(t.entry_gates.len(), 2);
    }

    #[test]
    fn test_try_advance_from_intake_succeeds() {
        let sm = StateMachine::new();
        let wi = test_workitem(WorkStage::Intake, vec![]);
        let result = sm.try_advance(&wi, &HashSet::new(), "system");
        assert!(matches!(result, TransitionResult::Ok { .. }));
        if let TransitionResult::Ok { new_stage, .. } = result {
            assert_eq!(new_stage, WorkStage::Roundtable);
        }
    }

    #[test]
    fn test_try_advance_blocked_missing_artifact() {
        let sm = StateMachine::new();
        let wi = test_workitem(WorkStage::Roundtable, vec![]);
        let result = sm.try_advance(&wi, &HashSet::new(), "system");
        assert!(matches!(result, TransitionResult::Blocked { .. }));
    }

    #[test]
    fn test_try_advance_with_artifact_succeeds() {
        let sm = StateMachine::new();
        let wi = test_workitem(WorkStage::Roundtable, vec!["Position-001".to_string()]);
        let mut completed = HashSet::new();
        completed.insert("Roundtable".to_string());
        let result = sm.try_advance(&wi, &completed, "system");
        assert!(matches!(result, TransitionResult::Ok { .. }));
        if let TransitionResult::Ok { new_stage, .. } = result {
            assert_eq!(new_stage, WorkStage::Architecture);
        }
    }

    #[test]
    fn test_try_advance_from_release_returns_failed() {
        let sm = StateMachine::new();
        let wi = test_workitem(WorkStage::Release, vec![]);
        let result = sm.try_advance(&wi, &HashSet::new(), "system");
        assert!(matches!(result, TransitionResult::Failed { .. }));
    }

    #[test]
    fn test_fallback_for_implement() {
        let sm = StateMachine::new();
        assert_eq!(
            sm.fallback_for(&WorkStage::Implement),
            WorkStage::Roundtable
        );
    }

    #[test]
    fn test_full_lifecycle_advance() {
        let sm = StateMachine::new();
        let mut wi = test_workitem(WorkStage::Intake, vec![]);
        let mut completed = HashSet::new();

        let result = sm.try_advance(&wi, &completed, "system");
        assert!(matches!(result, TransitionResult::Ok { .. }));
        wi.stage = WorkStage::Roundtable;
        completed.insert("Intake".to_string());

        wi.artifacts.push("Position-001".to_string());
        completed.insert("Roundtable".to_string());
        let result = sm.try_advance(&wi, &completed, "system");
        assert!(matches!(result, TransitionResult::Ok { .. }));
        wi.stage = WorkStage::Architecture;

        wi.artifacts.push("Decision-001".to_string());
        wi.artifacts.push("Architecture-001".to_string());
        completed.insert("Architecture".to_string());
        let result = sm.try_advance(&wi, &completed, "system");
        assert!(matches!(result, TransitionResult::Ok { .. }));
        wi.stage = WorkStage::Implement;

        wi.artifacts.push("ChangeSet-001".to_string());
        completed.insert("Implement".to_string());
        let result = sm.try_advance(&wi, &completed, "system");
        assert!(matches!(result, TransitionResult::Ok { .. }));
        wi.stage = WorkStage::Test;

        wi.artifacts.push("TestReport-001".to_string());
        completed.insert("Test".to_string());
        let result = sm.try_advance(&wi, &completed, "system");
        assert!(matches!(result, TransitionResult::Ok { .. }));
        wi.stage = WorkStage::Review;

        wi.artifacts.push("Review-001".to_string());
        completed.insert("Review".to_string());
        let result = sm.try_advance(&wi, &completed, "system");
        assert!(matches!(result, TransitionResult::Ok { .. }));
        wi.stage = WorkStage::PR;

        let result = sm.try_advance(&wi, &completed, "system");
        assert!(matches!(result, TransitionResult::Ok { .. }));
        if let TransitionResult::Ok { new_stage, .. } = result {
            assert_eq!(new_stage, WorkStage::Release);
        }
    }

    #[test]
    fn test_with_transitions_custom() {
        let sm = StateMachine::with_transitions(vec![StageTransition {
            from_stage: WorkStage::Intake,
            to_stage: WorkStage::Release,
            entry_gates: vec![],
            required_input_artifacts: vec![],
            expected_output_artifacts: vec![],
            failure_fallback: WorkStage::Intake,
        }]);
        let t = sm.find_transition(&WorkStage::Intake).unwrap();
        assert_eq!(t.to_stage, WorkStage::Release);
        assert!(sm.find_transition(&WorkStage::Roundtable).is_none());
    }
}
