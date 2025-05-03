use super::models::{Message, Role};
use super::LlmClient;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ollama_rs::Ollama;
use ollama_rs::generation::chat::{ChatMessage, request::ChatMessageRequest};
use ollama_rs::generation::completion::request::GenerationRequest;
use std::env;

pub struct OllamaClient {
    client: Ollama,
    model: String,
}

impl OllamaClient {
    #[allow(dead_code)]
    pub fn new() -> Self {
        // Default to localhost:11434 if not specified
        let host = env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost".to_string());
        let port = env::var("OLLAMA_PORT")
            .unwrap_or_else(|_| "11434".to_string())
            .parse::<u16>()
            .unwrap_or(11434);
        
        // Default model (use llama3.2 which is available)
        let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:latest".to_string());
        
        Self {
            client: Ollama::new(host, port),
            model,
        }
    }

    #[allow(dead_code)]
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }
    
    fn convert_message_to_chat_message(message: &Message) -> ChatMessage {
        match message.role {
            Role::User => ChatMessage::user(message.content.clone()),
            Role::Assistant => ChatMessage::assistant(message.content.clone()),
            Role::System => ChatMessage::system(message.content.clone()),
        }
    }
}

#[async_trait]
impl LlmClient for OllamaClient {
    async fn generate_response(&self, messages: &[Message]) -> Result<String> {
        if messages.is_empty() {
            return Err(anyhow::anyhow!("Empty messages"));
        }

        // For a simple completion with just the last message
        if messages.len() == 1 {
            let prompt = messages[0].content.clone();
            let request = GenerationRequest::new(self.model.clone(), prompt);
            
            let response = self.client.generate(request)
                .await
                .context("Failed to generate completion from Ollama")?;
            
            return Ok(response.response);
        }
        
        // Convert internal Message format to Ollama ChatMessage format
        let chat_messages: Vec<ChatMessage> = messages.iter()
            .map(Self::convert_message_to_chat_message)
            .collect();
        
        // Using the chat interface for multiple messages
        let request = ChatMessageRequest::new(self.model.clone(), chat_messages);
        
        let response = self.client.send_chat_messages(request)
            .await
            .context("Failed to generate chat response from Ollama")?;
        
        Ok(response.message.content)
    }

    async fn generate_response_with_tools(
        &self,
        messages: &[Message],
        _tools: &[super::Tool],
    ) -> Result<String> {
        // We'll keep it simple for now - just use regular generation
        // Tools support can be added in a future update using the Coordinator pattern
        self.generate_response(messages).await
    }
}