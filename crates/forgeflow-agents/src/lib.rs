use serde::{Deserialize, Serialize};
use forgeflow_domain::{
    AssembledPrompt, ExecutionEvent, ExtractedArtifact, Guidance, Lesson, WorkItem,
};

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

// --- Prompt Assembler ---

pub struct PromptAssembler {
    registry: RoleRegistry,
}

impl PromptAssembler {
    pub fn new(registry: RoleRegistry) -> Self {
        Self { registry }
    }

    pub fn assemble(
        &self,
        role: &str,
        workitem: &WorkItem,
        stage: &str,
        artifact_contents: &[(String, String)],
        recent_events: &[ExecutionEvent],
        guidances: &[Guidance],
        lessons: &[Lesson],
    ) -> AssembledPrompt {
        let contract = self
            .registry
            .contracts
            .iter()
            .find(|c| c.role == role)
            .expect("unknown role");

        let profile = default_roles()
            .into_iter()
            .find(|p| p.name == role)
            .expect("unknown role profile");

        let output_types = contract.output_artifact_types.join(", ");

        let system_prompt = format!(
            "你是 {role}，ForgeFlow 软件交付团队的 AI agent。\n\
             职责：{responsibility}。\n\
             正在处理工作项 '{title}'，当前阶段：{stage}。\n\
             你需要产出以下工件：{output_types}。\n\
             请用 markdown 格式输出，每个工件以 ## 工件名 开头。",
            role = role,
            responsibility = profile.responsibility,
            title = workitem.title,
            stage = stage,
            output_types = output_types,
        );

        let mut user_message = stage_user_message(stage, &workitem.title, artifact_contents);

        if !recent_events.is_empty() {
            let event_summary: Vec<String> = recent_events
                .iter()
                .rev()
                .take(5)
                .map(|e| format!("- [{}] {}: {:.100}", e.actor, e.stage, truncate(&e.action, 100)))
                .collect();
            user_message.push_str(&format!(
                "\n\n最近的执行记录：\n{}",
                event_summary.join("\n")
            ));
        }

        if !guidances.is_empty() {
            let guidance_notes: Vec<String> = guidances
                .iter()
                .filter(|g| g.should_intervene || matches!(g.severity, forgeflow_domain::Severity::Critical))
                .flat_map(|g| g.observations.clone())
                .take(3)
                .collect();
            if !guidance_notes.is_empty() {
                user_message.push_str(&format!(
                    "\n\n⚠️ X2 Manager 注意到：\n{}",
                    guidance_notes
                        .iter()
                        .map(|n| format!("- {n}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }

        let relevant_lessons: Vec<&Lesson> = lessons
            .iter()
            .filter(|l| {
                l.scope.stage.as_ref().is_none_or(|s| s.to_string() == stage)
                    || l.scope.role.as_ref().is_none_or(|r| r == role)
            })
            .take(3)
            .collect();

        if !relevant_lessons.is_empty() {
            user_message.push_str("\n\n以往执行的经验教训：");
            for lesson in relevant_lessons {
                user_message.push_str(&format!(
                    "\n- {}: {}",
                    lesson.observation, lesson.recommendation
                ));
            }
        }

        AssembledPrompt {
            system_prompt,
            user_message,
        }
    }
}

fn stage_user_message(
    stage: &str,
    title: &str,
    artifact_contents: &[(String, String)],
) -> String {
    let artifact_block = if artifact_contents.is_empty() {
        String::new()
    } else {
        let blocks: Vec<String> = artifact_contents
            .iter()
            .map(|(name, content)| format!("### {}\n```\n{}\n```", name, truncate(content, 2000)))
            .collect();
        format!("\n\n已有工件：\n{}", blocks.join("\n\n"))
    };

    match stage {
        "Intake" => format!(
            "分析以下需求，产出 Position 文档。Position 应包含：目标、范围、价值、边界。\n\n需求：{}{}",
            title, artifact_block
        ),
        "Roundtable" => format!(
            "基于已有的 Position 文档进行圆桌讨论。Product 产出 Critique，Architect 产出 Decision。\n\
             Critique 应分析 Position 的优劣。Decision 应给出最终决策和理由。{}",
            artifact_block
        ),
        "Architecture" => format!(
            "基于 Decision 设计系统架构。产出 Architecture 文档，包含：模块划分、接口定义、数据模型、关键技术选型。{}",
            artifact_block
        ),
        "Implement" => format!(
            "基于 Architecture 文档实现代码变更。产出 ChangeSet，包含：具体文件修改、代码片段、修改说明。{}",
            artifact_block
        ),
        "Test" => format!(
            "基于 ChangeSet 编写测试。产出 TestReport，包含：测试用例、预期结果、覆盖范围。{}",
            artifact_block
        ),
        "Review" => format!(
            "审查 TestReport 和 ChangeSet。产出 Review 文档，包含：质量评估、安全审查、性能影响、发布建议。{}",
            artifact_block
        ),
        "PR" => format!(
            "基于 Review 结论准备发布。产出 PullRequest 描述，包含：变更摘要、测试覆盖、发布检查清单。{}",
            artifact_block
        ),
        "Release" => format!(
            "执行发布流程。产出 Release Notes，包含：版本号、变更列表、已知问题、升级指南。{}",
            artifact_block
        ),
        _ => format!("处理工作项：{}{}", title, artifact_block),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let end = s.char_indices().take(max).last().map(|(i, _)| i).unwrap_or(max);
        format!("{}...", &s[..end])
    }
}

// --- Artifact Extractor ---

pub struct ArtifactExtractor;

impl ArtifactExtractor {
    pub fn extract(
        response_text: &str,
        expected_types: &[String],
        producer: &str,
        workitem_id: &str,
    ) -> Vec<ExtractedArtifact> {
        let mut artifacts = Vec::new();

        for artifact_type in expected_types {
            if let Some(content) = extract_markdown_section(response_text, artifact_type) {
                let artifact_id = format!(
                    "{}-{}-{}",
                    artifact_type,
                    workitem_id,
                    forgeflow_core::now().format("%Y%m%d%H%M%S")
                );
                artifacts.push(ExtractedArtifact {
                    artifact_id,
                    artifact_type: artifact_type.clone(),
                    content,
                    producer: producer.to_string(),
                });
            }
        }

        // Fallback: if no sections found, use entire response as first expected type
        if artifacts.is_empty() {
            if let Some(primary_type) = expected_types.first() {
                let artifact_id = format!(
                    "{}-{}-{}",
                    primary_type,
                    workitem_id,
                    forgeflow_core::now().format("%Y%m%d%H%M%S")
                );
                artifacts.push(ExtractedArtifact {
                    artifact_id,
                    artifact_type: primary_type.clone(),
                    content: response_text.to_string(),
                    producer: producer.to_string(),
                });
            }
        }

        artifacts
    }
}

fn extract_markdown_section(text: &str, section_name: &str) -> Option<String> {
    let heading_variants = vec![
        format!("## {}", section_name),
        format!("## {}", capitalize_first(section_name)),
        format!("### {}", section_name),
        format!("# {}", section_name),
    ];

    for heading in &heading_variants {
        if let Some(start) = text.find(heading.as_str()) {
            let content_start = start + heading.len();
            let content = find_next_section_or_end(text, content_start);
            return Some(content.trim().to_string());
        }
    }
    None
}

fn find_next_section_or_end(text: &str, from: usize) -> String {
    let rest = &text[from..];
    let lines: Vec<&str> = rest.lines().collect();

    let mut end = rest.len();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 && (line.starts_with("## ") || line.starts_with("# ")) {
            let prefix_len: usize = lines[..i].iter().map(|l| l.len() + 1).sum();
            end = prefix_len;
            break;
        }
    }

    rest[..end].trim_end().to_string()
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
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
    use forgeflow_domain::WorkStage;

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

    #[test]
    fn test_prompt_assembler_intake() {
        let registry = RoleRegistry::new();
        let assembler = PromptAssembler::new(registry);
        let workitem = test_workitem();
        let prompt = assembler.assemble(
            "Router",
            &workitem,
            "Intake",
            &[],
            &[],
            &[],
            &[],
        );
        assert!(prompt.system_prompt.contains("Router"));
        assert!(prompt.user_message.contains("Position"));
        assert!(prompt.system_prompt.contains("需求分析"));
    }

    #[test]
    fn test_prompt_assembler_with_artifacts() {
        let registry = RoleRegistry::new();
        let assembler = PromptAssembler::new(registry);
        let workitem = test_workitem();
        let prompt = assembler.assemble(
            "Architect",
            &workitem,
            "Architecture",
            &[(/*"Decision"*/"Decision".to_string(), "Use microservices".to_string())],
            &[],
            &[],
            &[],
        );
        assert!(prompt.user_message.contains("Decision"));
        assert!(prompt.user_message.contains("microservices"));
    }

    #[test]
    fn test_prompt_assembler_with_lessons() {
        let registry = RoleRegistry::new();
        let assembler = PromptAssembler::new(registry);
        let workitem = test_workitem();
        let lessons = vec![Lesson {
            lesson_id: "l-1".to_string(),
            scope: forgeflow_domain::LessonScope {
                variant: forgeflow_domain::MemoryScope::Project,
                stage: Some(WorkStage::Implement),
                role: None,
            },
            observation: "Implement stage often stalls without clear interfaces".to_string(),
            recommendation: "Ensure Architecture doc defines all interfaces".to_string(),
            evidence: "3 past occurrences".to_string(),
            learned_at: forgeflow_core::now(),
            confidence: 0.8,
        }];
        let prompt = assembler.assemble(
            "Coder",
            &workitem,
            "Implement",
            &[],
            &[],
            &[],
            &lessons,
        );
        assert!(prompt.user_message.contains("经验教训"));
        assert!(prompt.user_message.contains("interfaces"));
    }

    #[test]
    fn test_artifact_extractor_with_markers() {
        let response = "Some intro\n\n## Position\nThis is the position content.\n\n## Other\nIgnored.";
        let artifacts = ArtifactExtractor::extract(
            response,
            &["Position".to_string()],
            "Router",
            "wi-001",
        );
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].artifact_type, "Position");
        assert!(artifacts[0].content.contains("position content"));
    }

    #[test]
    fn test_artifact_extractor_fallback() {
        let response = "This is just plain text without markers.";
        let artifacts = ArtifactExtractor::extract(
            response,
            &["Position".to_string()],
            "Router",
            "wi-001",
        );
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].artifact_type, "Position");
        assert_eq!(artifacts[0].content, response);
    }

    #[test]
    fn test_artifact_extractor_multiple() {
        let response = "## Position\nPos content\n\n## Critique\nCritique content";
        let artifacts = ArtifactExtractor::extract(
            response,
            &["Position".to_string(), "Critique".to_string()],
            "Product",
            "wi-001",
        );
        assert_eq!(artifacts.len(), 2);
    }

    fn test_workitem() -> forgeflow_domain::WorkItem {
        forgeflow_domain::WorkItem {
            id: "wi-test".to_string(),
            title: "需求分析".to_string(),
            r#type: forgeflow_domain::WorkItemType::Feature,
            priority: forgeflow_domain::Priority::Medium,
            repo: "test".to_string(),
            stage: forgeflow_domain::WorkStage::Intake,
            owner: None,
            linked_issue: None,
            linked_branch: None,
            artifacts: vec![],
            checkpoints: vec![],
        }
    }
}
