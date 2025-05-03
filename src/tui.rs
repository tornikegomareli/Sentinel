use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ollama_rs::{
    coordinator::Coordinator,
    generation::chat::{request::ChatMessageRequest, ChatMessage},
    generation::tools::implementations::{Calculator, DDGSearcher, Scraper},
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
    env, io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Get the weather for a given city.
///
/// * city - City to get the weather for.
#[ollama_rs::function]
async fn get_weather(city: String) -> Result<String, Box<dyn std::error::Error + Sync + Send>> {
    Ok(reqwest::get(format!("https://wttr.in/{city}?format=%C+%t"))
        .await?
        .text()
        .await?)
}

// Define the chat message structure for our TUI
#[derive(Debug, Clone)]
struct UiMessage {
    role: MessageRole,
    content: String,
    input_tokens: usize,
    output_tokens: usize,
    used_tools: Vec<String>,
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
            used_tools: Vec::new(),
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

    fn assistant_with_tools(
        content: String,
        input_tokens: usize,
        output_tokens: usize,
        used_tools: Vec<String>,
    ) -> Self {
        let mut msg = Self::assistant(content, input_tokens, output_tokens);
        msg.used_tools = used_tools;
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

// Available tools for the LLM
#[derive(Debug, Clone, PartialEq)]
enum ToolType {
    Weather,
    Calculator,
    Search,
    Scraper,
    Finance,
}

impl ToolType {
    fn name(&self) -> &'static str {
        match self {
            ToolType::Weather => "get_weather",
            ToolType::Calculator => "Calculator",
            ToolType::Search => "DDGSearcher",
            ToolType::Scraper => "Scraper",
            ToolType::Finance => "StockScraper",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            ToolType::Weather => "Get weather information for a city",
            ToolType::Calculator => "Perform mathematical calculations",
            ToolType::Search => "Search the web using DuckDuckGo",
            ToolType::Scraper => "Scrape content from webpages",
            ToolType::Finance => "Get financial information about stocks",
        }
    }
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
    enabled_tools: Vec<ToolType>,
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

        // Include all available tools
        let enabled_tools = vec![
            ToolType::Weather,
            ToolType::Calculator,
            ToolType::Search,
            ToolType::Scraper,
            ToolType::Finance,
        ];

        Self {
            ollama,
            model,
            messages,
            input: String::new(),
            input_history: Vec::new(),
            input_history_index: 0,
            is_loading: false,
            use_tools: true, // Tools enabled by default
            enabled_tools,
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

    // Toggle global tools on/off
    fn toggle_tools(&mut self) {
        self.use_tools = !self.use_tools;
    }

    // Toggle a specific tool by its number (1-5)
    fn toggle_specific_tool(&mut self, tool_num: usize) {
        // Map tool number to tool type (1-indexed for user-friendliness)
        let tool_type = match tool_num {
            1 => Some(ToolType::Weather),
            2 => Some(ToolType::Calculator),
            3 => Some(ToolType::Search),
            4 => Some(ToolType::Scraper),
            5 => Some(ToolType::Finance),
            _ => None,
        };

        // Toggle the tool if a valid tool was selected
        if let Some(tool) = tool_type {
            // Check if the tool is already enabled
            if let Some(pos) = self.enabled_tools.iter().position(|t| t == &tool) {
                // If enabled, remove it
                self.enabled_tools.remove(pos);
            } else {
                // If disabled, add it
                self.enabled_tools.push(tool);
            }
        }
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
        let history: Vec<ChatMessage> = self
            .messages
            .iter()
            .take(message_index) // All except the last message
            .map(|msg| match msg.role {
                MessageRole::User => ChatMessage::user(msg.content.clone()),
                MessageRole::Assistant => ChatMessage::assistant(msg.content.clone()),
                MessageRole::System => ChatMessage::system(msg.content.clone()),
            })
            .collect();

        // Create user message for the request
        let user_content = user_message.content.clone();
        let chat_message = ChatMessage::user(user_content.clone());

        // Clear the tracked tools list before this new response
        {
            let mut tools = self.last_used_tools.lock().unwrap();
            tools.clear();
        }

        // Always use tools - they're enabled by default
        // Create a new Ollama client
        let ollama_client = Ollama::new(
            env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost".to_string()),
            env::var("OLLAMA_PORT")
                .unwrap_or_else(|_| "11434".to_string())
                .parse::<u16>()
                .unwrap_or(11434),
        );

        // Create a coordinator with enabled tools
        let mut coordinator = Coordinator::new(ollama_client, self.model.clone(), history)
            .options(ModelOptions::default().num_ctx(16384));

        // Add tools based on enabled settings
        for tool in &self.enabled_tools {
            coordinator = match tool {
                ToolType::Weather => coordinator.add_tool(get_weather),
                ToolType::Calculator => coordinator.add_tool(Calculator {}),
                ToolType::Search => coordinator.add_tool(DDGSearcher::new()),
                ToolType::Scraper => coordinator.add_tool(Scraper {}),
                ToolType::Finance => {
                    // Create StockScraper with default configuration
                    coordinator.add_tool(
                        ollama_rs::generation::tools::implementations::StockScraper::default(),
                    )
                }
            };
        }

        // Process with the coordinator
        match coordinator.chat(vec![chat_message]).await {
            Ok(response) => {
                // Estimate token usage based on text length
                let input_tokens = (user_message.content.len() as f32 / 4.0).ceil() as usize;
                let output_tokens = (response.message.content.len() as f32 / 4.0).ceil() as usize;

                // Identify which tools were used
                let mut used_tools = Vec::new();

                // Check for tool calls in the response
                if !response.message.tool_calls.is_empty() {
                    for tool_call in &response.message.tool_calls {
                        let tool_name = tool_call.function.name.clone();
                        if !used_tools.contains(&tool_name) {
                            used_tools.push(tool_name);
                        }
                    }
                }

                // If no explicit tool calls were found, check the content for tool usage patterns
                if used_tools.is_empty() {
                    let content = response.message.content.to_lowercase();

                    // Check for each enabled tool if it was potentially used
                    for tool in &self.enabled_tools {
                        match tool {
                            ToolType::Weather => {
                                if content.contains("weather")
                                    || content.contains("temperature")
                                    || content.contains("forecast")
                                    || content.contains("climate")
                                {
                                    used_tools.push(tool.name().to_string());
                                }
                            }
                            ToolType::Calculator => {
                                if content.contains("calculated")
                                    || content.contains("result is")
                                    || content.contains("math")
                                    || content.contains("computation")
                                    || content.contains("equals")
                                    || content.contains("calculate")
                                {
                                    used_tools.push(tool.name().to_string());
                                }
                            }
                            ToolType::Search => {
                                if content.contains("search")
                                    || content.contains("found information")
                                    || content.contains("according to")
                                    || content.contains("search results")
                                    || content.contains("online")
                                    || content.contains("internet")
                                {
                                    used_tools.push(tool.name().to_string());
                                }
                            }
                            ToolType::Scraper => {
                                if content.contains("webpage")
                                    || content.contains("website")
                                    || content.contains("web page")
                                    || content.contains("url")
                                    || content.contains("content from")
                                    || content.contains("page shows")
                                {
                                    used_tools.push(tool.name().to_string());
                                }
                            }
                            ToolType::Finance => {
                                if content.contains("stock")
                                    || content.contains("price")
                                    || content.contains("market")
                                    || content.contains("financial")
                                    || content.contains("shares")
                                    || content.contains("ticker")
                                {
                                    used_tools.push(tool.name().to_string());
                                }
                            }
                        }
                    }
                }

                // Update the last used tools
                {
                    let mut tools = self.last_used_tools.lock().unwrap();
                    *tools = used_tools.clone();
                }

                // Add the assistant's response to our messages
                let assistant_message = UiMessage::assistant_with_tools(
                    response.message.content,
                    input_tokens,
                    output_tokens,
                    used_tools,
                );

                // Add the response to our UI
                self.messages.push(assistant_message);
            }
            Err(err) => {
                // Handle errors by adding an error message
                let error_message = UiMessage::assistant(
                    format!("Error generating response with tools: {}", err),
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
            Constraint::Length(5), // Status bar with additional lines for tools and key bindings
            Constraint::Min(5),    // Messages
            Constraint::Length(5), // Input box
        ])
        .split(f.size());

    render_status_bar(f, app, chunks[0]);
    render_messages(f, app, chunks[1]);
    render_input_box(f, app, chunks[2]);
}

fn render_status_bar(f: &mut Frame, app: &SentinelApp, area: Rect) {
    // Create the first line with model info
    let status_text = Line::from(vec![
        Span::styled("Model: ", Style::default().fg(Color::Gray)),
        Span::styled(&app.model, Style::default().fg(Color::Green)),
        Span::styled(" | Tools: ", Style::default().fg(Color::Gray)),
        Span::styled("Enabled", Style::default().fg(Color::Green)),
    ]);

    // Create a second line showing available tools
    let tools_line = {
        let mut tool_spans = Vec::new();
        tool_spans.push(Span::styled(
            "Available: ",
            Style::default().fg(Color::Gray),
        ));

        // All possible tools
        let all_tools = vec![
            ToolType::Weather,
            ToolType::Calculator,
            ToolType::Search,
            ToolType::Scraper,
            ToolType::Finance,
        ];

        // Show all tools with their enabled status
        let current_tools = app.get_current_tools();

        for (i, tool) in all_tools.iter().enumerate() {
            let is_used = current_tools.contains(&tool.name().to_string());

            // Choose color based on usage
            let color = if is_used {
                // If tool was used, show as bright green
                Color::Green
            } else {
                // If not used yet, show as light green
                Color::LightGreen
            };

            tool_spans.push(Span::styled(
                tool.name().to_string(),
                Style::default().fg(color),
            ));

            // Add a separator if not the last tool
            if i < all_tools.len() - 1 {
                tool_spans.push(Span::raw(", "));
            }
        }

        Line::from(tool_spans)
    };

    let status_content = Text::from(vec![status_text, tools_line]);

    let status_bar = Paragraph::new(status_content)
        .block(Block::default().borders(Borders::ALL).title("Sentinel"));

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

            // Create base message line
            let mut lines = Vec::new();
            lines.push(Line::from(vec![role_span, content_span]));

            // Add tool usage info for assistant messages if tools were used
            if msg.role == MessageRole::Assistant && !msg.used_tools.is_empty() {
                let tools_used = format!("Tools used: {}", msg.used_tools.join(", "));
                let tools_span = Span::styled(
                    tools_used,
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                );
                lines.push(Line::from(vec![Span::raw("  "), tools_span]));
            }

            ListItem::new(Text::from(lines))
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
    let tools_list = if let Some(msg) = latest_message {
        if !msg.used_tools.is_empty() {
            msg.used_tools.join(", ")
        } else {
            let current_tools = app.get_current_tools();
            if current_tools.is_empty() {
                "None yet".to_string()
            } else {
                current_tools.join(", ")
            }
        }
    } else {
        let current_tools = app.get_current_tools();
        if current_tools.is_empty() {
            "None yet".to_string()
        } else {
            current_tools.join(", ")
        }
    };

    let tokens_info = vec![
        // Token information
        Line::from(vec![
            Span::raw("Input tokens: "),
            Span::styled(input_tokens, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::raw("Output tokens: "),
            Span::styled(output_tokens, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        // Tool status
        Line::from(vec![
            Span::raw("Tools: "),
            Span::styled("Enabled", Style::default().fg(Color::Green)),
        ]),
        // Tools used section header
        Line::from(vec![Span::styled(
            "Tools used in last response:",
            Style::default().add_modifier(Modifier::UNDERLINED),
        )]),
        // Show used tools with better formatting
        {
            let display_text = if tools_list.is_empty() || tools_list == "Disabled" {
                "None".to_string()
            } else {
                tools_list.clone() // Clone here to avoid the move
            };

            let text_color = if tools_list.contains("None") {
                Color::DarkGray
            } else {
                Color::Green
            };

            Line::from(vec![Span::styled(
                display_text,
                Style::default().fg(text_color),
            )])
        },
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
