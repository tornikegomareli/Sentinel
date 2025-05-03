use serde::{Deserialize, Serialize};

/// Represents the role of a message sender
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
}

// Implementation for converting from main::Role to tui::MessageRole
impl From<crate::Role> for MessageRole {
    fn from(role: crate::Role) -> Self {
        match role {
            crate::Role::User => MessageRole::User,
            crate::Role::Assistant => MessageRole::Assistant,
            crate::Role::System => MessageRole::System,
        }
    }
}

/// Represents a message in the chat conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(skip, default)]
    pub input_tokens: usize,
    #[serde(skip, default)]
    pub output_tokens: usize,
    #[serde(skip, default)]
    pub used_tools: Vec<String>,
}

impl UiMessage {
    /// Create a new message with the given role and content
    pub fn new(role: MessageRole, content: String) -> Self {
        Self {
            role,
            content,
            input_tokens: 0,
            output_tokens: 0,
            used_tools: Vec::new(),
        }
    }

    /// Create a new user message
    pub fn user(content: String) -> Self {
        Self::new(MessageRole::User, content)
    }

    /// Create a new assistant message with token counts
    pub fn assistant(content: String, input_tokens: usize, output_tokens: usize) -> Self {
        let mut msg = Self::new(MessageRole::Assistant, content);
        msg.input_tokens = input_tokens;
        msg.output_tokens = output_tokens;
        msg
    }

    /// Create a new assistant message with token counts and used tools
    pub fn assistant_with_tools(
        content: String,
        input_tokens: usize,
        output_tokens: usize,
        used_tools: Vec<String>,
    ) -> Self {
        let mut msg = Self::assistant(content, input_tokens, output_tokens);
        msg.used_tools = used_tools;
        msg
    }

    /// Create a new system message
    pub fn system(content: String) -> Self {
        Self::new(MessageRole::System, content)
    }
}

// Implementation for converting from main::Message to tui::UiMessage
impl From<crate::Message> for UiMessage {
    fn from(message: crate::Message) -> Self {
        UiMessage {
            role: MessageRole::from(message.role),
            content: message.content,
            input_tokens: message.input_tokens,
            output_tokens: message.output_tokens,
            used_tools: message.used_tools,
        }
    }
}