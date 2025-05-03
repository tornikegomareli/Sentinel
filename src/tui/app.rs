use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    time::{Duration, Instant},
};

use crate::tui::{
    llm::LlmHandler,
    message::UiMessage,
    ui::render_ui,
};

/// Input mode for the TUI
enum InputMode {
    Normal,
    Editing,
}

/// TUI Application state
pub struct SentinelApp {
    // LLM handler
    llm_handler: LlmHandler,
    
    // Message history
    messages: Vec<UiMessage>,
    
    // Input state
    input: String,
    input_history: Vec<String>,
    input_history_index: usize,
    
    // Loading state
    is_loading: bool,
}

impl SentinelApp {
    /// Create a new application
    fn new() -> Self {
        // Create LLM handler
        let llm_handler = LlmHandler::new();
        
        // Add a system message to start
        let mut messages = Vec::new();
        messages.push(UiMessage::system(
            "You are a helpful AI assistant.".to_string(),
        ));
        
        Self {
            llm_handler,
            messages,
            input: String::new(),
            input_history: Vec::new(),
            input_history_index: 0,
            is_loading: false,
        }
    }
    
    /// Get the current message history
    pub fn messages(&self) -> &[UiMessage] {
        &self.messages
    }
    
    /// Get the current input text
    pub fn input(&self) -> &str {
        &self.input
    }
    
    /// Check if the app is loading
    pub fn is_loading(&self) -> bool {
        self.is_loading
    }
    
    /// Get the model name
    pub fn model_name(&self) -> &str {
        self.llm_handler.model_name()
    }
    
    /// Get the current tools that were used
    pub fn get_current_tools(&self) -> Vec<String> {
        self.llm_handler.get_current_tools()
    }
    
    /// Add a character to the input
    fn handle_input(&mut self, c: char) {
        self.input.push(c);
    }
    
    /// Remove the last character from the input
    fn backspace(&mut self) {
        self.input.pop();
    }
    
    /// Go to the previous input in history
    fn previous_input(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        
        if self.input_history_index > 0 {
            self.input_history_index -= 1;
            self.input = self.input_history[self.input_history_index].clone();
        }
    }
    
    /// Go to the next input in history
    fn next_input(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        
        if self.input_history_index < self.input_history.len() - 1 {
            self.input_history_index += 1;
            self.input = self.input_history[self.input_history_index].clone();
        } else {
            self.input_history_index = self.input_history.len();
            self.input.clear();
        }
    }
    
    /// Submit the current input as a message
    fn submit_message(&mut self) -> Result<()> {
        if self.input.trim().is_empty() || self.is_loading {
            return Ok(());
        }
        
        // Add the user message to our UI
        let user_message = UiMessage::user(self.input.clone());
        self.messages.push(user_message);
        
        // Add to input history
        if !self.input.trim().is_empty() {
            self.input_history.push(self.input.clone());
            self.input_history_index = self.input_history.len();
        }
        
        // Clear the input field and set loading state
        self.input.clear();
        self.is_loading = true;
        
        Ok(())
    }
    
    /// Process the LLM response
    async fn process_response(&mut self) -> Result<()> {
        if !self.is_loading {
            return Ok(());
        }
        
        // Find the last user message
        let message_index = self.messages.len() - 1;
        let user_message = &self.messages[message_index];
        
        // Get previous conversation history
        let history = &self.messages[..message_index];
        
        // Process the message
        let response = self.llm_handler.process_message(history, user_message).await?;
        
        // Add the response to the messages
        self.messages.push(response);
        
        // Reset loading state
        self.is_loading = false;
        
        Ok(())
    }
}

/// TUI-specific state
struct TuiState {
    input_mode: InputMode,
    last_tick: Instant,
}

impl Default for TuiState {
    fn default() -> Self {
        Self {
            input_mode: InputMode::Editing, // Start in editing mode
            last_tick: Instant::now(),
        }
    }
}

/// Run the TUI application
pub async fn run() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = SentinelApp::new();
    
    // Create UI state
    let mut state = TuiState::default();
    
    // Start the main loop
    let tick_rate = Duration::from_millis(100);
    let result = run_app(&mut terminal, &mut app, &mut state, tick_rate).await;
    
    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    
    result
}

/// Main application loop
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut SentinelApp,
    state: &mut TuiState,
    tick_rate: Duration,
) -> Result<()> {
    loop {
        // Draw the UI
        terminal.draw(|f| render_ui::<CrosstermBackend<io::Stdout>>(f, app))?;
        
        // Handle events with timeout
        let timeout = tick_rate
            .checked_sub(state.last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match state.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('e') => {
                                state.input_mode = InputMode::Editing;
                            }
                            KeyCode::Char('q') => {
                                return Ok(());
                            }
                            _ => {}
                        },
                        InputMode::Editing => match key.code {
                            KeyCode::Enter => {
                                app.submit_message()?;
                            }
                            KeyCode::Esc => {
                                state.input_mode = InputMode::Normal;
                            }
                            KeyCode::Char(c) => {
                                app.handle_input(c);
                            }
                            KeyCode::Backspace => {
                                app.backspace();
                            }
                            KeyCode::Up => {
                                app.previous_input();
                            }
                            KeyCode::Down => {
                                app.next_input();
                            }
                            _ => {}
                        },
                    }
                }
            }
        }
        
        // Process LLM response if loading
        if app.is_loading() {
            app.process_response().await?;
        }
        
        // Update tick
        if state.last_tick.elapsed() >= tick_rate {
            state.last_tick = Instant::now();
        }
    }
}