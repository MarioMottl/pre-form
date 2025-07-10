use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use std::env;
use std::fs;
use std::io;

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Type,
    Scope,
    Description,
    Body,
    Footer,
}

struct App {
    types: Vec<String>,
    type_idx: usize,
    scope: String,
    description: String,
    body: String,
    footer: String,
    focus: Focus,
}

impl App {
    fn new() -> App {
        // Load commit types from .formal-git/components directory
        let mut types = Vec::new();
        if let Ok(entries) = fs::read_dir(".formal-git/components") {
            for entry in entries.filter_map(Result::ok) {
                if let Some(name) = entry.file_name().to_str() {
                    types.push(name.to_string());
                }
            }
        }
        if types.is_empty() {
            types = vec![
                "feat".into(),
                "fix".into(),
                "docs".into(),
                "style".into(),
                "refactor".into(),
                "test".into(),
                "chore".into(),
            ];
        }
        App {
            types,
            type_idx: 0,
            scope: String::new(),
            description: String::new(),
            body: String::new(),
            footer: String::new(),
            focus: Focus::Type,
        }
    }

    fn commit_message(&self) -> String {
        let prefix = if self.scope.is_empty() {
            format!("{}: {}", self.types[self.type_idx], self.description)
        } else {
            format!(
                "{}({}): {}",
                self.types[self.type_idx], self.scope, self.description
            )
        };
        let mut msg = prefix;
        if !self.body.is_empty() {
            msg.push_str("\n\n");
            msg.push_str(&self.body);
        }
        if !self.footer.is_empty() {
            msg.push_str("\n\n");
            msg.push_str(&self.footer);
        }
        msg
    }
}

fn main() -> Result<(), io::Error> {
    // Hook mode: argument is path to write
    let args: Vec<String> = env::args().collect();
    let hook_path = if args.len() == 2 {
        Some(args[1].clone())
    } else {
        None
    };

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    loop {
        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(3),
                    Constraint::Length(3),
                ])
                .split(area);

            // Type selector
            let items: Vec<ListItem> = app
                .types
                .iter()
                .map(|t| ListItem::new(Span::raw(t)))
                .collect();
            let mut state = ratatui::widgets::ListState::default();
            state.select(Some(app.type_idx));
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(Span::styled(
                    "Type",
                    if app.focus == Focus::Type {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )))
                .highlight_symbol("➡ ")
                .highlight_style(Style::default().add_modifier(Modifier::BOLD));
            f.render_stateful_widget(list, chunks[0], &mut state);

            // Scope input
            let scope = Paragraph::new(app.scope.clone()).block(
                Block::default().borders(Borders::ALL).title(Span::styled(
                    "Scope",
                    if app.focus == Focus::Scope {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )),
            );
            f.render_widget(scope, chunks[1]);

            // Description input
            let desc = Paragraph::new(app.description.clone()).block(
                Block::default().borders(Borders::ALL).title(Span::styled(
                    "Description",
                    if app.focus == Focus::Description {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )),
            );
            f.render_widget(desc, chunks[2]);

            // Body input
            let body = Paragraph::new(app.body.clone()).block(
                Block::default().borders(Borders::ALL).title(Span::styled(
                    "Body",
                    if app.focus == Focus::Body {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )),
            );
            f.render_widget(body, chunks[3]);

            // Footer input
            let footer = Paragraph::new(app.footer.clone()).block(
                Block::default().borders(Borders::ALL).title(Span::styled(
                    "Footer",
                    if app.focus == Focus::Footer {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )),
            );
            f.render_widget(footer, chunks[4]);
        })?;

        // Input handling
        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Tab => {
                        app.focus = match app.focus {
                            Focus::Type => Focus::Scope,
                            Focus::Scope => Focus::Description,
                            Focus::Description => Focus::Body,
                            Focus::Body => Focus::Footer,
                            Focus::Footer => Focus::Type,
                        }
                    }
                    KeyCode::Up if app.focus == Focus::Type => {
                        if app.type_idx > 0 {
                            app.type_idx -= 1;
                        }
                    }
                    KeyCode::Down if app.focus == Focus::Type => {
                        if app.type_idx + 1 < app.types.len() {
                            app.type_idx += 1;
                        }
                    }
                    KeyCode::Char(c) => match app.focus {
                        Focus::Scope => app.scope.push(c),
                        Focus::Description => app.description.push(c),
                        Focus::Body => app.body.push(c),
                        Focus::Footer => app.footer.push(c),
                        _ => {}
                    },
                    KeyCode::Backspace => match app.focus {
                        Focus::Scope => {
                            app.scope.pop();
                        }
                        Focus::Description => {
                            app.description.pop();
                        }
                        Focus::Body => {
                            app.body.pop();
                        }
                        Focus::Footer => {
                            app.footer.pop();
                        }
                        _ => {}
                    },
                    KeyCode::Enter | KeyCode::Esc => break,
                    _ => {}
                }
            }
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

    // Generate or write commit message
    let msg = app.commit_message();
    if let Some(path) = hook_path {
        fs::write(path, msg).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    } else {
        println!("{}", msg);
    }
    Ok(())
}

