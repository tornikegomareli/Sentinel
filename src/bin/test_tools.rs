use anyhow::Result;
use ollama_rs::{
    coordinator::Coordinator,
    generation::chat::ChatMessage,
    generation::tools::implementations::{Calculator, DDGSearcher, Scraper, StockScraper},
    models::ModelOptions,
    Ollama,
};
use std::env;

/// Get the weather for a given city.
#[ollama_rs::function]
async fn get_weather(city: String) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    Ok(reqwest::get(format!("https://wttr.in/{city}?format=%C+%t"))
        .await?
        .text()
        .await?)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenv::dotenv().ok();

    // Get model from command line or use default
    let model = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "llama3.2:latest".to_string());

    // Create Ollama client
    let host = env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost".to_string());
    let port = env::var("OLLAMA_PORT")
        .unwrap_or_else(|_| "11434".to_string())
        .parse::<u16>()
        .unwrap_or(11434);

    let ollama_client = Ollama::new(host, port);

    // Create coordinator with tools
    println!("Using model: {}", model);
    let history = vec![];
    let mut coordinator = Coordinator::new(ollama_client, model, history)
        .options(ModelOptions::default().num_ctx(16384))
        .add_tool(get_weather)
        .add_tool(Calculator {})
        .add_tool(DDGSearcher::new())
        .add_tool(Scraper {})
        .add_tool(StockScraper::default());

    println!("Tools added to coordinator");

    // Get user input
    println!("Enter your query (something that should use tools):");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim();

    // Process with coordinator
    println!("Processing query: {}", input);
    let user_message = ChatMessage::user(input.to_string());

    match coordinator.chat(vec![user_message]).await {
        Ok(response) => {
            println!("\nResponse: {}\n", response.message.content);

            // Debug tool calls
            println!("Tool calls count: {}", response.message.tool_calls.len());

            if !response.message.tool_calls.is_empty() {
                println!("\nTool calls details:");
                for (i, tool_call) in response.message.tool_calls.iter().enumerate() {
                    println!("Tool call #{}", i + 1);
                    println!("  - Tool name: {}", tool_call.function.name);
                    println!("  - Arguments: {}", tool_call.function.arguments);
                }
            } else {
                println!("\nNo explicit tool calls were made.");

                // Check for implicit tool usage
                let content = response.message.content.to_lowercase();
                println!("\nChecking content for implicit tool usage:");

                if content.contains("weather") {
                    println!("  - Implicit weather tool usage detected");
                }

                if content.contains("calculated") || content.contains("math") {
                    println!("  - Implicit calculator tool usage detected");
                }

                if content.contains("search") || content.contains("found") {
                    println!("  - Implicit search tool usage detected");
                }

                if content.contains("webpage") || content.contains("website") {
                    println!("  - Implicit scraper tool usage detected");
                }

                if content.contains("stock") || content.contains("financial") {
                    println!("  - Implicit finance tool usage detected");
                }
            }
        }
        Err(err) => {
            println!("Error: {}", err);
        }
    }

    Ok(())
}
