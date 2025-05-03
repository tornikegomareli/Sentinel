use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ollama_rs::{
    generation::chat::{request::ChatMessageRequest, ChatMessage},
    models::ModelOptions,
    Ollama,
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    env, 
    io, 
    sync::{Arc, Mutex},
    time::{Duration, Instant}
};

// Define the chat message structure for our TUI
#[derive(Debug, Clone)]
struct UiMessage {
    role: MessageRole,
    content: String,
    input_tokens: usize,
    output_tokens: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum MessageRole {
    User,
    Assistant,
    System,
}

impl UiMessage {
    fn new(role: MessageRole, content: String) -> Self {
        Self {
            role,
            content,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    fn user(content: String) -> Self {
        Self::new(MessageRole::User, content)
    }

    fn assistant(content: String, input_tokens: usize, output_tokens: usize) -> Self {
        let mut msg = Self::new(MessageRole::Assistant, content);
        msg.input_tokens = input_tokens;
        msg.output_tokens = output_tokens;
        msg
    }

    fn system(content: String) -> Self {
        Self::new(MessageRole::System, content)
    }
}

// Input mode for the TUI
enum InputMode {
    Normal,
    Editing,
}

// TUI Application state
struct SentinelApp {
    // Ollama client
    ollama: Ollama,
    model: String,
    
    // Message history
    messages: Vec<UiMessage>,
    
    // Input state
    input: String,
    input_history: Vec<String>,
    input_history_index: usize,
    
    // Loading state
    is_loading: bool,
    
    // Tool usage settings
    use_tools: bool,
    last_used_tools: Arc<Mutex<Vec<String>>>,
}

impl SentinelApp {
    fn new() -> Self {
        // Get host and port from environment or use defaults
        let host = env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost".to_string());
        let port = env::var("OLLAMA_PORT")
            .unwrap_or_else(|_| "11434".to_string())
            .parse::<u16>()
            .unwrap_or(11434);
        
        // Default model (use llama3.2 which is available)
        let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:latest".to_string());
        
        // Create the ollama client
        let ollama = Ollama::new(host, port);
        
        // Add a system message to start
        let mut messages = Vec::new();
        messages.push(UiMessage::system(
            "You are a helpful AI assistant.".to_string(),
        ));
        
        Self {
            ollama,
            model,
            messages,
            input: String::new(),
            input_history: Vec::new(),
            input_history_index: 0,
            is_loading: false,
            use_tools: false,
            last_used_tools: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    fn handle_input(&mut self, c: char) {
        self.input.push(c);
    }
    
    fn backspace(&mut self) {
        self.input.pop();
    }
    
    fn previous_input(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        
        if self.input_history_index > 0 {
            self.input_history_index -= 1;
            self.input = self.input_history[self.input_history_index].clone();
        }
    }
    
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
    
    fn toggle_tools(&mut self) {
        self.use_tools = !self.use_tools;
    }
    
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
    
    async fn get_llm_response(&mut self) -> Result<()> {
        if !self.is_loading {
            return Ok(());
        }
        
        // Find the last user message
        let message_index = self.messages.len() - 1;
        let user_message = &self.messages[message_index];
        
        // Convert UiMessages to ollama ChatMessages for history
        let history: Vec<ChatMessage> = self.messages
            .iter()
            .take(message_index) // All except the last message
            .map(|msg| match msg.role {
                MessageRole::User => ChatMessage::user(msg.content.clone()),
                MessageRole::Assistant => ChatMessage::assistant(msg.content.clone()),
                MessageRole::System => ChatMessage::system(msg.content.clone()),
            })
            .collect();
        
        // Create the request using the user's last message
        let user_content = user_message.content.clone();
        let chat_message = ChatMessage::user(user_content);
        
        // Build the chat request
        let chat_request = ChatMessageRequest::new(self.model.clone(), vec![chat_message])
            .options(ModelOptions::default().num_ctx(16384));
        
        // Send request using the ollama client
        let result = self.ollama
            .send_chat_messages_with_history(&mut history.clone(), chat_request)
            .await;
        
        // Process the response
        match result {
            Ok(response) => {
                // Estimate token usage based on text length
                let input_tokens = (user_message.content.len() as f32 / 4.0).ceil() as usize;
                let output_tokens = (response.message.content.len() as f32 / 4.0).ceil() as usize;
                
                // Add the assistant's response to our messages
                let assistant_message = UiMessage::assistant(
                    response.message.content,
                    input_tokens,
                    output_tokens,
                );
                
                // Add the response to our UI
                self.messages.push(assistant_message);
            }
            Err(err) => {
                // Handle errors by adding an error message
                let error_message = UiMessage::assistant(
                    format!("Error generating response: {}", err),
                    0,
                    0,
                );
                self.messages.push(error_message);
            }
        }
        
        // Reset loading state
        self.is_loading = false;
        
        Ok(())
    }
    
    fn get_current_tools(&self) -> Vec<String> {
        let tools = self.last_used_tools.lock().unwrap();
        tools.clone()
    }
}

// TUI-specific state (separate from app state)
struct TuiState {
    input_mode: InputMode,
    last_tick: Instant,
}

impl Default for TuiState {
    fn default() -> Self {
        Self {
            input_mode: InputMode::Normal,
            last_tick: Instant::now(),
        }
    }
}

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
    state.input_mode = InputMode::Editing; // Start in editing mode
    
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

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut SentinelApp,
    state: &mut TuiState,
    tick_rate: Duration,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;
        
        // Handle events
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
                            KeyCode::Char('t') => {
                                app.toggle_tools();
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
        if app.is_loading {
            app.get_llm_response().await?;
        }
        
        // Check if we need to update the tick
        if state.last_tick.elapsed() >= tick_rate {
            state.last_tick = Instant::now();
        }
    }
}

fn ui(f: &mut Frame, app: &SentinelApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Status bar
            Constraint::Min(5),    // Messages
            Constraint::Length(5), // Input box
        ])
        .split(f.size());
    
    render_status_bar(f, app, chunks[0]);
    render_messages(f, app, chunks[1]);
    render_input_box(f, app, chunks[2]);
}

fn render_status_bar(f: &mut Frame, app: &SentinelApp, area: Rect) {
    let status_text = Line::from(vec![
        Span::styled("Model: ", Style::default().fg(Color::Gray)),
        Span::styled(&app.model, Style::default().fg(Color::Green)),
        Span::styled(" | Tools: ", Style::default().fg(Color::Gray)),
        Span::styled(
            if app.use_tools { "Enabled" } else { "Disabled" },
            Style::default().fg(if app.use_tools {
                Color::Green
            } else {
                Color::Red
            }),
        ),
    ]);
    
    let status_bar =
        Paragraph::new(status_text).block(Block::default().borders(Borders::ALL).title("Sentinel"));
    
    f.render_widget(status_bar, area);
}

fn render_messages(f: &mut Frame, app: &SentinelApp, area: Rect) {
    // Split the messages area for the chat and stats
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(75), // Chat messages
            Constraint::Percentage(25), // Stats panel
        ])
        .split(area);
    
    // Create the messages view
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|msg| {
            let color = match msg.role {
                MessageRole::User => Color::Cyan,
                MessageRole::Assistant => Color::Green,
                MessageRole::System => Color::Yellow,
            };
            
            let role_span = Span::styled(
                format!(
                    "{}: ",
                    match msg.role {
                        MessageRole::User => "You",
                        MessageRole::Assistant => "Assistant",
                        MessageRole::System => "System",
                    }
                ),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            );
            
            let content_span = Span::raw(&msg.content);
            let line = Line::from(vec![role_span, content_span]);
            
            ListItem::new(Text::from(line))
        })
        .collect();
    
    let messages_list = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title("Conversation"))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");
    
    f.render_widget(messages_list, chunks[0]);
    
    // Create the stats panel - get the latest message stats if available
    let latest_message = app
        .messages
        .iter()
        .rev()
        .find(|msg| msg.role == MessageRole::Assistant);
    
    // Get values from the latest message or defaults
    let input_tokens = latest_message
        .map(|msg| msg.input_tokens.to_string())
        .unwrap_or_else(|| "0".to_string());
    
    let output_tokens = latest_message
        .map(|msg| msg.output_tokens.to_string())
        .unwrap_or_else(|| "0".to_string());
    
    // Format the list of currently used tools
    let tools_list = if app.use_tools {
        let current_tools = app.get_current_tools();
        if current_tools.is_empty() {
            "None (will be added in next iteration)".to_string()
        } else {
            current_tools.join(", ")
        }
    } else {
        "Disabled".to_string()
    };
    
    let tokens_info = vec![
        Line::from(vec![
            Span::raw("Input tokens: "),
            Span::styled(input_tokens, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::raw("Output tokens: "),
            Span::styled(output_tokens, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Tools: "),
            Span::styled(
                if app.use_tools { "Enabled" } else { "Disabled" },
                Style::default().fg(if app.use_tools {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Used: "),
            Span::styled(
                tools_list,
                Style::default().fg(Color::Gray),
            ),
        ]),
    ];
    
    let stats_widget = Paragraph::new(tokens_info)
        .block(Block::default().borders(Borders::ALL).title("Stats"))
        .wrap(Wrap { trim: true });
    
    f.render_widget(stats_widget, chunks[1]);
}

fn render_input_box(f: &mut Frame, app: &SentinelApp, area: Rect) {
    let input =
        Paragraph::new(app.input.as_str())
            .style(Style::default())
            .block(Block::default().borders(Borders::ALL).title("Input").style(
                Style::default().fg(if app.is_loading {
                    Color::DarkGray
                } else {
                    Color::White
                }),
            ));
    
    f.render_widget(input, area);
    
    // Show cursor if not loading
    if !app.is_loading {
        // Make the cursor visible and ask ratatui to put it at the specified coordinates after rendering
        f.set_cursor(
            // Put cursor past the end of the input text
            area.x + app.input.len() as u16 + 1,
            // Put cursor at the start of the input line
            area.y + 1,
        );
    }
}