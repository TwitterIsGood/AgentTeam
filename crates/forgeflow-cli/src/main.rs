use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use forgeflow_agents::default_roles;
use forgeflow_config::ForgeFlowPaths;
use forgeflow_core::now;
use forgeflow_domain::{Checkpoint, Priority, WorkItem, WorkItemType, WorkStage};
use forgeflow_memory::WorkItemStore;
use forgeflow_observability::{build_replay, format_event_trail, format_guidance_digest, assess_health, ExecutionMetrics};
use forgeflow_orchestrator::{StateMachine, TransitionResult, resume as orchestrator_resume};
use forgeflow_policy::GateResult;
use forgeflow_repo::{GitRepo, PullRequestSpec, IssueSpec, RepoOps, workitem_branch_name, repo_status_for_workitem};
use forgeflow_runtime::{FakeRuntime, OpenAIRuntime, Runtime};
use forgeflow_workflows::{dry_run_workflow, review_stage_guidance};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "forgeflow")]
#[command(about = "ForgeFlow CLI")]
struct Cli {
    #[arg(long, default_value = ".")]
    repo_root: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Init,
    Doctor,
    Workitem {
        #[command(subcommand)]
        command: WorkitemCommand,
    },
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommand,
    },
    Replay {
        #[arg(long)]
        id: String,
    },
    Metrics {
        #[arg(long)]
        id: String,
    },
    Health {
        #[arg(long)]
        id: String,
    },
    Repo {
        #[command(subcommand)]
        command: RepoCommand,
    },
}

#[derive(Subcommand, Debug)]
enum WorkitemCommand {
    Create(CreateWorkitemArgs),
    Status(StatusArgs),
    Advance(AdvanceArgs),
    Resume(ResumeArgs),
    Review(ReviewArgs),
    Guidance(GuidanceArgs),
}

#[derive(Subcommand, Debug)]
enum MemoryCommand {
    Checkpoint(CheckpointArgs),
}

#[derive(Subcommand, Debug)]
enum WorkflowCommand {
    Run(RunWorkflowArgs),
}

#[derive(Subcommand, Debug)]
enum RepoCommand {
    Status,
    Branch {
        #[arg(long)]
        id: String,
    },
    Pr {
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "main")]
        base: String,
        #[arg(long)]
        dry_run: bool,
    },
    Issue {
        #[arg(long)]
        id: String,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Args, Debug)]
struct CreateWorkitemArgs {
    #[arg(long)]
    id: String,
    #[arg(long)]
    title: String,
    #[arg(long, value_enum, default_value_t = WorkItemTypeArg::Feature)]
    kind: WorkItemTypeArg,
    #[arg(long, value_enum, default_value_t = PriorityArg::Medium)]
    priority: PriorityArg,
    #[arg(long)]
    owner: Option<String>,
    #[arg(long)]
    linked_issue: Option<String>,
    #[arg(long)]
    linked_branch: Option<String>,
}

#[derive(Args, Debug)]
struct StatusArgs {
    #[arg(long)]
    id: String,
}

#[derive(Args, Debug)]
struct CheckpointArgs {
    #[arg(long)]
    id: String,
    #[arg(long, value_enum)]
    stage: WorkStageArg,
    #[arg(long)]
    summary: String,
    #[arg(long, default_value = "")]
    blockers: String,
    #[arg(long, default_value = "continue with the next artifact")]
    next_step: String,
    #[arg(long, default_value = "verify workitem artifacts and event trail")]
    verification: String,
}

#[derive(Args, Debug)]
struct RunWorkflowArgs {
    #[arg(long)]
    id: String,
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    #[arg(long, default_value = "fake")]
    runtime: String,
    #[arg(long, default_value = "")]
    model: String,
}

#[derive(Args, Debug)]
struct AdvanceArgs {
    #[arg(long)]
    id: String,
    #[arg(long, default_value = "system")]
    actor: String,
}

#[derive(Args, Debug)]
struct ResumeArgs {
    #[arg(long)]
    id: String,
}

#[derive(Args, Debug)]
struct ReviewArgs {
    #[arg(long)]
    id: String,
}

#[derive(Args, Debug)]
struct GuidanceArgs {
    #[arg(long)]
    id: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum WorkItemTypeArg {
    Feature,
    Bugfix,
    Review,
    Release,
    Chore,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum PriorityArg {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum WorkStageArg {
    Intake,
    Roundtable,
    Architecture,
    Implement,
    Test,
    Review,
    Pr,
    Release,
}

impl From<WorkItemTypeArg> for WorkItemType {
    fn from(value: WorkItemTypeArg) -> Self {
        match value {
            WorkItemTypeArg::Feature => Self::Feature,
            WorkItemTypeArg::Bugfix => Self::Bugfix,
            WorkItemTypeArg::Review => Self::Review,
            WorkItemTypeArg::Release => Self::Release,
            WorkItemTypeArg::Chore => Self::Chore,
        }
    }
}

impl From<PriorityArg> for Priority {
    fn from(value: PriorityArg) -> Self {
        match value {
            PriorityArg::Low => Self::Low,
            PriorityArg::Medium => Self::Medium,
            PriorityArg::High => Self::High,
            PriorityArg::Critical => Self::Critical,
        }
    }
}

impl From<WorkStageArg> for WorkStage {
    fn from(value: WorkStageArg) -> Self {
        match value {
            WorkStageArg::Intake => Self::Intake,
            WorkStageArg::Roundtable => Self::Roundtable,
            WorkStageArg::Architecture => Self::Architecture,
            WorkStageArg::Implement => Self::Implement,
            WorkStageArg::Test => Self::Test,
            WorkStageArg::Review => Self::Review,
            WorkStageArg::Pr => Self::PR,
            WorkStageArg::Release => Self::Release,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = ForgeFlowPaths::discover(fs::canonicalize(&cli.repo_root).unwrap_or(cli.repo_root));
    let store = WorkItemStore::new(paths.clone());

    match cli.command {
        Command::Init => init_repo(&store, &paths),
        Command::Doctor => doctor(&paths),
        Command::Workitem { command } => match command {
            WorkitemCommand::Create(args) => create_workitem(&store, &paths, args),
            WorkitemCommand::Status(args) => status_workitem(&store, args),
            WorkitemCommand::Advance(args) => advance_workitem(&store, args),
            WorkitemCommand::Resume(args) => resume_workitem(&store, args),
            WorkitemCommand::Review(args) => review_workitem(&store, args),
            WorkitemCommand::Guidance(args) => guidance_history(&store, args),
        },
        Command::Memory { command } => match command {
            MemoryCommand::Checkpoint(args) => write_checkpoint(&store, args),
        },
        Command::Workflow { command } => match command {
            WorkflowCommand::Run(args) => run_workflow(&store, args),
        },
        Command::Replay { id } => replay_workitem(&store, id),
        Command::Metrics { id } => metrics_workitem(&store, id),
        Command::Health { id } => health_workitem(&store, &paths, id),
        Command::Repo { command } => match command {
            RepoCommand::Status => repo_status(&paths),
            RepoCommand::Branch { id } => repo_branch(&paths, &store, id),
            RepoCommand::Pr { id, base, dry_run } => repo_pr(&paths, &store, id, &base, dry_run),
            RepoCommand::Issue { id, dry_run } => repo_issue(&store, id, dry_run),
        },
    }
}

fn init_repo(store: &WorkItemStore, paths: &ForgeFlowPaths) -> Result<()> {
    store.init_layout()?;
    fs::create_dir_all(paths.repo_root.join(".forgeflow"))?;
    println!(
        "initialized ForgeFlow layout at {}",
        paths.repo_root.display()
    );
    Ok(())
}

fn doctor(paths: &ForgeFlowPaths) -> Result<()> {
    let roles = default_roles();
    let checks = [
        ("Cargo.toml", paths.repo_root.join("Cargo.toml").exists()),
        ("CLAUDE.md", paths.repo_root.join("CLAUDE.md").exists()),
        ("schemas", paths.schemas_dir.exists()),
        ("workitems", paths.workitems_dir.exists()),
    ];

    println!("ForgeFlow doctor");
    for (name, ok) in checks {
        println!("- {}: {}", name, if ok { "ok" } else { "missing" });
    }
    println!("- roles: {}", roles.len());
    Ok(())
}

fn create_workitem(
    store: &WorkItemStore,
    paths: &ForgeFlowPaths,
    args: CreateWorkitemArgs,
) -> Result<()> {
    store.init_layout()?;
    if store.workitem_exists(&args.id) {
        anyhow::bail!("workitem {} already exists", args.id);
    }

    let workitem = WorkItem {
        id: args.id,
        title: args.title,
        r#type: args.kind.into(),
        priority: args.priority.into(),
        repo: paths.repo_root.display().to_string(),
        stage: WorkStage::Intake,
        owner: args.owner,
        linked_issue: args.linked_issue,
        linked_branch: args.linked_branch,
        artifacts: vec![],
        checkpoints: vec![],
    };

    let root = store.create_workitem(&workitem)?;
    println!("created workitem at {}", root.display());
    Ok(())
}

fn status_workitem(store: &WorkItemStore, args: StatusArgs) -> Result<()> {
    let workitem = store.load_workitem(&args.id)?;
    println!("id: {}", workitem.id);
    println!("title: {}", workitem.title);
    println!("stage: {}", workitem.stage);
    println!("artifacts: {}", workitem.artifacts.len());
    println!("checkpoints: {}", workitem.checkpoints.len());
    Ok(())
}

fn advance_workitem(store: &WorkItemStore, args: AdvanceArgs) -> Result<()> {
    let mut workitem = store.load_workitem(&args.id)?;
    let events = store.load_events(&args.id)?;
    let completed_stages: HashSet<String> = events
        .iter()
        .filter(|e| matches!(e.status, forgeflow_domain::ExecutionStatus::Completed))
        .map(|e| e.stage.clone())
        .collect();

    let sm = StateMachine::new();
    let result = sm.try_advance(&workitem, &completed_stages, &args.actor);

    match &result {
        TransitionResult::Ok { new_stage, event } => {
            workitem.stage = new_stage.clone();
            store.save_workitem(&workitem)?;
            let file_name = format!("{}-advance.json", now().format("%Y%m%dT%H%M%SZ"));
            let json = serde_json::to_string_pretty(&event)?;
            store.append_event_json(&workitem.id, &file_name, &json)?;
            println!("advanced to stage: {new_stage}");
        }
        TransitionResult::Blocked { evaluation, .. } => {
            println!("transition blocked:");
            for (gate, result) in &evaluation.results {
                match result {
                    GateResult::Failed { reason } => {
                        println!("  FAIL [{}]: {reason}", gate.name);
                    }
                    GateResult::Warning { reason } => {
                        println!("  WARN [{}]: {reason}", gate.name);
                    }
                    GateResult::Passed => {}
                }
            }
        }
        TransitionResult::Failed {
            fallback_stage,
            reason,
            ..
        } => {
            println!("transition failed: {reason}");
            println!("fallback stage: {fallback_stage}");
        }
    }
    Ok(())
}

fn resume_workitem(store: &WorkItemStore, args: ResumeArgs) -> Result<()> {
    let context = orchestrator_resume(store, &args.id)?;
    println!("resumed workitem: {}", context.workitem.id);
    println!("current stage: {}", context.workitem.stage);
    println!("completed stages: {:?}", context.completed_stages);
    println!("next action: {}", context.next_action);
    if let Some(cp) = context.latest_checkpoint {
        println!("last checkpoint stage: {}", cp.stage);
        println!("last checkpoint summary: {}", cp.summary);
        if !cp.blockers.is_empty() {
            println!("blockers: {:?}", cp.blockers);
        }
    }
    Ok(())
}

fn review_workitem(store: &WorkItemStore, args: ReviewArgs) -> Result<()> {
    let workitem = store.load_workitem(&args.id)?;
    let events = store.load_events(&args.id)?;
    let existing_guidances = store.load_guidances(&args.id)?;
    let new_guidances = review_stage_guidance(&workitem, &events, &existing_guidances);

    if new_guidances.is_empty() {
        println!("stage {}: no issues detected", workitem.stage);
        return Ok(());
    }

    for g in &new_guidances {
        let path = store.write_guidance(g)?;
        println!(
            "[{:?}] {:?} - stage {:?}",
            g.severity, g.assessment, g.stage
        );
        for obs in &g.observations {
            println!("  observation: {obs}");
        }
        for sug in &g.suggestions {
            println!("  suggestion: {sug}");
        }
        if g.should_intervene {
            println!("  ** intervention recommended **");
        }
        println!("  persisted: {}", path.display());
    }
    Ok(())
}

fn guidance_history(store: &WorkItemStore, args: GuidanceArgs) -> Result<()> {
    let guidances = store.load_guidances(&args.id)?;
    if guidances.is_empty() {
        println!("no guidance records found for workitem {}", args.id);
        return Ok(());
    }
    println!(
        "guidance history for workitem {} ({} records):",
        args.id,
        guidances.len()
    );
    for g in &guidances {
        println!(
            "- [{:?}] {:?} at stage {} ({})",
            g.severity, g.assessment, g.stage, g.created_at
        );
        for obs in &g.observations {
            println!("  * {obs}");
        }
    }
    Ok(())
}

fn write_checkpoint(store: &WorkItemStore, args: CheckpointArgs) -> Result<()> {
    let mut workitem = store.load_workitem(&args.id)?;
    workitem.stage = args.stage.into();

    let checkpoint = Checkpoint {
        workitem_id: workitem.id.clone(),
        stage: workitem.stage.clone(),
        summary: args.summary.clone(),
        artifacts: workitem.artifacts.clone(),
        blockers: split_csv(&args.blockers),
        next_step: args.next_step.clone(),
        verification: args.verification.clone(),
        created_at: now(),
    };

    let path = store.write_checkpoint(&checkpoint)?;
    workitem.checkpoints.push(path.display().to_string());
    store.save_workitem(&workitem)?;
    store.write_summary(&workitem.id, &render_summary(&workitem, &checkpoint))?;
    println!("checkpoint written: {}", path.display());
    Ok(())
}

fn run_workflow(store: &WorkItemStore, args: RunWorkflowArgs) -> Result<()> {
    let workitem = store.load_workitem(&args.id)?;

    if args.dry_run {
        let runtime = FakeRuntime;
        let events = dry_run_workflow(&runtime, &workitem);
        let file_name = format!("{}-dry-run.json", now().format("%Y%m%dT%H%M%SZ"));
        let json = serde_json::to_string_pretty(&events)?;
        let path = store.append_event_json(&workitem.id, &file_name, &json)?;
        println!("workflow dry-run complete: {}", path.display());
        return Ok(());
    }

    match args.runtime.as_str() {
        "fake" => {
            anyhow::bail!("use --dry-run with fake runtime, or specify --runtime openai");
        }
        "openai" => {
            let base_url =
                std::env::var("FORGEFLOW_OPENAI_BASE_URL").unwrap_or_else(|_| {
                    "http://192.187.98.166:18317".to_string()
                });
            let api_key = std::env::var("FORGEFLOW_OPENAI_API_KEY")
                .unwrap_or_else(|_| "sk-cliproxy-vps-token".to_string());
            let model = if args.model.is_empty() {
                std::env::var("FORGEFLOW_OPENAI_MODEL")
                    .unwrap_or_else(|_| "gpt-5.4".to_string())
            } else {
                args.model
            };

            println!("connecting to {base_url} with model {model}...");
            let runtime = OpenAIRuntime::new(&base_url, &api_key, &model);

            let health = runtime.health_check();
            if !health.ok {
                anyhow::bail!("runtime health check failed: {}", health.detail);
            }
            println!("runtime healthy: {}", health.detail);

            let events = dry_run_workflow(&runtime, &workitem);
            let file_name = format!("{}-workflow.json", now().format("%Y%m%dT%H%M%SZ"));
            let json = serde_json::to_string_pretty(&events)?;
            let path = store.append_event_json(&workitem.id, &file_name, &json)?;

            for event in &events {
                println!(
                    "[{}] {} -> {:.80}...",
                    event.actor,
                    event.stage,
                    event.action.chars().take(80).collect::<String>()
                );
            }
            println!("workflow complete: {}", path.display());
        }
        other => {
            anyhow::bail!("unknown runtime: {other}. Use 'fake' or 'openai'");
        }
    }
    Ok(())
}

fn render_summary(workitem: &WorkItem, checkpoint: &Checkpoint) -> String {
    format!(
        "# {}\n\n- WorkItem ID: {}\n- Current stage: {}\n- Artifacts: {}\n- Blockers: {}\n- Next step: {}\n- Verification: {}\n",
        workitem.title,
        workitem.id,
        checkpoint.stage,
        if checkpoint.artifacts.is_empty() {
            "none".to_string()
        } else {
            checkpoint.artifacts.join(", ")
        },
        if checkpoint.blockers.is_empty() {
            "none".to_string()
        } else {
            checkpoint.blockers.join(", ")
        },
        checkpoint.next_step,
        checkpoint.verification,
    )
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn replay_workitem(store: &WorkItemStore, id: String) -> Result<()> {
    let workitem = store.load_workitem(&id)?;
    let events = store.load_events(&id)?;
    let guidances = store.load_guidances(&id)?;

    let replay = build_replay(&workitem, &events, &guidances);

    println!("=== Replay: {} ===", replay.workitem_title);
    println!("ID: {}", replay.workitem_id);
    println!("Current stage: {}", replay.current_stage);
    println!("Events: {}", replay.event_count);
    println!("Guidances: {}", replay.guidance_count);
    println!("Failed events: {}", replay.failed_event_count);

    if let Some(dur) = replay.total_duration_minutes {
        println!("Total duration: {} min", dur);
    }

    println!("\n--- Stage Timeline ---");
    for seg in &replay.stage_timeline {
        let status_label = match seg.status {
            forgeflow_observability::StageStatus::Active => "active",
            forgeflow_observability::StageStatus::Completed => "done",
            forgeflow_observability::StageStatus::Blocked => "blocked",
            forgeflow_observability::StageStatus::NotStarted => "pending",
        };
        println!(
            "  {} [{}] {} events{}",
            seg.stage,
            status_label,
            seg.event_count,
            seg.duration_minutes
                .map(|d| format!(" ({} min)", d))
                .unwrap_or_default()
        );
    }

    println!("\n--- Actor Summary ---");
    for actor in &replay.actor_summary {
        println!(
            "  {}: {} events ({} ok, {} fail)",
            actor.actor, actor.event_count, actor.completed, actor.failed
        );
    }

    if !events.is_empty() {
        println!("\n--- Event Trail ---");
        println!("{}", format_event_trail(&events));
    }

    if !guidances.is_empty() {
        println!("\n--- Guidance Digest ---");
        println!("{}", format_guidance_digest(&guidances));
    }

    Ok(())
}

fn metrics_workitem(store: &WorkItemStore, id: String) -> Result<()> {
    let events = store.load_events(&id)?;
    let metrics = ExecutionMetrics::from_events(&id, &events);

    println!("=== Metrics: {} ===", metrics.workitem_id);
    println!("Total events: {}", metrics.total_events);
    println!("Completed: {}", metrics.completed_events);
    println!("Failed: {}", metrics.failed_events);
    println!("Skipped: {}", metrics.skipped_events);
    println!("Success rate: {:.1}%", metrics.success_rate * 100.0);
    println!("Stages touched: {}", metrics.stages_touched.join(", "));
    println!("Actor count: {}", metrics.actor_count);
    if let Some(dur) = metrics.estimated_duration_minutes {
        println!("Duration: {} min", dur);
    }

    Ok(())
}

fn health_workitem(store: &WorkItemStore, paths: &ForgeFlowPaths, id: String) -> Result<()> {
    let workitem = store.load_workitem(&id)?;
    let events = store.load_events(&id)?;
    let guidances = store.load_guidances(&id)?;

    let health = assess_health(&workitem, &events, &guidances);

    println!("=== Health: {} ===", health.workitem_id);
    println!("Stage: {}", health.stage);
    println!("Healthy: {}", if health.is_healthy { "yes" } else { "no" });

    if !health.issues.is_empty() {
        println!("Issues:");
        for issue in &health.issues {
            println!("  - {issue}");
        }
    }

    // Also check repo status if possible
    if let Ok(repo) = GitRepo::open(&paths.repo_root) {
        match repo_status_for_workitem(&repo, &workitem) {
            Ok(ws) => {
                println!("\n--- Repo Status ---");
                println!("Branch: {}", ws.repo_status.current_branch);
                println!("Expected branch: {}", ws.expected_branch);
                println!("On expected: {}", if ws.on_expected_branch { "yes" } else { "no" });
                println!("Uncommitted changes: {}", if ws.repo_status.has_uncommitted_changes { "yes" } else { "no" });
            }
            Err(e) => println!("\nRepo status unavailable: {e}"),
        }
    }

    Ok(())
}

fn repo_status(paths: &ForgeFlowPaths) -> Result<()> {
    let repo = GitRepo::open(&paths.repo_root)?;
    let status = repo.status_summary()?;

    println!("=== Repo Status ===");
    println!("Root: {}", status.root.display());
    println!("Current branch: {}", status.current_branch);
    println!("Uncommitted changes: {}", if status.has_uncommitted_changes { "yes" } else { "no" });
    println!("Branches: {}", status.branches.join(", "));

    Ok(())
}

fn repo_branch(paths: &ForgeFlowPaths, store: &WorkItemStore, id: String) -> Result<()> {
    let workitem = store.load_workitem(&id)?;
    let repo = GitRepo::open(&paths.repo_root)?;
    let branch_name = workitem_branch_name(&workitem);

    println!("Creating branch '{}' for workitem {}...", branch_name, workitem.id);
    repo.create_branch(&branch_name)?;
    println!("Created and switched to branch: {branch_name}");

    Ok(())
}

fn repo_pr(paths: &ForgeFlowPaths, store: &WorkItemStore, id: String, base: &str, dry_run: bool) -> Result<()> {
    let workitem = store.load_workitem(&id)?;
    let spec = PullRequestSpec::from_workitem(&workitem, base);

    if dry_run {
        println!("=== PR Spec (dry run) ===");
        println!("Title: {}", spec.title);
        println!("Head: {}", spec.head_branch);
        println!("Base: {}", spec.base_branch);
        println!("Labels: {}", spec.labels.join(", "));
        println!("\nBody:\n{}", spec.body);
        return Ok(());
    }

    // Use gh CLI to create PR
    let output = std::process::Command::new("gh")
        .args([
            "pr", "create",
            "--title", &spec.title,
            "--body", &spec.body,
            "--head", &spec.head_branch,
            "--base", &spec.base_branch,
        ])
        .current_dir(&paths.repo_root)
        .output()?;

    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        println!("Created PR: {url}");
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create PR: {}", stderr.trim());
    }

    Ok(())
}

fn repo_issue(store: &WorkItemStore, id: String, dry_run: bool) -> Result<()> {
    let workitem = store.load_workitem(&id)?;
    let spec = IssueSpec::from_workitem(&workitem);

    if dry_run {
        println!("=== Issue Spec (dry run) ===");
        println!("Title: {}", spec.title);
        println!("Labels: {}", spec.labels.join(", "));
        println!("\nBody:\n{}", spec.body);
        return Ok(());
    }

    println!("Issue creation requires gh CLI integration.");
    println!("Title: {}", spec.title);
    println!("Labels: {}", spec.labels.join(", "));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::split_csv;

    #[test]
    fn split_csv_ignores_empty_items() {
        assert_eq!(split_csv("a, b, ,c"), vec!["a", "b", "c"]);
    }
}
