use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::{Frame, Terminal};
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
        let mut types = Vec::new();
        if let Ok(entries) = fs::read_dir(".pre-form-git/components") {
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

// Render the UI
fn draw_ui(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(7), // show full dropdown
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

    // Dropdown for types
    let items: Vec<ListItem> = app
        .types
        .iter()
        .map(|t| ListItem::new(Span::raw(t)))
        .collect();
    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.type_idx));
    let title_style = if app.focus == Focus::Type {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled("Type", title_style)),
        )
        .highlight_symbol("➡ ")
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, chunks[0], &mut state);

    // Text inputs with cursor positioning
    let inputs = [
        ("Scope", &app.scope, Focus::Scope),
        ("Description", &app.description, Focus::Description),
        ("Body", &app.body, Focus::Body),
        ("Footer", &app.footer, Focus::Footer),
    ];

    for (i, (label, content, focus)) in inputs.iter().enumerate() {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            *label,
            if app.focus == *focus {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ));
        let para = Paragraph::new(content.as_str()).block(block);
        f.render_widget(para, chunks[i + 1]);
        if app.focus == *focus {
            let x = chunks[i + 1].x + 1 + content.len() as u16;
            let y = chunks[i + 1].y + 1;
            f.set_cursor(x, y);
        }
    }
}

fn main() -> Result<(), io::Error> {
    let args: Vec<String> = env::args().collect();
    let hook_path = if args.len() == 2 {
        Some(args[1].clone())
    } else {
        None
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    loop {
        terminal.draw(|f| draw_ui(f, &app))?;
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
                    KeyCode::Backspace => {
                        if app.focus == Focus::Scope {
                            app.scope.pop();
                        } else if app.focus == Focus::Description {
                            app.description.pop();
                        } else if app.focus == Focus::Body {
                            app.body.pop();
                        } else if app.focus == Focus::Footer {
                            app.footer.pop();
                        }
                    }
                    KeyCode::Enter | KeyCode::Esc => break,
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    let msg = app.commit_message();
    if let Some(path) = hook_path {
        fs::write(path, msg).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    } else {
        println!("{}", msg);
    }
    Ok(())
}

