use crate::Message;
use crate::Role;
use anyhow::{Context, Result};
use async_trait::async_trait;
use ollama_rs::generation::chat::{request::ChatMessageRequest, ChatMessage};
use ollama_rs::generation::completion::request::GenerationRequest;
use ollama_rs::generation::tools::implementations::{Calculator, DDGSearcher, Scraper};
use ollama_rs::models::ModelOptions;
use ollama_rs::Ollama;
use std::env;
use std::sync::{Arc, Mutex};

// Import our custom Bash tool
use crate::tools::bash::Bash;

pub struct OllamaClient {
    client: Ollama,
    model: String,
    host: String,
    port: u16,
    last_used_tools: Arc<Mutex<Vec<String>>>,
}

/// Get the weather for a given city.
///
/// * city - City to get the weather for.
#[ollama_rs::function]
async fn get_weather(city: String) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    Ok(reqwest::get(format!("https://wttr.in/{city}?format=%C+%t"))
        .await?
        .text()
        .await?)
}

impl OllamaClient {
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
            client: Ollama::new(host.clone(), port),
            model,
            host,
            port,
            last_used_tools: Arc::new(Mutex::new(Vec::new())),
        }
    }

    // Get currently tracked tools
    pub fn get_last_used_tools(&self) -> Vec<String> {
        let tools = self.last_used_tools.lock().unwrap();
        tools.clone()
    }

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

    // Helper function to estimate token count from text length
    // This is a very rough approximation - tokens are typically ~4 chars each
    fn estimate_token_count(text: &str) -> usize {
        (text.len() as f32 / 4.0).ceil() as usize
    }
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
    async fn generate_response(&self, messages: &[Message]) -> Result<(String, usize, usize)>;
    async fn generate_response_with_tools(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(String, usize, usize, Vec<String>)>;
}

// Tool definition
#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[async_trait]
impl LlmClient for OllamaClient {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    async fn generate_response(&self, messages: &[Message]) -> Result<(String, usize, usize)> {
        if messages.is_empty() {
            return Err(anyhow::anyhow!("Empty messages"));
        }

        // For a simple completion with just the last message
        if messages.len() == 1 {
            let prompt = messages[0].content.clone();
            let request = GenerationRequest::new(self.model.clone(), prompt);

            let response = self
                .client
                .generate(request)
                .await
                .context("Failed to generate completion from Ollama")?;

            // For single message completion, we don't get token counts, so estimate
            let input_tokens = Self::estimate_token_count(&messages[0].content);
            let output_tokens = Self::estimate_token_count(&response.response);

            return Ok((response.response, input_tokens, output_tokens));
        }

        // Convert internal Message format to Ollama ChatMessage format
        let chat_messages: Vec<ChatMessage> = messages
            .iter()
            .map(Self::convert_message_to_chat_message)
            .collect();

        // Using the chat interface for multiple messages
        let request = ChatMessageRequest::new(self.model.clone(), chat_messages);

        let response = self
            .client
            .send_chat_messages(request)
            .await
            .context("Failed to generate chat response from Ollama")?;

        // For chat messages, we get an eval count which somewhat correlates to token count
        // This is a rough estimate - done is a boolean in recent ollama-rs versions,
        // so we need to just estimate tokens
        let input_tokens = Self::estimate_token_count(
            &messages
                .iter()
                .fold(String::new(), |acc, m| acc + &m.content + "\n"),
        );
        let output_tokens = Self::estimate_token_count(&response.message.content);

        Ok((response.message.content, input_tokens, output_tokens))
    }

    async fn generate_response_with_tools(
        &self,
        messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(String, usize, usize, Vec<String>)> {
        if messages.is_empty() {
            return Err(anyhow::anyhow!("Empty messages"));
        }

        let last_message = messages
            .last()
            .ok_or_else(|| anyhow::anyhow!("No messages found"))?;

        if last_message.role != Role::User {
            return Err(anyhow::anyhow!("Last message must be from user"));
        }

        // Create a copy of the Ollama client
        let ollama_client = Ollama::new(self.host.clone(), self.port);

        // Convert messages to ChatMessage format for history
        let chat_history: Vec<ChatMessage> = messages
            .iter()
            .take(messages.len() - 1) // All except the last message
            .map(Self::convert_message_to_chat_message)
            .collect();

        // Clear the tracked tools list before this new response
        {
            let mut tools = self.last_used_tools.lock().unwrap();
            tools.clear();
        }

        // Create a coordinator with tools
        let mut coordinator = ollama_rs::coordinator::Coordinator::new(
            ollama_client,
            self.model.clone(),
            chat_history,
        )
        .options(ModelOptions::default().num_ctx(16384))
        .add_tool(get_weather)
        .add_tool(Calculator {})
        .add_tool(DDGSearcher::new())
        .add_tool(Scraper {})
        .add_tool(Bash::new());

        // Send the last user message to the coordinator
        let user_message = ChatMessage::user(last_message.content.clone());

        let response = coordinator
            .chat(vec![user_message])
            .await
            .context("Failed to generate response with tools")?;

        // Track which tools were actually used in this response
        // by examining the tool_calls in the final response message
        {
            let mut tools = self.last_used_tools.lock().unwrap();

            // Check if there are any tool calls in the response message
            if !response.message.tool_calls.is_empty() {
                for tool_call in &response.message.tool_calls {
                    // Add each unique tool name to our tracking list
                    let tool_name = tool_call.function.name.clone();
                    if !tools.contains(&tool_name) {
                        tools.push(tool_name);
                    }
                }
            }

            // If we still don't have any tools recorded, this means the coordinator has already
            // processed all tool calls internally and they're not in the final message
            // In this case, we need to check which tools were registered with the coordinator
            // and check if they were used via specific patterns in the response content
            if tools.is_empty() {
                let content = response.message.content.to_lowercase();

                // Check for patterns indicating tool usage in the response text
                if content.contains("weather")
                    || content.contains("temperature")
                    || content.contains("forecast")
                {
                    tools.push("weather".to_string());
                }

                if content.contains("calculated")
                    || content.contains("result is")
                    || content.contains("math")
                    || content.contains("computation")
                {
                    tools.push("Calculator".to_string());
                }

                if content.contains("search")
                    || content.contains("found information")
                    || content.contains("according to")
                    || content.contains("search results")
                {
                    tools.push("DDGSearcher".to_string());
                }

                if content.contains("webpage")
                    || content.contains("website")
                    || content.contains("web page")
                    || content.contains("url")
                {
                    tools.push("Scraper".to_string());
                }

                // Check for Bash tool usage
                if content.contains("command")
                    || content.contains("executed")
                    || content.contains("terminal")
                    || content.contains("shell")
                    || content.contains("bash")
                    || content.contains("output shows")
                    || content.contains("running")
                {
                    tools.push("bash".to_string());
                }
            }
        }

        // Get tools from our tracked list
        let used_tools = self.get_last_used_tools();

        // Estimate token usage
        let input_tokens = Self::estimate_token_count(&last_message.content);
        let output_tokens = Self::estimate_token_count(&response.message.content);

        Ok((
            response.message.content,
            input_tokens,
            output_tokens,
            used_tools,
        ))
    }
}
