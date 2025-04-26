mod app;
mod llm;
mod tui;
mod cli;

use anyhow::Result;
use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    
    let cli = Cli::parse();
    
    match cli.command {
        Some(command) => {
            // Handle CLI commands
        },
        None => {
            // Launch TUI
            tui::run().await?;
        }
    }
    
    Ok(())
}