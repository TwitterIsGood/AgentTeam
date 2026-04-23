use forgeflow_domain::WorkItem;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GateType {
    ArtifactExists { artifact_type: String },
    StageCompleted { stage: String },
    ManualApproval { approver: String },
    CustomCondition { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyGate {
    pub name: String,
    pub required: bool,
    pub gate_type: GateType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GateResult {
    Passed,
    Failed { reason: String },
    Warning { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateEvaluation {
    pub target_stage: String,
    pub results: Vec<(PolicyGate, GateResult)>,
    pub allowed: bool,
}

pub struct GateEvaluator;

impl GateEvaluator {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(
        &self,
        gates: &[PolicyGate],
        workitem: &WorkItem,
        completed_stages: &HashSet<String>,
    ) -> GateEvaluation {
        let results: Vec<(PolicyGate, GateResult)> = gates
            .iter()
            .map(|gate| {
                (
                    gate.clone(),
                    self.check_single(gate, workitem, completed_stages),
                )
            })
            .collect();

        let allowed = results.iter().all(|(gate, result)| {
            if !gate.required {
                return true;
            }
            !matches!(result, GateResult::Failed { .. })
        });

        GateEvaluation {
            target_stage: String::new(),
            results,
            allowed,
        }
    }

    fn check_single(
        &self,
        gate: &PolicyGate,
        workitem: &WorkItem,
        completed_stages: &HashSet<String>,
    ) -> GateResult {
        match &gate.gate_type {
            GateType::ArtifactExists { artifact_type } => {
                let found = workitem.artifacts.iter().any(|a| a.contains(artifact_type));
                if found {
                    GateResult::Passed
                } else if gate.required {
                    GateResult::Failed {
                        reason: format!("missing artifact: {artifact_type}"),
                    }
                } else {
                    GateResult::Warning {
                        reason: format!("missing artifact: {artifact_type}"),
                    }
                }
            }
            GateType::StageCompleted { stage } => {
                if completed_stages.contains(stage) {
                    GateResult::Passed
                } else if gate.required {
                    GateResult::Failed {
                        reason: format!("stage not completed: {stage}"),
                    }
                } else {
                    GateResult::Warning {
                        reason: format!("stage not completed: {stage}"),
                    }
                }
            }
            GateType::ManualApproval { .. } => GateResult::Passed,
            GateType::CustomCondition { .. } => GateResult::Passed,
        }
    }
}

impl Default for GateEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgeflow_domain::{Priority, WorkItemType, WorkStage};

    fn test_workitem(artifacts: Vec<String>) -> WorkItem {
        WorkItem {
            id: "wi-test".to_string(),
            title: "test".to_string(),
            r#type: WorkItemType::Feature,
            priority: Priority::Medium,
            repo: "test".to_string(),
            stage: WorkStage::Intake,
            owner: None,
            linked_issue: None,
            linked_branch: None,
            artifacts,
            checkpoints: vec![],
        }
    }

    #[test]
    fn test_artifact_exists_gate_passes() {
        let wi = test_workitem(vec!["Decision-001".to_string()]);
        let gate = PolicyGate {
            name: "has-decision".to_string(),
            required: true,
            gate_type: GateType::ArtifactExists {
                artifact_type: "Decision".to_string(),
            },
        };
        let result = GateEvaluator::new().check_single(&gate, &wi, &HashSet::new());
        assert_eq!(result, GateResult::Passed);
    }

    #[test]
    fn test_artifact_exists_gate_fails_required() {
        let wi = test_workitem(vec![]);
        let gate = PolicyGate {
            name: "has-decision".to_string(),
            required: true,
            gate_type: GateType::ArtifactExists {
                artifact_type: "Decision".to_string(),
            },
        };
        let result = GateEvaluator::new().check_single(&gate, &wi, &HashSet::new());
        assert!(matches!(result, GateResult::Failed { .. }));
    }

    #[test]
    fn test_artifact_exists_gate_warns_not_required() {
        let wi = test_workitem(vec![]);
        let gate = PolicyGate {
            name: "has-decision".to_string(),
            required: false,
            gate_type: GateType::ArtifactExists {
                artifact_type: "Decision".to_string(),
            },
        };
        let result = GateEvaluator::new().check_single(&gate, &wi, &HashSet::new());
        assert!(matches!(result, GateResult::Warning { .. }));
    }

    #[test]
    fn test_stage_completed_gate_passes() {
        let mut stages = HashSet::new();
        stages.insert("Architecture".to_string());
        let gate = PolicyGate {
            name: "arch-done".to_string(),
            required: true,
            gate_type: GateType::StageCompleted {
                stage: "Architecture".to_string(),
            },
        };
        let result = GateEvaluator::new().check_single(&gate, &test_workitem(vec![]), &stages);
        assert_eq!(result, GateResult::Passed);
    }

    #[test]
    fn test_stage_completed_gate_fails() {
        let gate = PolicyGate {
            name: "arch-done".to_string(),
            required: true,
            gate_type: GateType::StageCompleted {
                stage: "Architecture".to_string(),
            },
        };
        let result =
            GateEvaluator::new().check_single(&gate, &test_workitem(vec![]), &HashSet::new());
        assert!(matches!(result, GateResult::Failed { .. }));
    }

    #[test]
    fn test_manual_approval_always_passes_v0() {
        let gate = PolicyGate {
            name: "approve".to_string(),
            required: true,
            gate_type: GateType::ManualApproval {
                approver: "tech-lead".to_string(),
            },
        };
        let result =
            GateEvaluator::new().check_single(&gate, &test_workitem(vec![]), &HashSet::new());
        assert_eq!(result, GateResult::Passed);
    }

    #[test]
    fn test_custom_condition_always_passes_v0() {
        let gate = PolicyGate {
            name: "custom".to_string(),
            required: true,
            gate_type: GateType::CustomCondition {
                name: "irrelevant".to_string(),
            },
        };
        let result =
            GateEvaluator::new().check_single(&gate, &test_workitem(vec![]), &HashSet::new());
        assert_eq!(result, GateResult::Passed);
    }

    #[test]
    fn test_evaluation_blocks_on_required_failure() {
        let gates = vec![
            PolicyGate {
                name: "needs-artifact".to_string(),
                required: true,
                gate_type: GateType::ArtifactExists {
                    artifact_type: "Missing".to_string(),
                },
            },
            PolicyGate {
                name: "approve".to_string(),
                required: true,
                gate_type: GateType::ManualApproval {
                    approver: "lead".to_string(),
                },
            },
        ];
        let eval = GateEvaluator::new().evaluate(&gates, &test_workitem(vec![]), &HashSet::new());
        assert!(!eval.allowed);
    }

    #[test]
    fn test_evaluation_allows_with_warnings() {
        let gates = vec![
            PolicyGate {
                name: "optional-artifact".to_string(),
                required: false,
                gate_type: GateType::ArtifactExists {
                    artifact_type: "Missing".to_string(),
                },
            },
            PolicyGate {
                name: "approve".to_string(),
                required: true,
                gate_type: GateType::ManualApproval {
                    approver: "lead".to_string(),
                },
            },
        ];
        let eval = GateEvaluator::new().evaluate(&gates, &test_workitem(vec![]), &HashSet::new());
        assert!(eval.allowed);
    }
}
