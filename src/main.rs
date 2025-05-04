pub mod llm;
pub mod tools;
pub mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use llm::ollama::{LlmClient, OllamaClient};
use serde::{Deserialize, Serialize};
use std::io::Write;
use tokio;

// Terminal colors for better user experience
pub mod terminal_colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const BRIGHT_GREEN: &str = "\x1b[1;32m";
    pub const BRIGHT_BLUE: &str = "\x1b[1;34m";
    pub const BRIGHT_WHITE: &str = "\x1b[1;37m";
    pub const YELLOW: &str = "\x1b[1;33m";
    pub const CYAN: &str = "\x1b[1;36m";
    pub const MAGENTA: &str = "\x1b[1;35m";
    pub const RED: &str = "\x1b[1;31m";
}

#[derive(Parser)]
#[command(name = "sentinel")]
#[command(about = "LLM based Terminal agent", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Ask the LLM a question
    Ask {
        /// The message to send to the LLM
        #[arg(required = true)]
        message: Vec<String>,

        /// The model to use
        #[arg(short, long, default_value = "llama3.2:latest")]
        model: String,

        /// Use tools
        #[arg(short, long)]
        tools: bool,
    },

    /// Change configuration
    Config {
        /// Set the model to use
        #[arg(short, long)]
        model: Option<String>,
    },
}

// Message and Role definitions used by both the CLI and TUI
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
    #[serde(skip, default)]
    pub input_tokens: usize,
    #[serde(skip, default)]
    pub output_tokens: usize,
    #[serde(skip, default)]
    pub used_tools: Vec<String>,
}

// Agent struct that manages conversation with LLM
pub struct Agent {
    pub client: OllamaClient,
    pub model: String,
    pub conversation: Vec<Message>,
}

impl Agent {
    // Create a new agent with the specified model
    pub fn new(model: &str) -> Self {
        Self {
            client: OllamaClient::new().with_model(model),
            model: model.to_string(),
            conversation: Vec::new(),
        }
    }

    // Start the conversation loop
    pub async fn start(&mut self) -> Result<()> {
        self.print_colored_banner();
        self.print_help();

        let tools = self.client.get_available_tools();
        if !tools.is_empty() {
            self.print_info(&format!("Available tools: {}", tools.join(", ")));
        }

        self.print_divider();

        loop {
            self.print_user_prompt();

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input.is_empty() {
                continue;
            }

            // Check for commands
            if input.starts_with('/') {
                if self.process_command(input) {
                    continue;
                } else {
                    // If it's not a recognized command, treat it as regular input
                    if input.to_lowercase() == "/exit" {
                        break;
                    }
                }
            }

            // Add user message to conversation history
            let user_message = Message {
                role: Role::User,
                content: input.to_string(),
                input_tokens: 0,
                output_tokens: 0,
                used_tools: Vec::new(),
            };

            self.conversation.push(user_message);

            // Generate response with tools
            self.print_info("Processing message with tools enabled...");

            match self
                .client
                .generate_response_with_tools(&self.conversation, &[])
                .await
            {
                Ok((text, input_tokens, output_tokens, used_tools)) => {
                    // Print tool usage if any
                    if !used_tools.is_empty() {
                        self.print_info("Sentinel is using tools...");

                        println!(
                            "{}Tool usage:{}",
                            terminal_colors::MAGENTA,
                            terminal_colors::RESET
                        );
                        for tool in &used_tools {
                            println!("  - {}", tool);
                        }
                    }

                    // Print Claude's response
                    self.print_ollama_response(&text);

                    // Print token usage info
                    self.print_token_info(input_tokens, output_tokens);

                    // Add Claude's response to conversation history
                    let assistant_message = Message {
                        role: Role::Assistant,
                        content: text,
                        input_tokens,
                        output_tokens,
                        used_tools: used_tools.clone(),
                    };

                    self.conversation.push(assistant_message);
                }
                Err(e) => {
                    self.print_error(&format!("Error generating response: {}", e));
                }
            }

            self.print_divider();
        }

        Ok(())
    }

    // Process special commands (prefixed with /)
    fn process_command(&mut self, command: &str) -> bool {
        match command.to_lowercase().as_str() {
            "/exit" => {
                self.print_info("Goodbye!");
                std::process::exit(0);
            }
            "/help" => {
                self.print_help();
                true
            }
            "/clear" => {
                self.clear_conversation();
                true
            }
            "/tools" => {
                self.list_tools();
                true
            }
            _ => {
                if command.starts_with('/') {
                    self.print_error(&format!("Unknown command: {}", command));
                    self.print_info("Type /help for available commands");
                    true
                } else {
                    false
                }
            }
        }
    }

    // List available tools
    fn list_tools(&self) {
        let tools = self.client.get_available_tools();

        if tools.is_empty() {
            self.print_info("No tools available");
            return;
        }

        self.print_info("Available tools:");
        for tool in tools {
            println!(
                "  {}{}{}",
                terminal_colors::MAGENTA,
                tool,
                terminal_colors::RESET
            );
        }
    }

    // Clear conversation history
    fn clear_conversation(&mut self) {
        self.conversation.clear();
        self.print_info("Conversation cleared");
    }

    // Print user prompt
    fn print_user_prompt(&self) {
        print!(
            "\n{}User: {}",
            terminal_colors::BRIGHT_GREEN,
            terminal_colors::RESET
        );
        std::io::stdout().flush().unwrap();
    }

    // Print Claude's response
    fn print_ollama_response(&self, text: &str) {
        println!(
            "\n{}Sentinel: {}{}",
            terminal_colors::BRIGHT_BLUE,
            terminal_colors::RESET,
            text
        );
    }

    // Print token usage information
    fn print_token_info(&self, input_tokens: usize, output_tokens: usize) {
        println!(
            "\n{}(Input tokens: {}, Output tokens: {}){}",
            terminal_colors::YELLOW,
            input_tokens,
            output_tokens,
            terminal_colors::RESET
        );
    }

    // Print error message
    fn print_error(&self, message: &str) {
        println!(
            "{}Error: {}{}",
            terminal_colors::RED,
            message,
            terminal_colors::RESET
        );
    }

    // Print general information
    fn print_info(&self, message: &str) {
        println!(
            "{}{}{}",
            terminal_colors::BRIGHT_WHITE,
            message,
            terminal_colors::RESET
        );
    }

    // Print command help
    fn print_command(&self, command: &str, description: &str) {
        println!(
            "  {}{}{}  - {}",
            terminal_colors::CYAN,
            command,
            terminal_colors::RESET,
            description
        );
    }

    // Print separator line
    fn print_divider(&self) {
        println!(
            "{}-------------------------------------------{}",
            terminal_colors::BRIGHT_WHITE,
            terminal_colors::RESET
        );
    }

    // Print application banner
    fn print_colored_banner(&self) {
        println!(
            "{}{}🤖 Sentinel AI Agent{}",
            terminal_colors::BOLD,
            terminal_colors::BRIGHT_BLUE,
            terminal_colors::RESET
        );
        println!(
            "{}Model: {}{}",
            terminal_colors::BRIGHT_WHITE,
            self.model,
            terminal_colors::RESET
        );
    }

    // Print help message
    fn print_help(&self) {
        println!(
            "{}Available commands:{}",
            terminal_colors::BRIGHT_WHITE,
            terminal_colors::RESET
        );
        self.print_command("/exit", "Quit the application");
        self.print_command("/clear", "Clear the conversation history");
        self.print_command("/tools", "List available tools");
        self.print_command("/help", "Show this help message");
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file if it exists
    dotenv::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Some(command) => match command {
            Commands::Ask {
                message,
                model,
                tools,
            } => {
                let client = OllamaClient::new().with_model(&model);
                let prompt = message.join(" ");

                let user_message = Message {
                    role: Role::User,
                    content: prompt,
                    input_tokens: 0,
                    output_tokens: 0,
                    used_tools: Vec::new(),
                };

                // Print user message with colored formatting
                println!(
                    "\n{}[USER]{} {}",
                    terminal_colors::CYAN,
                    terminal_colors::RESET,
                    user_message.content
                );

                if tools {
                    println!(
                        "\n{}[SENTINEL]{} Using Ollama with tools enabled...",
                        terminal_colors::MAGENTA,
                        terminal_colors::RESET
                    );

                    let (text, input_tokens, output_tokens, used_tools) = client
                        .generate_response_with_tools(&[user_message], &[])
                        .await?;

                    // Print summary of tool usage
                    if !used_tools.is_empty() {
                        println!(
                            "\n{}[TOOL SUMMARY]{} Tools used in this response:",
                            terminal_colors::YELLOW,
                            terminal_colors::RESET
                        );

                        for tool in used_tools {
                            println!("  - {}", tool);
                        }
                    }

                    println!(
                        "\n{}[ASSISTANT]{} {}",
                        terminal_colors::BRIGHT_GREEN,
                        terminal_colors::RESET,
                        text
                    );

                    println!(
                        "\n{}[INFO]{} Tokens: {} input, {} output",
                        terminal_colors::BRIGHT_WHITE,
                        terminal_colors::RESET,
                        input_tokens,
                        output_tokens
                    );
                } else {
                    println!(
                        "\n{}[SENTINEL]{} Using Ollama without tools...",
                        terminal_colors::MAGENTA,
                        terminal_colors::RESET
                    );

                    let (text, input_tokens, output_tokens) =
                        client.generate_response(&[user_message]).await?;

                    println!(
                        "\n{}[ASSISTANT]{} {}",
                        terminal_colors::BRIGHT_GREEN,
                        terminal_colors::RESET,
                        text
                    );

                    println!(
                        "\n{}[INFO]{} Tokens: {} input, {} output",
                        terminal_colors::BRIGHT_WHITE,
                        terminal_colors::RESET,
                        input_tokens,
                        output_tokens
                    );
                };
            }
            Commands::Config { .. } => {
                println!(
                    "{}[SENTINEL]{} Configuration not yet implemented",
                    terminal_colors::MAGENTA,
                    terminal_colors::RESET
                );
            }
        },
        None => {
            // Create and start the agent
            let mut agent = Agent::new("llama3.2:latest");
            agent.start().await?;
        }
    }

    Ok(())
}
