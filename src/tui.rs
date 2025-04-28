use crate::app::App;
use crate::llm::{create_client, LlmProvider};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use std::{io, time::Duration};
use tokio::time;

pub async fn run() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let llm_client = create_client(LlmProvider::Claude);
    let mut app = App::new(llm_client);

    // Run the event loop
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = time::Instant::now();

    loop {
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            break;
                        }
                        KeyCode::Char('t')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            app.toggle_tools();
                        }
                        KeyCode::Char(c) => {
                            app.handle_input(c);
                        }
                        KeyCode::Enter => {
                            app.submit_message()?;
                        }
                        KeyCode::Backspace => {
                            app.backspace();
                        }
                        KeyCode::Esc => {
                            app.clear_input();
                        }
                        KeyCode::Up => {
                            app.previous_input();
                        }
                        KeyCode::Down => {
                            app.next_input();
                        }
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            if app.is_loading {
                app.get_llm_response().await?;
            }
            last_tick = time::Instant::now();
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(f.size());

    // Create a scrollable area for chat messages
    let mut messages_text = Vec::new();

    for msg in &app.messages {
        let role = match msg.role {
            crate::llm::Role::User => "You",
            crate::llm::Role::Assistant => "Claude",
            crate::llm::Role::System => "System",
        };

        // Add role as a Line with bold style
        let mut role_line = Line::from(format!("{}: ", role));
        role_line.spans[0].style = Style::default().add_modifier(Modifier::BOLD);
        messages_text.push(role_line);

        // Add content as a separate Line(s) with normal style
        let content_lines = msg.content.split('\n');
        for content in content_lines {
            // Add indentation to content lines for better readability
            messages_text.push(Line::from(format!("  {}", content)));
        }

        // Add a blank line between messages
        messages_text.push(Line::from(""));
    }

    // Create a paragraph with all messages
    let messages = Paragraph::new(messages_text)
        .block(Block::default().borders(Borders::ALL).title("Chat"))
        .wrap(Wrap { trim: false });

    f.render_widget(messages, chunks[0]);

    // Input field
    let input_status = if app.is_loading {
        "Loading...".to_string()
    } else {
        let tools_status = if app.use_tools {
            "Tools: ON"
        } else {
            "Tools: OFF"
        };
        format!(
            "Enter message (Ctrl+Q to quit, Ctrl+T to toggle tools) | {}",
            tools_status
        )
    };

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default())
        .block(Block::default().borders(Borders::ALL).title(input_status))
        .wrap(Wrap { trim: true });

    f.render_widget(input, chunks[1]);

    // Set cursor position
    if !app.is_loading {
        f.set_cursor(chunks[1].x + app.input.len() as u16 + 1, chunks[1].y + 1);
    }
}
