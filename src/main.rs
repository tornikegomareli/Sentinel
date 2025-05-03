mod llm;
mod tools;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use llm::ollama::{LlmClient, OllamaClient};
use serde::{Deserialize, Serialize};
use tokio;

#[derive(Parser)]
#[command(name = "sentinel")]
#[command(about = "Llama based Terminal agent", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
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

                let response_text = if tools {
                    println!("Using Ollama with tools...");
                    let (text, _, _, _) = client
                        .generate_response_with_tools(&[user_message], &[])
                        .await?;
                    text
                } else {
                    println!("Using Ollama without tools...");
                    let (text, _, _) = client.generate_response(&[user_message]).await?;
                    text
                };

                println!("\nResponse:\n{}", response_text);
            }
            Commands::Config { .. } => {
                println!("Configuration not yet implemented");
            }
        },
        None => {
            // Run the TUI
            tui::run().await?;
        }
    }

    Ok(())
}
