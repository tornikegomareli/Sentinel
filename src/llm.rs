mod claude;
mod models;
mod ollama;

pub use claude::ClaudeClient;
pub use models::{Message, Role, Tool};
pub use ollama::OllamaClient;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn generate_response(&self, messages: &[Message]) -> Result<String>;
    async fn generate_response_with_tools(&self, messages: &[Message], tools: &[Tool]) -> Result<String>;
}

pub enum LlmProvider {
    Claude,
    OpenAI,
    Gemini,
    Ollama,
}

pub fn create_client(provider: LlmProvider) -> Box<dyn LlmClient> {
    match provider {
        LlmProvider::Claude => Box::new(ClaudeClient::new()),
        LlmProvider::OpenAI => unimplemented!("OpenAI support coming soon"),
        LlmProvider::Gemini => unimplemented!("Gemini support coming soon"),
        LlmProvider::Ollama => Box::new(OllamaClient::new()),
    }
}

// Helper function to create a tool from JSON schema
pub fn create_tool(name: &str, description: &str, schema_json: &str) -> Result<Tool> {
    let schema: Value = serde_json::from_str(schema_json)?;
    Ok(Tool {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: schema,
    })
}