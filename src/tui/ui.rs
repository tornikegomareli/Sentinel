use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::tui::{
    app::SentinelApp,
    llm::ToolType,
    message::MessageRole,
};

/// Render the main UI
pub fn render_ui<B: Backend>(f: &mut Frame, app: &SentinelApp) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Status bar
            Constraint::Min(5),    // Messages
            Constraint::Length(3), // Input box
        ])
        .split(f.size());

    render_status_bar::<B>(f, app, chunks[0]);
    render_messages::<B>(f, app, chunks[1]);
    render_input_box::<B>(f, app, chunks[2]);
}

/// Render the status bar
fn render_status_bar<B: Backend>(f: &mut Frame, app: &SentinelApp, area: Rect) {
    // Create status text with model info
    let status_text = Line::from(vec![
        Span::styled("Model: ", Style::default().fg(Color::Gray)),
        Span::styled(app.model_name(), Style::default().fg(Color::Green)),
        Span::styled(" | Tools: ", Style::default().fg(Color::Gray)),
        Span::styled("Enabled", Style::default().fg(Color::Green)),
    ]);

    // Create tools display line
    let tools_line = {
        let mut tool_spans = Vec::new();
        
        // All possible tools
        let all_tools = [
            ToolType::Weather,
            ToolType::Calculator,
            ToolType::Search,
            ToolType::Scraper,
            ToolType::Finance,
        ];

        // Show tools and highlight used ones
        let current_tools = app.get_current_tools();
        
        // Create spans for each tool
        for (i, tool) in all_tools.iter().enumerate() {
            let is_used = current_tools.contains(&tool.name().to_string());
            
            // Choose color based on if the tool was used
            let color = if is_used {
                Color::Green
            } else {
                Color::DarkGray
            };
            
            // Add tool name with appropriate color
            if i > 0 {
                tool_spans.push(Span::raw(" "));
            }
            tool_spans.push(Span::styled(
                tool.name().to_string(),
                Style::default().fg(color),
            ));
        }
        
        Line::from(tool_spans)
    };

    // Create the status box
    let status_content = Text::from(vec![status_text, tools_line]);
    
    let status_bar = Paragraph::new(status_content)
        .block(Block::default().borders(Borders::ALL).title("Sentinel"));
    
    f.render_widget(status_bar, area);
}

/// Render the messages area
fn render_messages<B: Backend>(f: &mut Frame, app: &SentinelApp, area: Rect) {
    // Split the messages area for the chat and stats
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(75), // Chat messages
            Constraint::Percentage(25), // Stats panel
        ])
        .split(area);

    // Create the message list items
    let messages: Vec<ListItem> = app
        .messages()
        .iter()
        .map(|msg| {
            let color = match msg.role {
                MessageRole::User => Color::Cyan,
                MessageRole::Assistant => Color::Green,
                MessageRole::System => Color::Yellow,
            };

            let role_name = match msg.role {
                MessageRole::User => "You",
                MessageRole::Assistant => "Assistant",
                MessageRole::System => "System",
            };

            // Create role label with appropriate color
            let role_span = Span::styled(
                format!("{}: ", role_name),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            );
            
            // Create content
            let content_span = Span::raw(&msg.content);
            
            // Create text with role and content
            let mut lines = Vec::new();
            lines.push(Line::from(vec![role_span, content_span]));
            
            // Add tool usage info for assistant messages if tools were used
            if msg.role == MessageRole::Assistant && !msg.used_tools.is_empty() {
                let tools_used = format!("Tools: {}", msg.used_tools.join(", "));
                let tools_span = Span::styled(
                    tools_used,
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                );
                lines.push(Line::from(vec![Span::raw("  "), tools_span]));
            }
            
            ListItem::new(Text::from(lines))
        })
        .collect();

    // Create the messages list
    let messages_list = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title("Conversation"))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    
    f.render_widget(messages_list, chunks[0]);

    // Render the stats panel
    render_stats_panel::<B>(f, app, chunks[1]);
}

/// Render the stats panel
fn render_stats_panel<B: Backend>(f: &mut Frame, app: &SentinelApp, area: Rect) {
    // Get the latest message for stats
    let latest_message = app
        .messages()
        .iter()
        .rev()
        .find(|msg| msg.role == MessageRole::Assistant);
    
    // Get token counts
    let input_tokens = latest_message
        .map(|msg| msg.input_tokens.to_string())
        .unwrap_or_else(|| "0".to_string());
    
    let output_tokens = latest_message
        .map(|msg| msg.output_tokens.to_string())
        .unwrap_or_else(|| "0".to_string());
    
    // Get used tools
    let used_tools = if let Some(msg) = latest_message {
        if !msg.used_tools.is_empty() {
            msg.used_tools.join(", ")
        } else {
            "None".to_string()
        }
    } else {
        "None".to_string()
    };
    
    // Create the stats text
    let stats_text = vec![
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
            Span::styled("Tools used:", Style::default().add_modifier(Modifier::UNDERLINED)),
        ]),
        {
            let color = if used_tools == "None" { Color::DarkGray } else { Color::Green };
            Line::from(vec![
                Span::styled(
                    used_tools.clone(),
                    Style::default().fg(color),
                ),
            ])
        },
    ];
    
    // Create the stats widget
    let stats_widget = Paragraph::new(Text::from(stats_text))
        .block(Block::default().borders(Borders::ALL).title("Stats"))
        .wrap(Wrap { trim: true });
    
    f.render_widget(stats_widget, area);
}

/// Render the input box
fn render_input_box<B: Backend>(f: &mut Frame, app: &SentinelApp, area: Rect) {
    // Create the input box
    let input = Paragraph::new(app.input())
        .style(Style::default())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Input")
                .style(Style::default().fg(if app.is_loading() {
                    Color::DarkGray
                } else {
                    Color::White
                })),
        );
    
    f.render_widget(input, area);
    
    // Show cursor if not loading
    if !app.is_loading() {
        f.set_cursor(
            // Put cursor past the end of the input text
            area.x + app.input().len() as u16 + 1,
            // Position at the start of the input line
            area.y + 1,
        );
    }
}