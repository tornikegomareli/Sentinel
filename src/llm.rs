mod claude;
mod models;

pub use claude::ClaudeClient;
pub use models::{Message, Role};

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn generate_response(&self, messages: &[Message]) -> Result<String>;
}

pub enum LlmProvider {
    Claude,
    OpenAI,
    Gemini,
}

pub fn create_client(provider: LlmProvider) -> Box<dyn LlmClient> {
    match provider {
        LlmProvider::Claude => Box::new(ClaudeClient::new()),
        LlmProvider::OpenAI => unimplemented!("OpenAI support coming soon"),
        LlmProvider::Gemini => unimplemented!("Gemini support coming soon"),
    }
}