use forgeflow_config::ForgeFlowPaths;
use forgeflow_core::Result;
use forgeflow_domain::{Checkpoint, ExecutionEvent, Guidance, WorkItem};
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
}
