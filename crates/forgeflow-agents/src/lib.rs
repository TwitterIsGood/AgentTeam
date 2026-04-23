use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub name: &'static str,
    pub responsibility: &'static str,
}

pub fn default_roles() -> Vec<AgentProfile> {
    vec![
        AgentProfile {
            name: "Router",
            responsibility: "route work into the correct workflow",
        },
        AgentProfile {
            name: "Product",
            responsibility: "clarify scope and delivery value",
        },
        AgentProfile {
            name: "Architect",
            responsibility: "shape boundaries and design",
        },
        AgentProfile {
            name: "Coder",
            responsibility: "implement bounded changes",
        },
        AgentProfile {
            name: "Tester",
            responsibility: "verify expected behavior",
        },
        AgentProfile {
            name: "Reviewer",
            responsibility: "assess quality and release readiness",
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleContract {
    pub role: String,
    pub responsible_stages: Vec<String>,
    pub input_artifact_types: Vec<String>,
    pub output_artifact_types: Vec<String>,
}

pub struct RoleRegistry {
    contracts: Vec<RoleContract>,
}

impl RoleRegistry {
    pub fn new() -> Self {
        Self {
            contracts: Self::default_contracts(),
        }
    }

    pub fn contracts_for_stage(&self, stage: &str) -> Vec<&RoleContract> {
        self.contracts
            .iter()
            .filter(|c| c.responsible_stages.iter().any(|s| s == stage))
            .collect()
    }

    pub fn expected_output_artifacts(&self, stage: &str) -> Vec<String> {
        self.contracts
            .iter()
            .filter(|c| c.responsible_stages.iter().any(|s| s == stage))
            .flat_map(|c| c.output_artifact_types.clone())
            .collect()
    }

    fn default_contracts() -> Vec<RoleContract> {
        vec![
            RoleContract {
                role: "Router".to_string(),
                responsible_stages: vec!["Intake".to_string()],
                input_artifact_types: vec![],
                output_artifact_types: vec!["Position".to_string()],
            },
            RoleContract {
                role: "Product".to_string(),
                responsible_stages: vec!["Intake".to_string(), "Roundtable".to_string()],
                input_artifact_types: vec!["Position".to_string()],
                output_artifact_types: vec!["Position".to_string(), "Critique".to_string()],
            },
            RoleContract {
                role: "Architect".to_string(),
                responsible_stages: vec!["Roundtable".to_string(), "Architecture".to_string()],
                input_artifact_types: vec!["Position".to_string(), "Critique".to_string()],
                output_artifact_types: vec!["Decision".to_string(), "Architecture".to_string()],
            },
            RoleContract {
                role: "Coder".to_string(),
                responsible_stages: vec!["Implement".to_string()],
                input_artifact_types: vec!["Decision".to_string(), "Architecture".to_string()],
                output_artifact_types: vec!["ChangeSet".to_string()],
            },
            RoleContract {
                role: "Tester".to_string(),
                responsible_stages: vec!["Test".to_string()],
                input_artifact_types: vec!["ChangeSet".to_string()],
                output_artifact_types: vec!["TestReport".to_string()],
            },
            RoleContract {
                role: "Reviewer".to_string(),
                responsible_stages: vec!["Review".to_string()],
                input_artifact_types: vec!["TestReport".to_string(), "ChangeSet".to_string()],
                output_artifact_types: vec!["Review".to_string()],
            },
        ]
    }
}

impl Default for RoleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInput {
    pub role: String,
    pub instruction: String,
    pub artifact_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentOutput {
    pub role: String,
    pub action_taken: String,
    pub output_refs: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_six_contracts() {
        let registry = RoleRegistry::new();
        assert_eq!(registry.contracts.len(), 6);
    }

    #[test]
    fn test_contracts_for_implement() {
        let registry = RoleRegistry::new();
        let contracts = registry.contracts_for_stage("Implement");
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].role, "Coder");
    }

    #[test]
    fn test_expected_output_for_roundtable() {
        let registry = RoleRegistry::new();
        let outputs = registry.expected_output_artifacts("Roundtable");
        assert!(outputs.contains(&"Decision".to_string()));
        assert!(outputs.contains(&"Architecture".to_string()));
    }

    #[test]
    fn test_every_role_has_output_artifacts() {
        let registry = RoleRegistry::new();
        for contract in &registry.contracts {
            assert!(
                !contract.output_artifact_types.is_empty(),
                "role {} has no output artifacts",
                contract.role
            );
        }
    }
}
