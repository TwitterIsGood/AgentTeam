use forgeflow_core::Timestamp;
use forgeflow_domain::{ExecutionEvent, ExecutionStatus, Guidance, Severity, WorkItem, WorkStage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Replay ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySummary {
    pub workitem_id: String,
    pub workitem_title: String,
    pub current_stage: WorkStage,
    pub event_count: usize,
    pub guidance_count: usize,
    pub stage_timeline: Vec<StageSegment>,
    pub actor_summary: Vec<ActorStat>,
    pub total_duration_minutes: Option<i64>,
    pub failed_event_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSegment {
    pub stage: String,
    pub event_count: usize,
    pub first_event_at: Option<Timestamp>,
    pub last_event_at: Option<Timestamp>,
    pub duration_minutes: Option<i64>,
    pub status: StageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StageStatus {
    Active,
    Completed,
    Blocked,
    NotStarted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorStat {
    pub actor: String,
    pub event_count: usize,
    pub completed: usize,
    pub failed: usize,
}

pub fn build_replay(
    workitem: &WorkItem,
    events: &[ExecutionEvent],
    guidances: &[Guidance],
) -> ReplaySummary {
    let stage_timeline = build_stage_timeline(events);
    let actor_summary = build_actor_summary(events);
    let total_duration = compute_total_duration(events);
    let failed_event_count = events
        .iter()
        .filter(|e| matches!(e.status, ExecutionStatus::Failed))
        .count();

    ReplaySummary {
        workitem_id: workitem.id.clone(),
        workitem_title: workitem.title.clone(),
        current_stage: workitem.stage.clone(),
        event_count: events.len(),
        guidance_count: guidances.len(),
        stage_timeline,
        actor_summary,
        total_duration_minutes: total_duration,
        failed_event_count,
    }
}

fn build_stage_timeline(events: &[ExecutionEvent]) -> Vec<StageSegment> {
    let mut by_stage: HashMap<String, Vec<&ExecutionEvent>> = HashMap::new();
    for e in events {
        by_stage
            .entry(e.stage.clone())
            .or_default()
            .push(e);
    }

    let stage_order = [
        "Intake",
        "Roundtable",
        "Architecture",
        "Implement",
        "Test",
        "Review",
        "PR",
        "Release",
    ];

    let mut segments = Vec::new();

    for stage in &stage_order {
        let stage_events = match by_stage.get(*stage) {
            Some(evts) => evts,
            None => continue,
        };

        let first_ts = stage_events.iter().map(|e| e.timestamp).min();
        let last_ts = stage_events.iter().map(|e| e.timestamp).max();
        let duration = match (first_ts, last_ts) {
            (Some(f), Some(l)) => Some(l.signed_duration_since(f).num_minutes()),
            _ => None,
        };

        let has_active = stage_events
            .iter()
            .any(|e| matches!(e.status, ExecutionStatus::Started));
        let all_completed = stage_events
            .iter()
            .all(|e| matches!(e.status, ExecutionStatus::Completed));
        let has_failed = stage_events
            .iter()
            .any(|e| matches!(e.status, ExecutionStatus::Failed));

        let status = if has_active {
            StageStatus::Active
        } else if has_failed {
            StageStatus::Blocked
        } else if all_completed {
            StageStatus::Completed
        } else {
            StageStatus::NotStarted
        };

        segments.push(StageSegment {
            stage: stage.to_string(),
            event_count: stage_events.len(),
            first_event_at: first_ts,
            last_event_at: last_ts,
            duration_minutes: duration,
            status,
        });
    }

    segments
}

fn build_actor_summary(events: &[ExecutionEvent]) -> Vec<ActorStat> {
    let mut by_actor: HashMap<String, (usize, usize, usize)> = HashMap::new();
    for e in events {
        let (total, completed, failed) = by_actor
            .entry(e.actor.clone())
            .or_insert((0, 0, 0));
        *total += 1;
        if matches!(e.status, ExecutionStatus::Completed) {
            *completed += 1;
        }
        if matches!(e.status, ExecutionStatus::Failed) {
            *failed += 1;
        }
    }

    let mut stats: Vec<ActorStat> = by_actor
        .into_iter()
        .map(|(actor, (total, completed, failed))| ActorStat {
            actor,
            event_count: total,
            completed,
            failed,
        })
        .collect();
    stats.sort_by(|a, b| b.event_count.cmp(&a.event_count));
    stats
}

fn compute_total_duration(events: &[ExecutionEvent]) -> Option<i64> {
    let min_ts = events.iter().map(|e| e.timestamp).min()?;
    let max_ts = events.iter().map(|e| e.timestamp).max()?;
    Some(max_ts.signed_duration_since(min_ts).num_minutes())
}

// --- Event trail formatting ---

pub fn format_event_trail(events: &[ExecutionEvent]) -> String {
    let mut lines = Vec::new();
    for e in events {
        let status_icon = match &e.status {
            ExecutionStatus::Completed => "+",
            ExecutionStatus::Failed => "x",
            ExecutionStatus::Started => ">",
            ExecutionStatus::Skipped => "-",
        };
        let action_preview: String = e.action.chars().take(80).collect();
        lines.push(format!(
            "[{}] {} | {} | {} | {}",
            status_icon, e.timestamp.format("%Y-%m-%d %H:%M"), e.stage, e.actor, action_preview
        ));
    }
    lines.join("\n")
}

// --- Guidance digest ---

pub fn format_guidance_digest(guidances: &[Guidance]) -> String {
    if guidances.is_empty() {
        return "No guidance records.".to_string();
    }

    let mut lines = Vec::new();
    for g in guidances {
        let severity_label = match g.severity {
            Severity::Critical => "CRIT",
            Severity::Warning => "WARN",
            Severity::Info => "INFO",
        };
        let intervene = if g.should_intervene { " [INTERVENE]" } else { "" };
        lines.push(format!(
            "[{}] {:?} - stage {:?}{}",
            severity_label, g.assessment, g.stage, intervene
        ));
        for obs in &g.observations {
            lines.push(format!("  * {obs}"));
        }
        for sug in &g.suggestions {
            lines.push(format!("  > {sug}"));
        }
    }
    lines.join("\n")
}

// --- Metrics ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub workitem_id: String,
    pub total_events: usize,
    pub completed_events: usize,
    pub failed_events: usize,
    pub skipped_events: usize,
    pub success_rate: f64,
    pub stages_touched: Vec<String>,
    pub actor_count: usize,
    pub estimated_duration_minutes: Option<i64>,
}

impl ExecutionMetrics {
    pub fn from_events(workitem_id: &str, events: &[ExecutionEvent]) -> Self {
        let completed = events
            .iter()
            .filter(|e| matches!(e.status, ExecutionStatus::Completed))
            .count();
        let failed = events
            .iter()
            .filter(|e| matches!(e.status, ExecutionStatus::Failed))
            .count();
        let skipped = events
            .iter()
            .filter(|e| matches!(e.status, ExecutionStatus::Skipped))
            .count();

        let success_rate = if events.is_empty() {
            0.0
        } else {
            completed as f64 / events.len() as f64
        };

        let mut stages: Vec<String> = events
            .iter()
            .map(|e| e.stage.clone())
            .collect();
        stages.sort();
        stages.dedup();

        let actors: std::collections::HashSet<&str> =
            events.iter().map(|e| e.actor.as_str()).collect();

        let estimated_duration = compute_total_duration(events);

        Self {
            workitem_id: workitem_id.to_string(),
            total_events: events.len(),
            completed_events: completed,
            failed_events: failed,
            skipped_events: skipped,
            success_rate,
            stages_touched: stages,
            actor_count: actors.len(),
            estimated_duration_minutes: estimated_duration,
        }
    }
}

// --- Health snapshot ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemHealth {
    pub workitem_id: String,
    pub stage: WorkStage,
    pub is_healthy: bool,
    pub issues: Vec<String>,
}

pub fn assess_health(
    workitem: &WorkItem,
    events: &[ExecutionEvent],
    guidances: &[Guidance],
) -> WorkItemHealth {
    let mut issues = Vec::new();

    let recent_failures: Vec<_> = events
        .iter()
        .filter(|e| {
            e.stage == workitem.stage.to_string()
                && matches!(e.status, ExecutionStatus::Failed)
        })
        .collect();

    if recent_failures.len() >= 3 {
        issues.push(format!(
            "Stage {} has {} consecutive failures",
            workitem.stage,
            recent_failures.len()
        ));
    }

    let critical_guidances: Vec<_> = guidances
        .iter()
        .filter(|g| g.severity == Severity::Critical && g.should_intervene)
        .collect();

    if !critical_guidances.is_empty() {
        issues.push(format!(
            "{} critical guidance(s) requiring intervention",
            critical_guidances.len()
        ));
    }

    WorkItemHealth {
        workitem_id: workitem.id.clone(),
        stage: workitem.stage.clone(),
        is_healthy: issues.is_empty(),
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgeflow_core::now;
    use forgeflow_domain::{Priority, SupervisionAssessment, WorkItemType};

    fn test_workitem(stage: WorkStage) -> WorkItem {
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
            artifacts: vec![],
            checkpoints: vec![],
        }
    }

    fn test_event(stage: &str, actor: &str, status: ExecutionStatus) -> ExecutionEvent {
        ExecutionEvent {
            event_id: forgeflow_core::new_id("evt"),
            workitem_id: "wi-test".to_string(),
            stage: stage.to_string(),
            actor: actor.to_string(),
            action: "did something".to_string(),
            timestamp: now(),
            input_refs: vec![],
            output_refs: vec![],
            status,
        }
    }

    #[test]
    fn test_replay_summary_counts_events() {
        let wi = test_workitem(WorkStage::Implement);
        let events = vec![
            test_event("Implement", "Coder", ExecutionStatus::Completed),
            test_event("Implement", "Coder", ExecutionStatus::Completed),
            test_event("Test", "Tester", ExecutionStatus::Failed),
        ];
        let replay = build_replay(&wi, &events, &[]);
        assert_eq!(replay.event_count, 3);
        assert_eq!(replay.failed_event_count, 1);
        assert_eq!(replay.stage_timeline.len(), 2);
    }

    #[test]
    fn test_actor_summary_aggregation() {
        let events = vec![
            test_event("Implement", "Coder", ExecutionStatus::Completed),
            test_event("Implement", "Coder", ExecutionStatus::Failed),
            test_event("Test", "Tester", ExecutionStatus::Completed),
        ];
        let summary = build_actor_summary(&events);
        assert_eq!(summary.len(), 2);
        let coder = summary.iter().find(|s| s.actor == "Coder").unwrap();
        assert_eq!(coder.event_count, 2);
        assert_eq!(coder.completed, 1);
        assert_eq!(coder.failed, 1);
    }

    #[test]
    fn test_execution_metrics_success_rate() {
        let events = vec![
            test_event("Implement", "Coder", ExecutionStatus::Completed),
            test_event("Implement", "Coder", ExecutionStatus::Completed),
            test_event("Test", "Tester", ExecutionStatus::Failed),
        ];
        let metrics = ExecutionMetrics::from_events("wi-test", &events);
        assert_eq!(metrics.total_events, 3);
        assert_eq!(metrics.completed_events, 2);
        assert!((metrics.success_rate - 0.6667).abs() < 0.01);
    }

    #[test]
    fn test_health_assessment_healthy() {
        let wi = test_workitem(WorkStage::Implement);
        let events = vec![
            test_event("Implement", "Coder", ExecutionStatus::Completed),
        ];
        let health = assess_health(&wi, &events, &[]);
        assert!(health.is_healthy);
    }

    #[test]
    fn test_health_assessment_unhealthy_on_failures() {
        let wi = test_workitem(WorkStage::Implement);
        let events: Vec<ExecutionEvent> = (0..4)
            .map(|_| test_event("Implement", "Coder", ExecutionStatus::Failed))
            .collect();
        let health = assess_health(&wi, &events, &[]);
        assert!(!health.is_healthy);
        assert!(!health.issues.is_empty());
    }

    #[test]
    fn test_format_event_trail() {
        let events = vec![
            test_event("Implement", "Coder", ExecutionStatus::Completed),
            test_event("Test", "Tester", ExecutionStatus::Failed),
        ];
        let trail = format_event_trail(&events);
        assert!(trail.contains("[+]"));
        assert!(trail.contains("[x]"));
    }

    #[test]
    fn test_format_guidance_digest_empty() {
        let digest = format_guidance_digest(&[]);
        assert_eq!(digest, "No guidance records.");
    }

    #[test]
    fn test_format_guidance_digest_with_records() {
        let guidances = vec![Guidance {
            guidance_id: "g-1".to_string(),
            workitem_id: "wi-test".to_string(),
            stage: WorkStage::Implement,
            assessment: SupervisionAssessment::Stuck,
            observations: vec!["no output".to_string()],
            suggestions: vec!["break down work".to_string()],
            severity: Severity::Critical,
            should_intervene: true,
            created_at: now(),
        }];
        let digest = format_guidance_digest(&guidances);
        assert!(digest.contains("CRIT"));
        assert!(digest.contains("INTERVENE"));
    }
}
