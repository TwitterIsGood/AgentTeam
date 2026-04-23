use forgeflow_core::HealthStatus;
use serde::{Deserialize, Serialize};

pub trait Runtime {
    fn execute(&self, request: ExecutionRequest) -> ExecutionResponse;
    fn stream_execute(&self, request: ExecutionRequest) -> Vec<String>;
    fn health_check(&self) -> HealthStatus;
    fn capabilities(&self) -> RuntimeCapabilities;
    fn estimate_cost(&self, request: &ExecutionRequest) -> CostEstimate;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub actor: String,
    pub instruction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResponse {
    pub actor: String,
    pub output: String,
    pub tokens: u32,
    pub latency_ms: u64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCapabilities {
    pub streaming: bool,
    pub structured_output: bool,
    pub cost_estimation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Default)]
pub struct FakeRuntime;

impl Runtime for FakeRuntime {
    fn execute(&self, request: ExecutionRequest) -> ExecutionResponse {
        ExecutionResponse {
            actor: request.actor,
            output: format!("dry-run: {}", request.instruction),
            tokens: 42,
            latency_ms: 5,
            estimated_cost_usd: 0.0,
        }
    }

    fn stream_execute(&self, request: ExecutionRequest) -> Vec<String> {
        vec![format!("dry-run chunk: {}", request.instruction)]
    }

    fn health_check(&self) -> HealthStatus {
        HealthStatus {
            name: "fake-runtime".to_string(),
            ok: true,
            detail: "available".to_string(),
        }
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            streaming: true,
            structured_output: true,
            cost_estimation: true,
        }
    }

    fn estimate_cost(&self, _request: &ExecutionRequest) -> CostEstimate {
        CostEstimate {
            estimated_cost_usd: 0.0,
        }
    }
}
