use anyhow::Result;
use forgeflow_agents::{ArtifactExtractor, PromptAssembler, RoleRegistry};
use forgeflow_core::{new_id, now};
use forgeflow_domain::{
    ExecutionEvent, ExecutionStatus, ExtractedArtifact, Guidance, Lesson, WorkItem,
};
use forgeflow_memory::WorkItemStore;
use forgeflow_runtime::Runtime;
use forgeflow_workflows::Supervisor;

pub struct StepResult {
    pub events: Vec<ExecutionEvent>,
    pub guidances: Vec<Guidance>,
    pub artifacts_extracted: Vec<ExtractedArtifact>,
    pub total_tokens: u32,
    pub total_cost_usd: f64,
}

pub fn execute_step(
    runtime: &dyn Runtime,
    store: &WorkItemStore,
    supervisor: &Supervisor,
    prompt_assembler: &PromptAssembler,
    role_registry: &RoleRegistry,
    workitem: &mut WorkItem,
    lessons: &[Lesson],
) -> Result<StepResult> {
    let stage_str = workitem.stage.to_string();
    let contracts = role_registry.contracts_for_stage(&stage_str);

    let events = store.load_events(&workitem.id)?;
    let existing_guidances = store.load_guidances(&workitem.id)?;

    let mut step_events = Vec::new();
    let mut step_artifacts = Vec::new();
    let mut total_tokens = 0u32;
    let mut total_cost = 0.0f64;

    for contract in &contracts {
        let mut artifact_contents = Vec::new();
        for input_type in &contract.input_artifact_types {
            if let Some(content) = store.load_artifact_content(&workitem.id, input_type)? {
                artifact_contents.push((input_type.clone(), content));
            }
        }

        let prompt = prompt_assembler.assemble(
            &contract.role,
            workitem,
            &stage_str,
            &artifact_contents,
            &events,
            &existing_guidances,
            lessons,
        );

        let request = forgeflow_runtime::ExecutionRequest {
            actor: contract.role.clone(),
            instruction: format!("{}\n\n{}", prompt.system_prompt, prompt.user_message),
        };

        let response = runtime.execute(request);
        total_tokens += response.tokens;
        total_cost += response.estimated_cost_usd;

        let expected_outputs: Vec<String> = contract.output_artifact_types.clone();
        let extracted = ArtifactExtractor::extract(
            &response.output,
            &expected_outputs,
            &contract.role,
            &workitem.id,
        );

        for artifact in &extracted {
            let file_name = format!("{}.md", artifact.artifact_id);
            store.write_artifact_file(&workitem.id, &file_name, &artifact.content)?;

            if !workitem.artifacts.iter().any(|a| a.contains(&artifact.artifact_type)) {
                workitem.artifacts.push(artifact.artifact_type.clone());
            }
        }
        step_artifacts.extend(extracted);

        let event = ExecutionEvent {
            event_id: new_id("evt"),
            workitem_id: workitem.id.clone(),
            stage: stage_str.clone(),
            actor: contract.role.clone(),
            action: response.output.clone(),
            timestamp: now(),
            input_refs: artifact_contents.iter().map(|(n, _)| n.clone()).collect(),
            output_refs: step_artifacts.iter().map(|a| a.artifact_id.clone()).collect(),
            status: ExecutionStatus::Completed,
        };
        step_events.push(event);
    }

    store.save_workitem(workitem)?;

    // Persist events
    if !step_events.is_empty() {
        let file_name = format!("{}-step.json", now().format("%Y%m%dT%H%M%SZ"));
        let json = serde_json::to_string_pretty(&step_events)?;
        store.append_event_json(&workitem.id, &file_name, &json)?;
    }

    // Supervision review
    let all_events: Vec<ExecutionEvent> = events
        .iter()
        .chain(step_events.iter())
        .cloned()
        .collect();
    let all_guidances: Vec<Guidance> = existing_guidances.clone();

    let new_guidances = supervisor.review_stage(workitem, &all_events, &all_guidances);
    for g in &new_guidances {
        store.write_guidance(g)?;
    }

    Ok(StepResult {
        events: step_events,
        guidances: new_guidances,
        artifacts_extracted: step_artifacts,
        total_tokens,
        total_cost_usd: total_cost,
    })
}
