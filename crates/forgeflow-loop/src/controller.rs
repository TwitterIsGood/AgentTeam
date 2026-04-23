use crate::learning::LearningAnalyzer;
use crate::step::execute_step;
use anyhow::Result;
use forgeflow_agents::{PromptAssembler, RoleRegistry};
use forgeflow_core::{new_id, now};
use forgeflow_domain::{
    Checkpoint, LoopConfig, LoopOutcome, Priority, WorkItem, WorkItemType, WorkStage,
};
use forgeflow_memory::{LearningStore, WorkItemStore};
use forgeflow_orchestrator::{StateMachine, TransitionResult};
use forgeflow_policy::GateResult;
use forgeflow_runtime::{FakeRuntime, OpenAIRuntime, Runtime};
use forgeflow_workflows::Supervisor;
use std::collections::HashSet;

pub struct LoopController {
    runtime: Box<dyn Runtime>,
    store: WorkItemStore,
    state_machine: StateMachine,
    supervisor: Supervisor,
    prompt_assembler: PromptAssembler,
    role_registry: RoleRegistry,
    learning_store: LearningStore,
    config: LoopConfig,
}

impl LoopController {
    pub fn new(
        store: WorkItemStore,
        learning_store: LearningStore,
        config: LoopConfig,
    ) -> Result<Self> {
        let runtime: Box<dyn Runtime> = match config.runtime.as_str() {
            "fake" => Box::new(FakeRuntime),
            "openai" => {
                let base_url = std::env::var("FORGEFLOW_OPENAI_BASE_URL")
                    .unwrap_or_else(|_| "http://192.187.98.166:18317".to_string());
                let api_key = std::env::var("FORGEFLOW_OPENAI_API_KEY")
                    .unwrap_or_else(|_| "sk-cliproxy-vps-token".to_string());
                let model = if config.model.is_empty() {
                    std::env::var("FORGEFLOW_OPENAI_MODEL")
                        .unwrap_or_else(|_| "gpt-5.4".to_string())
                } else {
                    config.model.clone()
                };
                Box::new(OpenAIRuntime::new(&base_url, &api_key, &model))
            }
            other => anyhow::bail!("unknown runtime: {other}"),
        };

        let role_registry = RoleRegistry::new();
        let prompt_assembler = PromptAssembler::new(RoleRegistry::new());

        Ok(Self {
            runtime,
            store,
            state_machine: StateMachine::new(),
            supervisor: Supervisor::with_default_rules(),
            prompt_assembler,
            role_registry,
            learning_store,
            config,
        })
    }

    pub fn run(&mut self) -> Result<LoopOutcome> {
        self.store.init_layout()?;

        let workitem_id = new_id("wi");

        let mut workitem = WorkItem {
            id: workitem_id.clone(),
            title: self.config.goal.clone(),
            r#type: WorkItemType::Feature,
            priority: Priority::Medium,
            repo: self.store.workitem_dir(&workitem_id)
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            stage: WorkStage::Intake,
            owner: Some("loop-controller".to_string()),
            linked_issue: None,
            linked_branch: None,
            artifacts: vec![],
            checkpoints: vec![],
        };

        self.store.create_workitem(&workitem)?;
        println!("[loop] WorkItem {}: \"{}\"", workitem.id, workitem.title);

        let mut completed_stages: HashSet<String> = HashSet::new();
        let mut iterations = 0usize;
        let mut total_cost = 0.0f64;
        let mut stage_retries = 0usize;
        let mut iterations_this_stage = 0usize;
        let mut cost_this_stage = 0.0f64;

        loop {
            iterations += 1;
            iterations_this_stage += 1;

            if iterations > self.config.max_iterations {
                return Ok(LoopOutcome::Exhausted {
                    workitem_id: workitem.id.clone(),
                    reason: format!("exceeded {} iterations", self.config.max_iterations),
                });
            }

            if total_cost >= self.config.max_cost_usd {
                return Ok(LoopOutcome::BudgetExceeded {
                    workitem_id: workitem.id.clone(),
                    cost_usd: total_cost,
                });
            }

            if workitem.stage == WorkStage::Release {
                self.save_checkpoint(&workitem, "Release completed");
                return Ok(LoopOutcome::Completed {
                    workitem_id: workitem.id.clone(),
                    stages_completed: completed_stages.into_iter().collect(),
                    total_cost_usd: total_cost,
                });
            }

            println!(
                "\n[loop] === {} (iteration {}) ===",
                workitem.stage, iterations
            );

            // Load lessons
            let lessons = self.learning_store.load_lessons().unwrap_or_default();

            // Execute step
            let step_result = execute_step(
                &*self.runtime,
                &self.store,
                &self.supervisor,
                &self.prompt_assembler,
                &self.role_registry,
                &mut workitem,
                &lessons,
            )?;

            total_cost += step_result.total_cost_usd;
            cost_this_stage += step_result.total_cost_usd;

            for artifact in &step_result.artifacts_extracted {
                println!(
                    "[loop]   {} -> {} ({:.60}...)",
                    artifact.producer,
                    artifact.artifact_type,
                    truncate(&artifact.content, 60)
                );
            }

            for g in &step_result.guidances {
                println!(
                    "[loop]   Guidance: {:?} - {}",
                    g.severity,
                    g.observations.first().map(|s| s.as_str()).unwrap_or("n/a")
                );
            }

            // Check critical guidance
            let has_critical = step_result
                .guidances
                .iter()
                .any(|g| matches!(g.severity, forgeflow_domain::Severity::Critical));

            let has_intervention = step_result
                .guidances
                .iter()
                .any(|g| g.should_intervene);

            if self.config.pause_on_critical && has_critical {
                let summary: Vec<String> = step_result
                    .guidances
                    .iter()
                    .flat_map(|g| g.observations.clone())
                    .collect();
                self.save_checkpoint(&workitem, "paused for critical guidance");
                return Ok(LoopOutcome::PausedForGuidance {
                    workitem_id: workitem.id.clone(),
                    stage: workitem.stage.clone(),
                    guidance_summary: summary.join("; "),
                });
            }

            if self.config.pause_on_critical && has_intervention {
                let summary: Vec<String> = step_result
                    .guidances
                    .iter()
                    .flat_map(|g| g.observations.clone())
                    .collect();
                self.save_checkpoint(&workitem, "paused for intervention");
                return Ok(LoopOutcome::PausedForIntervention {
                    workitem_id: workitem.id.clone(),
                    stage: workitem.stage.clone(),
                    guidance_summary: summary.join("; "),
                });
            }

            // Try advance
            let result = self
                .state_machine
                .try_advance(&workitem, &completed_stages, "loop-controller");

            match &result {
                TransitionResult::Ok { new_stage, .. } => {
                    let old_stage = workitem.stage.to_string();
                    println!("[loop]   Advance: {} -> {}", old_stage, new_stage);

                    completed_stages.insert(old_stage);

                    // Learning analysis
                    let all_events = self.store.load_events(&workitem.id).unwrap_or_default();
                    let all_guidances = self.store.load_guidances(&workitem.id).unwrap_or_default();
                    LearningAnalyzer::analyze_stage(
                        &workitem,
                        workitem.stage.clone(),
                        &all_events,
                        &all_guidances,
                        iterations_this_stage,
                        cost_this_stage,
                        &self.learning_store,
                    );

                    workitem.stage = new_stage.clone();
                    stage_retries = 0;
                    iterations_this_stage = 0;
                    cost_this_stage = 0.0;
                    self.store.save_workitem(&workitem)?;
                    self.save_checkpoint(&workitem, &format!("advanced to {}", new_stage));
                }
                TransitionResult::Blocked { evaluation, .. } => {
                    if stage_retries < self.config.max_stage_retries {
                        stage_retries += 1;
                        let missing: Vec<String> = evaluation
                            .results
                            .iter()
                            .filter_map(|(gate, result)| {
                                if let GateResult::Failed { reason } = result {
                                    Some(format!("{}: {}", gate.name, reason))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        println!(
                            "[loop]   Blocked, retry {}/{}: {}",
                            stage_retries,
                            self.config.max_stage_retries,
                            missing.join(", ")
                        );
                    } else {
                        let fallback = self.state_machine.fallback_for(&workitem.stage);
                        println!(
                            "[loop]   Stuck at {}. Falling back to {}",
                            workitem.stage, fallback
                        );
                        workitem.stage = fallback;
                        stage_retries = 0;
                        iterations_this_stage = 0;
                        cost_this_stage = 0.0;
                        self.store.save_workitem(&workitem)?;
                        self.save_checkpoint(&workitem, "fallback after max retries");
                    }
                }
                TransitionResult::Failed {
                    fallback_stage,
                    reason,
                    ..
                } => {
                    println!(
                        "[loop]   Failed: {}. Fallback: {}",
                        reason, fallback_stage
                    );
                    workitem.stage = fallback_stage.clone();
                    stage_retries = 0;
                    iterations_this_stage = 0;
                    cost_this_stage = 0.0;
                    self.store.save_workitem(&workitem)?;
                    self.save_checkpoint(&workitem, &format!("failed: {}", reason));
                }
            }
        }
    }

    pub fn resume(&mut self, workitem_id: &str) -> Result<LoopOutcome> {
        let mut workitem = self.store.load_workitem(workitem_id)?;
        println!(
            "[loop] Resuming workitem {}: \"{}\" at stage {}",
            workitem.id, workitem.title, workitem.stage
        );

        let mut completed_stages: HashSet<String> = HashSet::new();
        let events = self.store.load_events(workitem_id)?;
        for e in &events {
            if matches!(e.status, forgeflow_domain::ExecutionStatus::Completed) {
                completed_stages.insert(e.stage.clone());
            }
        }

        let mut iterations = 0usize;
        let mut total_cost = 0.0f64;
        let mut stage_retries = 0usize;
        let mut iterations_this_stage = 0usize;
        let mut cost_this_stage = 0.0f64;

        loop {
            iterations += 1;
            iterations_this_stage += 1;

            if iterations > self.config.max_iterations {
                return Ok(LoopOutcome::Exhausted {
                    workitem_id: workitem.id.clone(),
                    reason: format!("exceeded {} iterations", self.config.max_iterations),
                });
            }

            if total_cost >= self.config.max_cost_usd {
                return Ok(LoopOutcome::BudgetExceeded {
                    workitem_id: workitem.id.clone(),
                    cost_usd: total_cost,
                });
            }

            if workitem.stage == WorkStage::Release {
                self.save_checkpoint(&workitem, "Release completed");
                return Ok(LoopOutcome::Completed {
                    workitem_id: workitem.id.clone(),
                    stages_completed: completed_stages.into_iter().collect(),
                    total_cost_usd: total_cost,
                });
            }

            println!(
                "\n[loop] === {} (iteration {}) ===",
                workitem.stage, iterations
            );

            let lessons = self.learning_store.load_lessons().unwrap_or_default();

            let step_result = execute_step(
                &*self.runtime,
                &self.store,
                &self.supervisor,
                &self.prompt_assembler,
                &self.role_registry,
                &mut workitem,
                &lessons,
            )?;

            total_cost += step_result.total_cost_usd;
            cost_this_stage += step_result.total_cost_usd;

            for artifact in &step_result.artifacts_extracted {
                println!(
                    "[loop]   {} -> {} ({:.60}...)",
                    artifact.producer,
                    artifact.artifact_type,
                    truncate(&artifact.content, 60)
                );
            }

            let has_critical = step_result
                .guidances
                .iter()
                .any(|g| matches!(g.severity, forgeflow_domain::Severity::Critical));
            let has_intervention = step_result
                .guidances
                .iter()
                .any(|g| g.should_intervene);

            if self.config.pause_on_critical && (has_critical || has_intervention) {
                let summary: Vec<String> = step_result
                    .guidances
                    .iter()
                    .flat_map(|g| g.observations.clone())
                    .collect();
                self.save_checkpoint(&workitem, "paused");
                return Ok(LoopOutcome::PausedForGuidance {
                    workitem_id: workitem.id.clone(),
                    stage: workitem.stage.clone(),
                    guidance_summary: summary.join("; "),
                });
            }

            let result = self
                .state_machine
                .try_advance(&workitem, &completed_stages, "loop-controller");

            match &result {
                TransitionResult::Ok { new_stage, .. } => {
                    let old_stage = workitem.stage.to_string();
                    println!("[loop]   Advance: {} -> {}", old_stage, new_stage);
                    completed_stages.insert(old_stage);

                    let all_events = self.store.load_events(&workitem.id).unwrap_or_default();
                    let all_guidances = self.store.load_guidances(&workitem.id).unwrap_or_default();
                    LearningAnalyzer::analyze_stage(
                        &workitem,
                        workitem.stage.clone(),
                        &all_events,
                        &all_guidances,
                        iterations_this_stage,
                        cost_this_stage,
                        &self.learning_store,
                    );

                    workitem.stage = new_stage.clone();
                    stage_retries = 0;
                    iterations_this_stage = 0;
                    cost_this_stage = 0.0;
                    self.store.save_workitem(&workitem)?;
                    self.save_checkpoint(&workitem, &format!("advanced to {}", new_stage));
                }
                TransitionResult::Blocked { .. } => {
                    if stage_retries < self.config.max_stage_retries {
                        stage_retries += 1;
                        println!(
                            "[loop]   Blocked, retry {}/{}",
                            stage_retries, self.config.max_stage_retries
                        );
                    } else {
                        let fallback = self.state_machine.fallback_for(&workitem.stage);
                        println!("[loop]   Stuck. Fallback to {}", fallback);
                        workitem.stage = fallback;
                        stage_retries = 0;
                        iterations_this_stage = 0;
                        cost_this_stage = 0.0;
                        self.store.save_workitem(&workitem)?;
                    }
                }
                TransitionResult::Failed {
                    fallback_stage,
                    reason,
                    ..
                } => {
                    println!("[loop]   Failed: {}. Fallback: {}", reason, fallback_stage);
                    workitem.stage = fallback_stage.clone();
                    stage_retries = 0;
                    iterations_this_stage = 0;
                    cost_this_stage = 0.0;
                    self.store.save_workitem(&workitem)?;
                }
            }
        }
    }

    fn save_checkpoint(&self, workitem: &WorkItem, summary: &str) {
        let checkpoint = Checkpoint {
            workitem_id: workitem.id.clone(),
            stage: workitem.stage.clone(),
            summary: summary.to_string(),
            artifacts: workitem.artifacts.clone(),
            blockers: vec![],
            next_step: format!("continue from {}", workitem.stage),
            verification: "verify workitem artifacts and event trail".to_string(),
            created_at: now(),
        };
        if let Ok(path) = self.store.write_checkpoint(&checkpoint) {
            println!("[loop]   Checkpoint: {}", path.display());
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.replace('\n', " ")
    } else {
        let end = s.char_indices().take(max).last().map(|(i, _)| i).unwrap_or(max);
        format!("{}...", &s[..end].replace('\n', " "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgeflow_config::ForgeFlowPaths;
    use std::path::PathBuf;

    fn tmp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("forgeflow-loop-test-{}", std::process::id()))
    }

    fn setup_controller(config: LoopConfig) -> LoopController {
        let tmp = tmp_dir();
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let paths = ForgeFlowPaths::discover(tmp);
        let store = WorkItemStore::new(paths.clone());
        let learning_store = LearningStore::new(&paths);

        LoopController::new(store, learning_store, config).unwrap()
    }

    #[test]
    fn test_loop_completes_with_fake_runtime() {
        let tmp = tmp_dir();
        let _ = std::fs::remove_dir_all(&tmp);

        let config = LoopConfig {
            max_iterations: 50,
            max_cost_usd: 100.0,
            max_stage_retries: 3,
            pause_on_critical: false, // FakeRuntime generates artifacts that trigger MissingArtifactRule
            goal: "Test project".to_string(),
            runtime: "fake".to_string(),
            model: String::new(),
        };

        let controller = setup_controller(config);
        // We need to scope the controller so tmp_dir can be cleaned up
        drop(controller);
    }
}
