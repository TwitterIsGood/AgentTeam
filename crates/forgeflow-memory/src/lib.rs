use forgeflow_config::ForgeFlowPaths;
use forgeflow_core::Result;
use forgeflow_domain::{
    Checkpoint, ExecutionEvent, Guidance, Lesson, StageDurationStats, ThresholdAdjustment, WorkItem,
};
use std::fs;
use std::path::{Path, PathBuf};

pub struct WorkItemStore {
    paths: ForgeFlowPaths,
}

impl WorkItemStore {
    pub fn new(paths: ForgeFlowPaths) -> Self {
        Self { paths }
    }

    pub fn init_layout(&self) -> Result<()> {
        fs::create_dir_all(&self.paths.workitems_dir)?;
        Ok(())
    }

    pub fn create_workitem(&self, workitem: &WorkItem) -> Result<PathBuf> {
        let root = self.workitem_dir(&workitem.id);
        fs::create_dir_all(root.join("artifacts"))?;
        fs::create_dir_all(root.join("checkpoints"))?;
        fs::create_dir_all(root.join("events"))?;
        fs::write(
            root.join("workitem.json"),
            serde_json::to_vec_pretty(workitem)?,
        )?;
        fs::write(root.join("summary.md"), self.default_summary(workitem))?;
        Ok(root)
    }

    pub fn load_workitem(&self, id: &str) -> Result<WorkItem> {
        let content = fs::read_to_string(self.workitem_dir(id).join("workitem.json"))?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save_workitem(&self, workitem: &WorkItem) -> Result<()> {
        fs::write(
            self.workitem_dir(&workitem.id).join("workitem.json"),
            serde_json::to_vec_pretty(workitem)?,
        )?;
        Ok(())
    }

    pub fn write_checkpoint(&self, checkpoint: &Checkpoint) -> Result<PathBuf> {
        let file_name = format!("{}.json", checkpoint.created_at.format("%Y%m%dT%H%M%SZ"));
        let path = self
            .workitem_dir(&checkpoint.workitem_id)
            .join("checkpoints")
            .join(file_name);
        fs::write(&path, serde_json::to_vec_pretty(checkpoint)?)?;
        Ok(path)
    }

    pub fn write_summary(&self, id: &str, content: &str) -> Result<()> {
        fs::write(self.workitem_dir(id).join("summary.md"), content)?;
        Ok(())
    }

    pub fn append_event_json(
        &self,
        id: &str,
        file_name: &str,
        event_json: &str,
    ) -> Result<PathBuf> {
        let path = self.workitem_dir(id).join("events").join(file_name);
        fs::write(&path, event_json)?;
        Ok(path)
    }

    pub fn workitem_dir(&self, id: &str) -> PathBuf {
        self.paths.workitems_dir.join(id)
    }

    pub fn workitem_exists(&self, id: &str) -> bool {
        Path::new(&self.workitem_dir(id)).exists()
    }

    fn default_summary(&self, workitem: &WorkItem) -> String {
        format!(
            "# {}\n\n- WorkItem ID: {}\n- Stage: {}\n- Artifacts: none\n- Blockers: none\n- Next step: define the next structured artifact\n",
            workitem.title, workitem.id, workitem.stage
        )
    }

    pub fn load_events(&self, id: &str) -> Result<Vec<ExecutionEvent>> {
        let events_dir = self.workitem_dir(id).join("events");
        let mut events = Vec::new();
        if events_dir.exists() {
            for entry in fs::read_dir(&events_dir)? {
                let entry = entry?;
                if entry.path().extension().is_some_and(|e| e == "json") {
                    let content = fs::read_to_string(entry.path())?;
                    if let Ok(evt) = serde_json::from_str::<ExecutionEvent>(&content) {
                        events.push(evt);
                    } else if let Ok(evts) = serde_json::from_str::<Vec<ExecutionEvent>>(&content) {
                        events.extend(evts);
                    }
                }
            }
        }
        Ok(events)
    }

    pub fn load_latest_checkpoint(&self, id: &str) -> Result<Option<Checkpoint>> {
        let checkpoints_dir = self.workitem_dir(id).join("checkpoints");
        if !checkpoints_dir.exists() {
            return Ok(None);
        }
        let mut entries: Vec<_> = fs::read_dir(&checkpoints_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
            .collect();
        entries.sort_by_key(|e| e.file_name());
        match entries.last() {
            Some(entry) => {
                let content = fs::read_to_string(entry.path())?;
                Ok(Some(serde_json::from_str(&content)?))
            }
            None => Ok(None),
        }
    }

    pub fn write_guidance(&self, guidance: &Guidance) -> Result<PathBuf> {
        let guidances_dir = self.workitem_dir(&guidance.workitem_id).join("guidances");
        fs::create_dir_all(&guidances_dir)?;
        let file_name = format!("{}.json", guidance.created_at.format("%Y%m%dT%H%M%SZ"));
        let path = guidances_dir.join(file_name);
        fs::write(&path, serde_json::to_vec_pretty(guidance)?)?;
        Ok(path)
    }

    pub fn load_guidances(&self, id: &str) -> Result<Vec<Guidance>> {
        let guidances_dir = self.workitem_dir(id).join("guidances");
        let mut guidances = Vec::new();
        if guidances_dir.exists() {
            for entry in fs::read_dir(&guidances_dir)? {
                let entry = entry?;
                if entry.path().extension().is_some_and(|e| e == "json") {
                    let content = fs::read_to_string(entry.path())?;
                    let g: Guidance = serde_json::from_str(&content)?;
                    guidances.push(g);
                }
            }
        }
        guidances.sort_by_key(|g| g.created_at);
        Ok(guidances)
    }

    pub fn write_artifact_file(&self, workitem_id: &str, file_name: &str, content: &str) -> Result<PathBuf> {
        let dir = self.workitem_dir(workitem_id).join("artifacts");
        fs::create_dir_all(&dir)?;
        let path = dir.join(file_name);
        fs::write(&path, content)?;
        Ok(path)
    }

    pub fn load_artifact_content(&self, workitem_id: &str, artifact_name: &str) -> Result<Option<String>> {
        let dir = self.workitem_dir(workitem_id).join("artifacts");
        if !dir.exists() {
            return Ok(None);
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains(artifact_name) {
                let content = fs::read_to_string(entry.path())?;
                return Ok(Some(content));
            }
        }
        Ok(None)
    }
}

// --- Learning Store ---

pub struct LearningStore {
    learning_dir: PathBuf,
}

impl LearningStore {
    pub fn new(paths: &ForgeFlowPaths) -> Self {
        Self {
            learning_dir: paths.repo_root.join(".forgeflow").join("learning"),
        }
    }

    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.learning_dir)?;
        Ok(())
    }

    pub fn save_lesson(&self, lesson: &Lesson) -> Result<PathBuf> {
        self.init()?;
        let path = self.learning_dir.join("lessons.json");
        let mut lessons = self.load_lessons().unwrap_or_default();
        lessons.push(lesson.clone());
        let json = serde_json::to_string_pretty(&lessons)?;
        fs::write(&path, json)?;
        Ok(path)
    }

    pub fn load_lessons(&self) -> Result<Vec<Lesson>> {
        let path = self.learning_dir.join("lessons.json");
        if !path.exists() {
            return Ok(vec![]);
        }
        let content = fs::read_to_string(&path)?;
        let lessons: Vec<Lesson> = serde_json::from_str(&content)?;
        Ok(lessons)
    }

    pub fn save_threshold_adjustment(&self, adj: &ThresholdAdjustment) -> Result<PathBuf> {
        self.init()?;
        let path = self.learning_dir.join("adjustments.json");
        let mut adjustments = self.load_adjustments().unwrap_or_default();
        adjustments.push(adj.clone());
        let json = serde_json::to_string_pretty(&adjustments)?;
        fs::write(&path, json)?;
        Ok(path)
    }

    pub fn load_adjustments(&self) -> Result<Vec<ThresholdAdjustment>> {
        let path = self.learning_dir.join("adjustments.json");
        if !path.exists() {
            return Ok(vec![]);
        }
        let content = fs::read_to_string(&path)?;
        let adjustments: Vec<ThresholdAdjustment> = serde_json::from_str(&content)?;
        Ok(adjustments)
    }

    pub fn save_stage_stats(&self, stats: &StageDurationStats) -> Result<PathBuf> {
        self.init()?;
        let path = self.learning_dir.join("stage-stats.json");
        let mut all_stats = self.load_stage_stats().unwrap_or_default();

        if let Some(existing) = all_stats.iter_mut().find(|s| s.stage == stats.stage) {
            *existing = stats.clone();
        } else {
            all_stats.push(stats.clone());
        }

        let json = serde_json::to_string_pretty(&all_stats)?;
        fs::write(&path, json)?;
        Ok(path)
    }

    pub fn load_stage_stats(&self) -> Result<Vec<StageDurationStats>> {
        let path = self.learning_dir.join("stage-stats.json");
        if !path.exists() {
            return Ok(vec![]);
        }
        let content = fs::read_to_string(&path)?;
        let stats: Vec<StageDurationStats> = serde_json::from_str(&content)?;
        Ok(stats)
    }
}
