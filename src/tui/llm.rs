use anyhow::Result;
use ollama_rs::{
    coordinator::Coordinator,
    generation::chat::ChatMessage,
    generation::tools::implementations::{Calculator, DDGSearcher, Scraper, StockScraper},
    models::ModelOptions,
    Ollama,
};
use std::{
    env,
    sync::{Arc, Mutex},
};

use crate::tui::message::{MessageRole, UiMessage};

/// Weather function for Ollama
///
/// Gets weather information for a given city
#[ollama_rs::function]
pub async fn get_weather(city: String) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    Ok(reqwest::get(format!("https://wttr.in/{city}?format=%C+%t"))
        .await?
        .text()
        .await?)
}

/// Available tools for the LLM
#[derive(Debug, Clone, PartialEq)]
pub enum ToolType {
    Weather,
    Calculator,
    Search,
    Scraper,
    Finance,
}

impl ToolType {
    /// Get the name of the tool as used by Ollama
    pub fn name(&self) -> &'static str {
        match self {
            ToolType::Weather => "get_weather",
            ToolType::Calculator => "Calculator",
            ToolType::Search => "DDGSearcher",
            ToolType::Scraper => "Scraper",
            ToolType::Finance => "StockScraper",
        }
    }
}

/// Handle LLM interactions
pub struct LlmHandler {
    host: String,
    port: u16,
    model: String,
    enabled_tools: Vec<ToolType>,
    last_used_tools: Arc<Mutex<Vec<String>>>,
}

impl LlmHandler {
    /// Create a new LLM handler
    pub fn new() -> Self {
        // Get host and port from environment or use defaults
        let host = env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost".to_string());
        let port = env::var("OLLAMA_PORT")
            .unwrap_or_else(|_| "11434".to_string())
            .parse::<u16>()
            .unwrap_or(11434);

        // Default model
        let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:latest".to_string());

        // All available tools
        let enabled_tools = vec![
            ToolType::Weather,
            ToolType::Calculator,
            ToolType::Search,
            ToolType::Scraper,
            ToolType::Finance,
        ];

        Self {
            host,
            port,
            model,
            enabled_tools,
            last_used_tools: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.model
    }

    /// Get currently used tools
    pub fn get_current_tools(&self) -> Vec<String> {
        let tools = self.last_used_tools.lock().unwrap();
        tools.clone()
    }

    /// Get a new Ollama client
    fn create_client(&self) -> Ollama {
        Ollama::new(self.host.clone(), self.port)
    }

    /// Process a message with Ollama and update the conversation
    pub async fn process_message(
        &self,
        history: &[UiMessage],
        user_message: &UiMessage,
    ) -> Result<UiMessage> {
        // Convert UiMessages to ollama ChatMessages for history
        let chat_history: Vec<ChatMessage> = history
            .iter()
            .map(|msg| match msg.role {
                MessageRole::User => ChatMessage::user(msg.content.clone()),
                MessageRole::Assistant => ChatMessage::assistant(msg.content.clone()),
                MessageRole::System => ChatMessage::system(msg.content.clone()),
            })
            .collect();

        // Create user message for the request
        let chat_message = ChatMessage::user(user_message.content.clone());

        // Clear the tracked tools list before this new response
        {
            let mut tools = self.last_used_tools.lock().unwrap();
            tools.clear();
        }

        // Create a coordinator with enabled tools
        let ollama_client = self.create_client();
        let mut coordinator = Coordinator::new(ollama_client, self.model.clone(), chat_history)
            .options(ModelOptions::default().num_ctx(16384));

        // Add tools based on enabled settings
        for tool in &self.enabled_tools {
            coordinator = match tool {
                ToolType::Weather => coordinator.add_tool(get_weather),
                ToolType::Calculator => coordinator.add_tool(Calculator {}),
                ToolType::Search => coordinator.add_tool(DDGSearcher::new()),
                ToolType::Scraper => coordinator.add_tool(Scraper {}),
                ToolType::Finance => coordinator.add_tool(StockScraper::default()),
            };
        }

        // Process with the coordinator
        match coordinator.chat(vec![chat_message]).await {
            Ok(response) => {
                // Estimate token usage based on text length
                let input_tokens = (user_message.content.len() as f32 / 4.0).ceil() as usize;
                let output_tokens = (response.message.content.len() as f32 / 4.0).ceil() as usize;

                // Identify which tools were used
                let mut used_tools = Vec::new();

                // Check for tool calls in the response
                if !response.message.tool_calls.is_empty() {
                    for tool_call in &response.message.tool_calls {
                        let tool_name = tool_call.function.name.clone();
                        if !used_tools.contains(&tool_name) {
                            used_tools.push(tool_name);
                        }
                    }
                }

                // Detect tools from content if no explicit calls
                if used_tools.is_empty() {
                    self.detect_tools_from_content(&response.message.content, &mut used_tools);
                }

                // Update the last used tools
                {
                    let mut tools = self.last_used_tools.lock().unwrap();
                    *tools = used_tools.clone();
                }

                // Create the assistant message
                Ok(UiMessage::assistant_with_tools(
                    response.message.content,
                    input_tokens,
                    output_tokens,
                    used_tools,
                ))
            }
            Err(err) => {
                // Return an error message
                Ok(UiMessage::assistant(
                    format!("Error generating response with tools: {}", err),
                    0,
                    0,
                ))
            }
        }
    }

    /// Detect which tools might have been used based on response content
    fn detect_tools_from_content(&self, content: &str, used_tools: &mut Vec<String>) {
        let content = content.to_lowercase();

        // Check for each enabled tool if it was potentially used
        for tool in &self.enabled_tools {
            match tool {
                ToolType::Weather => {
                    if content.contains("weather")
                        || content.contains("temperature")
                        || content.contains("forecast")
                        || content.contains("climate")
                    {
                        used_tools.push(tool.name().to_string());
                    }
                }
                ToolType::Calculator => {
                    if content.contains("calculated")
                        || content.contains("result is")
                        || content.contains("math")
                        || content.contains("computation")
                        || content.contains("equals")
                        || content.contains("calculate")
                    {
                        used_tools.push(tool.name().to_string());
                    }
                }
                ToolType::Search => {
                    if content.contains("search")
                        || content.contains("found information")
                        || content.contains("according to")
                        || content.contains("search results")
                        || content.contains("online")
                        || content.contains("internet")
                    {
                        used_tools.push(tool.name().to_string());
                    }
                }
                ToolType::Scraper => {
                    if content.contains("webpage")
                        || content.contains("website")
                        || content.contains("web page")
                        || content.contains("url")
                        || content.contains("content from")
                        || content.contains("page shows")
                    {
                        used_tools.push(tool.name().to_string());
                    }
                }
                ToolType::Finance => {
                    if content.contains("stock")
                        || content.contains("price")
                        || content.contains("market")
                        || content.contains("financial")
                        || content.contains("shares")
                        || content.contains("ticker")
                    {
                        used_tools.push(tool.name().to_string());
                    }
                }
            }
        }
    }
}
