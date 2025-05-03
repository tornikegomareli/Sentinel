// Re-export the public API
mod app;
mod message;
mod ui;
mod llm;

pub use app::run;

// The public modules and types users need
pub use message::{MessageRole, UiMessage};