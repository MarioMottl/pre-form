use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Position};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::{Frame, Terminal};
use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "pre-form", version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    // Path passed by Git hook (e.g., .git/COMMIT_EDITMSG)
    #[arg()]
    commit_msg_path: Option<String>,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    Install,
}

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
            f.set_cursor_position(Position::new(x, y));
        }
    }
}

fn run_tui(hook_path: PathBuf) -> Result<()> {
    enable_raw_mode().context("failed to enable raw mode")?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("failed to enter alternate screen / enable mouse capture")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to initialize TUI terminal")?;

    let mut app = App::new();
    loop {
        terminal
            .draw(|f| draw_ui(f, &app))
            .context("failed to draw TUI frame")?;

        if event::poll(Duration::from_millis(200)).context("failed to poll for terminal events")? {
            match event::read().context("failed to read terminal event")? {
                Event::Key(key) => match key.code {
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
                },
                _ => {}
            }
        }
    }

    // restore terminal state
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .context("failed to leave alternate screen / disable mouse capture")?;
    terminal
        .show_cursor()
        .context("failed to show terminal cursor")?;

    // write out the commit message
    let msg = app.commit_message();
    fs::write(&hook_path, msg).with_context(|| {
        format!(
            "failed to write commit message to `{}`",
            hook_path.display()
        )
    })?;

    Ok(())
}

pub fn install_hook() -> Result<()> {
    let hook_dir = Path::new(".git/hooks");

    fs::create_dir_all(&hook_dir)
        .with_context(|| format!("failed to create directory `{}`", hook_dir.display()))?;

    let hook_path = hook_dir.join("prepare-commit-msg");
    let script = r#"#!/bin/sh
# pre-form Git hook: generates commit message via TUI
if [ -z "$2" ]; then
  pre-form "$1"
fi
"#;

    let mut file = File::create(&hook_path)
        .with_context(|| format!("failed to create hook file `{}`", hook_path.display()))?;

    file.write_all(script.as_bytes())
        .with_context(|| format!("failed to write to `{}`", hook_path.display()))?;

    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("failed to set permissions on `{}`", hook_path.display()))?;

    println!("Git hook installed successfully at {}", hook_path.display());
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Some(Command::Install) => {
            install_hook().context("failed to install git hook")?;
        }
        None => {
            let hook_path = env::args()
                .nth(1)
                .context("no hook_path provided; expected path to hooks/prepare-commit-msg")?;
            let hook_path = PathBuf::from(hook_path);

            run_tui(hook_path).context("failed while running TUI for commit message")?;
        }
    }

    Ok(())
}
