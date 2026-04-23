use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySummary {
    pub workitem_id: String,
    pub event_count: usize,
}
