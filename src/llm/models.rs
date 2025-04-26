use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

// Claude specific models
#[derive(Debug, Serialize, Deserialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeResponse {
    pub content: Vec<ClaudeContent>,
}

#[derive(Debug, Deserialize)]
pub struct ClaudeContent {
    pub text: String,
    pub r#type: String,
}