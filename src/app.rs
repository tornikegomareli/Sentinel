use crate::llm::{LlmClient, Message, Role, Tool};
use anyhow::Result;
use std::collections::VecDeque;

pub struct App {
    pub input: String,
    pub messages: Vec<Message>,
    pub current_response: String,
    pub is_loading: bool,
    pub history_index: usize,
    pub input_history: VecDeque<String>,
    pub llm_client: Box<dyn LlmClient>,
    pub tools: Vec<Tool>,
    pub use_tools: bool,
}

impl App {
    pub fn new(llm_client: Box<dyn LlmClient>) -> Self {
        Self {
            input: String::new(),
            messages: Vec::new(),
            current_response: String::new(),
            is_loading: false,
            history_index: 0,
            input_history: VecDeque::with_capacity(50),
            llm_client,
            tools: Vec::new(),
            use_tools: false,
        }
    }
    
    pub fn add_tool(&mut self, tool: Tool) {
        self.tools.push(tool);
    }
    
    pub fn toggle_tools(&mut self) {
        self.use_tools = !self.use_tools;
    }
    
    pub fn handle_input(&mut self, character: char) {
        self.input.push(character);
    }
    
    pub fn backspace(&mut self) {
        self.input.pop();
    }
    
    pub fn clear_input(&mut self) {
        self.input.clear();
    }
    
    pub fn submit_message(&mut self) -> Result<()> {
        if self.input.trim().is_empty() {
            return Ok(());
        }
        
        let user_message = Message {
            role: Role::User,
            content: self.input.clone(),
        };
        
        self.messages.push(user_message);
        
        // Add to history
        if !self.input.trim().is_empty() {
            self.input_history.push_front(self.input.clone());
            if self.input_history.len() > 50 {
                self.input_history.pop_back();
            }
            self.history_index = 0;
        }
        
        self.input.clear();
        self.is_loading = true;
        
        Ok(())
    }
    
    pub async fn get_llm_response(&mut self) -> Result<()> {
        if !self.is_loading || self.messages.is_empty() {
            return Ok(());
        }
        
        let response = if self.use_tools && !self.tools.is_empty() {
            self.llm_client.generate_response_with_tools(&self.messages, &self.tools).await?
        } else {
            self.llm_client.generate_response(&self.messages).await?
        };
        
        let assistant_message = Message {
            role: Role::Assistant,
            content: response.clone(),
        };
        
        self.messages.push(assistant_message);
        self.current_response = response;
        self.is_loading = false;
        
        Ok(())
    }
    
    pub fn previous_input(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        
        if self.history_index < self.input_history.len() {
            self.input = self.input_history[self.history_index].clone();
            self.history_index += 1;
        }
    }
    
    pub fn next_input(&mut self) {
        if self.input_history.is_empty() || self.history_index == 0 {
            self.input.clear();
            return;
        }
        
        self.history_index -= 1;
        if self.history_index == 0 {
            self.input.clear();
        } else {
            self.input = self.input_history[self.history_index - 1].clone();
        }
    }
}