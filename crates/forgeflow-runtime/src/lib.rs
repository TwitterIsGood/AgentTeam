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

// --- OpenAI-compatible Runtime ---

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatUsage {
    #[allow(dead_code)]
    prompt_tokens: u32,
    completion_tokens: u32,
    #[allow(dead_code)]
    total_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

pub struct OpenAIRuntime {
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAIRuntime {
    pub fn new(base_url: &str, api_key: &str, model: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }
}

impl Runtime for OpenAIRuntime {
    fn execute(&self, request: ExecutionRequest) -> ExecutionResponse {
        let start = std::time::Instant::now();

        let system_prompt = format!(
            "You are {}, an AI agent in a software delivery team. {}",
            request.actor, request.instruction
        );

        let chat_req = ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: request.instruction.clone(),
                },
            ],
            max_tokens: 2048,
        };

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        let url = format!("{}/v1/chat/completions", self.base_url);

        // Retry up to 3 times with increasing backoff
        let mut last_error = String::new();
        for attempt in 0..3 {
            let result = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&chat_req)
                .send();

            match result {
                Ok(resp) => {
                    let latency_ms = start.elapsed().as_millis() as u64;

                    match resp.json::<ChatResponse>() {
                        Ok(chat_resp) => {
                            let output = chat_resp
                                .choices
                                .first()
                                .map(|c| c.message.content.clone())
                                .unwrap_or_default();

                            let tokens = chat_resp
                                .usage
                                .map(|u| u.completion_tokens)
                                .unwrap_or(0);

                            return ExecutionResponse {
                                actor: request.actor,
                                output,
                                tokens,
                                latency_ms,
                                estimated_cost_usd: 0.0,
                            };
                        }
                        Err(e) => {
                            return ExecutionResponse {
                                actor: request.actor,
                                output: format!("error parsing response: {e}"),
                                tokens: 0,
                                latency_ms,
                                estimated_cost_usd: 0.0,
                            };
                        }
                    }
                }
                Err(e) => {
                    last_error = format!("{e}");
                    if attempt < 2 {
                        let wait = std::time::Duration::from_secs(2u64.pow(attempt as u32 + 1));
                        std::thread::sleep(wait);
                    }
                }
            }
        }

        ExecutionResponse {
            actor: request.actor,
            output: format!("error calling LLM after 3 retries: {last_error}"),
            tokens: 0,
            latency_ms: start.elapsed().as_millis() as u64,
            estimated_cost_usd: 0.0,
        }
    }

    fn stream_execute(&self, request: ExecutionRequest) -> Vec<String> {
        let response = self.execute(request);
        vec![response.output]
    }

    fn health_check(&self) -> HealthStatus {
        let client = reqwest::blocking::Client::new();
        let url = format!("{}/v1/models", self.base_url);

        let result = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send();

        match result {
            Ok(resp) if resp.status().is_success() => HealthStatus {
                name: format!("openai-runtime ({})", self.model),
                ok: true,
                detail: "connected".to_string(),
            },
            Ok(resp) => HealthStatus {
                name: format!("openai-runtime ({})", self.model),
                ok: false,
                detail: format!("HTTP {}", resp.status()),
            },
            Err(e) => HealthStatus {
                name: format!("openai-runtime ({})", self.model),
                ok: false,
                detail: format!("connection error: {e}"),
            },
        }
    }

    fn capabilities(&self) -> RuntimeCapabilities {
        RuntimeCapabilities {
            streaming: false,
            structured_output: true,
            cost_estimation: false,
        }
    }

    fn estimate_cost(&self, _request: &ExecutionRequest) -> CostEstimate {
        CostEstimate {
            estimated_cost_usd: 0.0,
        }
    }
}
