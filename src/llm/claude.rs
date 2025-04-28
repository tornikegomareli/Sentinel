use super::models::{ClaudeRequest, ClaudeResponse, Message, Tool};
use super::LlmClient;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::env;

pub struct ClaudeClient {
    api_key: String,
    model: String,
}

impl ClaudeClient {
    #[allow(dead_code)]
    pub fn new() -> Self {
        let api_key = env::var("CLAUDE_API_KEY").expect("CLAUDE_API_KEY must be set");

        Self {
            api_key,
            model: "claude-3-opus-20240229".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    async fn send_request(&self, request: &ClaudeRequest) -> Result<ClaudeResponse> {
        let client = reqwest::Client::new();

        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(request)
            .send()
            .await
            .context("Failed to send request to Claude API")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await?;
            anyhow::bail!("Claude API error: {}: {}", status, text);
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .context("Failed to parse Claude API response")?;

        if claude_response.content.is_empty() {
            anyhow::bail!("Empty response from Claude API");
        }

        Ok(claude_response)
    }
}

#[async_trait]
impl LlmClient for ClaudeClient {
    async fn generate_response(&self, messages: &[Message]) -> Result<String> {
        let request = ClaudeRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            max_tokens: 4096,
            temperature: Some(0.7),
            system: None,
            tools: vec![], // No tools
        };

        let claude_response = self.send_request(&request).await?;
        Ok(claude_response.content[0].text.clone())
    }

    async fn generate_response_with_tools(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<String> {
        let request = ClaudeRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            max_tokens: 4096,
            temperature: Some(0.7),
            system: None,
            tools: tools.to_vec(),
        };

        let claude_response = self.send_request(&request).await?;

        // If there's a tool use, we'd handle that here
        if let Some(tool_use) = &claude_response.tool_use {
            // log that we got a tool use request
            eprintln!("Tool use request: {} - {:?}", tool_use.name, tool_use.input);
        }

        Ok(claude_response.content[0].text.clone())
    }
}
