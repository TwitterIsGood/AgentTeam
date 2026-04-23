use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use forgeflow_agents::default_roles;
use forgeflow_config::ForgeFlowPaths;
use forgeflow_core::now;
use forgeflow_domain::{Checkpoint, Priority, WorkItem, WorkItemType, WorkStage};
use forgeflow_memory::WorkItemStore;
use forgeflow_orchestrator::{StateMachine, TransitionResult, resume as orchestrator_resume};
use forgeflow_policy::GateResult;
use forgeflow_runtime::FakeRuntime;
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
    if !args.dry_run {
        anyhow::bail!("only --dry-run is supported in V0");
    }

    let workitem = store.load_workitem(&args.id)?;
    let runtime = FakeRuntime;
    let events = dry_run_workflow(&runtime, &workitem);
    let file_name = format!("{}-dry-run.json", now().format("%Y%m%dT%H%M%SZ"));
    let json = serde_json::to_string_pretty(&events)?;
    let path = store.append_event_json(&workitem.id, &file_name, &json)?;
    println!("workflow dry-run complete: {}", path.display());
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

#[cfg(test)]
mod tests {
    use super::split_csv;

    #[test]
    fn split_csv_ignores_empty_items() {
        assert_eq!(split_csv("a, b, ,c"), vec!["a", "b", "c"]);
    }
}
