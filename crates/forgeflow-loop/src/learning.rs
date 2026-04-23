use forgeflow_core::{new_id, now};
use forgeflow_domain::{
    ExecutionEvent, ExecutionStatus, Guidance, Lesson, LessonScope, MemoryScope, Severity,
    StageDurationStats, WorkItem, WorkStage,
};
use forgeflow_memory::LearningStore;

pub struct LearningAnalyzer;

impl LearningAnalyzer {
    pub fn analyze_stage(
        _workitem: &WorkItem,
        stage: WorkStage,
        events: &[ExecutionEvent],
        guidances: &[Guidance],
        iterations_this_stage: usize,
        cost_this_stage: f64,
        learning_store: &LearningStore,
    ) -> Vec<Lesson> {
        let mut lessons = Vec::new();

        let stage_events: Vec<_> = events
            .iter()
            .filter(|e| e.stage == stage.to_string())
            .collect();

        let failed_count = stage_events
            .iter()
            .filter(|e| matches!(e.status, ExecutionStatus::Failed))
            .count();

        // Lesson: Stage took many iterations
        if iterations_this_stage > 5 {
            lessons.push(make_lesson(
                LessonScope {
                    variant: MemoryScope::Project,
                    stage: Some(stage.clone()),
                    role: None,
                },
                format!(
                    "Stage {} required {} iterations to complete",
                    stage, iterations_this_stage
                ),
                format!(
                    "Consider providing more explicit guidance in prompts for stage {}",
                    stage
                ),
                format!(
                    "{} events, {} failures, {} guidances generated",
                    stage_events.len(),
                    failed_count,
                    guidances.len()
                ),
            ));
        }

        // Lesson: Repeated guidance patterns
        let critical_guidances: Vec<_> = guidances
            .iter()
            .filter(|g| g.severity == Severity::Critical)
            .collect();

        if critical_guidances.len() >= 2 {
            let observations: Vec<String> = critical_guidances
                .iter()
                .flat_map(|g| g.observations.clone())
                .collect();
            lessons.push(make_lesson(
                LessonScope {
                    variant: MemoryScope::Project,
                    stage: Some(stage.clone()),
                    role: None,
                },
                format!(
                    "Multiple critical guidances in stage {}: {}",
                    stage,
                    observations.join("; ")
                ),
                "Review stage entry criteria and artifact quality expectations".to_string(),
                format!("{} critical guidances generated", critical_guidances.len()),
            ));
        }

        // Lesson: High failure rate
        if failed_count > 3 {
            lessons.push(make_lesson(
                LessonScope {
                    variant: MemoryScope::Project,
                    stage: Some(stage.clone()),
                    role: None,
                },
                format!(
                    "Stage {} had {} execution failures",
                    stage, failed_count
                ),
                "Investigate root cause and consider fallback strategies".to_string(),
                format!("{} failures out of {} events", failed_count, stage_events.len()),
            ));
        }

        // Lesson: Smooth completion
        if iterations_this_stage == 1 && guidances.is_empty() {
            lessons.push(make_lesson(
                LessonScope {
                    variant: MemoryScope::Project,
                    stage: Some(stage.clone()),
                    role: None,
                },
                format!("Stage {} completed smoothly in one iteration", stage),
                format!(
                    "Current approach works well for stage {}. Maintain this pattern.",
                    stage
                ),
                "No guidances, no failures, single iteration".to_string(),
            ));
        }

        // Persist lessons
        for lesson in &lessons {
            let _ = learning_store.save_lesson(lesson);
        }

        // Update stage stats
        let completed_count = stage_events
            .iter()
            .filter(|e| matches!(e.status, ExecutionStatus::Completed))
            .count();
        let success_rate = if stage_events.is_empty() {
            1.0
        } else {
            completed_count as f64 / stage_events.len() as f64
        };

        let stats = StageDurationStats {
            stage,
            sample_count: 1,
            avg_iterations: iterations_this_stage as f64,
            avg_cost_usd: cost_this_stage,
            success_rate,
        };
        let _ = learning_store.save_stage_stats(&stats);

        lessons
    }
}

fn make_lesson(
    scope: LessonScope,
    observation: String,
    recommendation: String,
    evidence: String,
) -> Lesson {
    Lesson {
        lesson_id: new_id("lesson"),
        scope,
        observation,
        recommendation,
        evidence,
        learned_at: now(),
        confidence: 0.6,
    }
}
