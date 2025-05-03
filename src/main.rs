mod app;
mod cli;
mod llm;
mod tui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use llm::{LlmClient, Message, OllamaClient, Role};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Some(command) => match command {
            Commands::Ask {
                message,
                model: _,
                tools,
            } => {
                let client = OllamaClient::new();
                let prompt = message.join(" ");

                let user_message = Message {
                    role: Role::User,
                    content: prompt,
                };

                let response = if tools {
                    println!("Using Ollama with tools...");
                    client
                        .generate_response_with_tools(&[user_message], &[])
                        .await?
                } else {
                    println!("Using Ollama without tools...");
                    client.generate_response(&[user_message]).await?
                };

                println!("\nResponse:\n{}", response);
            }
            Commands::Config { .. } => {
                println!("Configuration not yet implemented");
            }
        },
        None => {
            tui::run().await?;
        }
    }

    Ok(())
}
