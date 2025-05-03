use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Optional command to run
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Send a one-off message and get a response
    Ask {
        /// The message to send
        #[arg(required = true)]
        message: Vec<String>,
        
        /// LLM model to use
        #[arg(short, long, default_value = "claude")]
        model: String,
        
        /// Enable tools for capable models
        #[arg(short, long)]
        tools: bool,
    },
    
    /// Set configuration options
    Config {
        /// Set the default LLM model
        #[arg(long)]
        model: Option<String>,
        
        /// Set the API key for Claude
        #[arg(long)]
        claude_key: Option<String>,
        
        /// Set the API key for OpenAI
        #[arg(long)]
        openai_key: Option<String>,
        
        /// Set the API key for Gemini
        #[arg(long)]
        gemini_key: Option<String>,
        
        /// Set the host for Ollama
        #[arg(long)]
        ollama_host: Option<String>,
        
        /// Set the port for Ollama
        #[arg(long)]
        ollama_port: Option<u16>,
        
        /// Set the model for Ollama
        #[arg(long)]
        ollama_model: Option<String>,
    },
}