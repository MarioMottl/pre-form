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
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::{Frame, Terminal};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "pre-form", version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Path passed by Git hook (e.g., .git/COMMIT_EDITMSG)
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

#[derive(Clone, Default)]
struct TextInput {
    value: String,
    cursor: usize, // byte index
}
impl TextInput {
    fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
        }
    }
    fn from(s: String) -> Self {
        Self {
            cursor: s.len(),
            value: s,
        }
    }
    fn insert_char(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }
    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut idx = self.cursor - 1;
        while !self.value.is_char_boundary(idx) {
            idx -= 1;
        }
        self.value.drain(idx..self.cursor);
        self.cursor = idx;
    }
    fn delete(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let next = self.cursor + self.value[self.cursor..].chars().next().unwrap().len_utf8();
        self.value.drain(self.cursor..next);
    }
    fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut idx = self.cursor - 1;
        while !self.value.is_char_boundary(idx) {
            idx -= 1;
        }
        self.cursor = idx;
    }
    fn move_right(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let next = self.cursor + self.value[self.cursor..].chars().next().unwrap().len_utf8();
        self.cursor = next;
    }
    fn move_home(&mut self) {
        self.cursor = 0;
    }
    fn move_end(&mut self) {
        self.cursor = self.value.len();
    }
}

enum OverlayTarget {
    NewType,
    NewScope,
}
struct Overlay {
    target: OverlayTarget,
    input: TextInput,
}

struct App {
    types: Vec<String>,
    type_idx: usize,
    scope: TextInput,
    description: TextInput,
    body: TextInput,
    footer: TextInput,
    focus: Focus,
    overlay: Option<Overlay>,
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
            scope: TextInput::new(),
            description: TextInput::new(),
            body: TextInput::new(),
            footer: TextInput::new(),
            focus: Focus::Type,
            overlay: None,
        }
    }

    fn commit_message(&self) -> String {
        let scope = &self.scope.value;
        let description = &self.description.value;
        let body = &self.body.value;
        let footer = &self.footer.value;

        let prefix = if scope.is_empty() {
            format!("{}: {}", self.types[self.type_idx], description)
        } else {
            format!("{}({}): {}", self.types[self.type_idx], scope, description)
        };
        let mut msg = prefix;
        if !body.is_empty() {
            msg.push_str("\n\n");
            msg.push_str(body);
        }
        if !footer.is_empty() {
            msg.push_str("\n\n");
            msg.push_str(footer);
        }
        msg
    }
}

// ---------- persistence helpers ----------
fn preform_dir() -> PathBuf {
    PathBuf::from(".pre-form-git")
}
fn components_dir() -> PathBuf {
    preform_dir().join("components")
}
fn scopes_file() -> PathBuf {
    preform_dir().join("scopes.txt")
}

fn persist_new_type(name: &str) -> Result<()> {
    fs::create_dir_all(components_dir()).context("creating components dir failed")?;
    let p = components_dir().join(name);
    if !p.exists() {
        File::create(p).context("creating type file failed")?;
    }
    Ok(())
}

fn persist_new_scope(name: &str) -> Result<()> {
    fs::create_dir_all(preform_dir()).context("create .pre-form-git failed")?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(scopes_file())
        .context("open scopes.txt failed")?;
    writeln!(f, "{}", name).context("write scope failed")?;
    Ok(())
}

// ---------- UI ----------
fn draw_ui(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(7), // Type list
            Constraint::Length(3), // Scope
            Constraint::Length(3), // Description
            Constraint::Min(3),    // Body
            Constraint::Length(3), // Footer
        ])
        .split(area);

    // Types list (dropdown-like)
    let items: Vec<ListItem> = app
        .types
        .iter()
        .map(|t| ListItem::new(Span::raw(t)))
        .collect();
    let mut state = ListState::default();
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
                .title(Span::styled("Type  ( + to add )", title_style)),
        )
        .highlight_symbol("➡ ")
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, chunks[0], &mut state);

    // Text inputs
    let inputs: [(&str, &TextInput, Focus); 4] = [
        ("Scope  ( + to add )", &app.scope, Focus::Scope),
        ("Description", &app.description, Focus::Description),
        ("Body", &app.body, Focus::Body),
        ("Footer", &app.footer, Focus::Footer),
    ];

    for (i, (label, ti, focus)) in inputs.iter().enumerate() {
        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            *label,
            if app.focus == *focus {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ));
        let para = Paragraph::new(ti.value.as_str()).block(block);
        f.render_widget(para, chunks[i + 1]);
        if app.focus == *focus && app.overlay.is_none() {
            // cursor inside the block (1 char padding)
            let x = chunks[i + 1].x + 1 + ti.cursor as u16;
            let y = chunks[i + 1].y + 1;
            f.set_cursor_position(Position::new(x, y));
        }
    }

    // Overlay (modal) to add type/scope
    if let Some(ov) = &app.overlay {
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(3),
                Constraint::Percentage(57),
            ])
            .split(area);
        let inner_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ])
            .split(outer[1]);

        let title = match ov.target {
            OverlayTarget::NewType => "New Type (Enter to save, Esc to cancel)",
            OverlayTarget::NewScope => "New Scope (Enter to save, Esc to cancel)",
        };
        let block = Block::default().borders(Borders::ALL).title(title);
        let para = Paragraph::new(ov.input.value.as_str()).block(block);
        f.render_widget(para, inner_row[1]);

        let x = inner_row[1].x + 1 + ov.input.cursor as u16;
        let y = inner_row[1].y + 1;
        f.set_cursor_position(Position::new(x, y));
    }
}

// helpers
fn current_input_mut(app: &mut App) -> Option<&mut TextInput> {
    match app.focus {
        Focus::Scope => Some(&mut app.scope),
        Focus::Description => Some(&mut app.description),
        Focus::Body => Some(&mut app.body),
        Focus::Footer => Some(&mut app.footer),
        _ => None,
    }
}

fn maybe_open_overlay(app: &mut App) {
    app.overlay = match app.focus {
        Focus::Type => Some(Overlay {
            target: OverlayTarget::NewType,
            input: TextInput::new(),
        }),
        Focus::Scope => Some(Overlay {
            target: OverlayTarget::NewScope,
            input: TextInput::new(),
        }),
        _ => None,
    };
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
            if let Event::Key(key) = event::read().context("failed to read terminal event")? {
                // If an overlay is open, handle it first and continue.
                if let Some(ov) = &mut app.overlay {
                    match key.code {
                        KeyCode::Esc => {
                            app.overlay = None;
                        }
                        KeyCode::Enter => {
                            let name = ov.input.value.trim();
                            if !name.is_empty() {
                                match ov.target {
                                    OverlayTarget::NewType => {
                                        persist_new_type(name)?;
                                        app.types.push(name.to_string());
                                        app.type_idx = app.types.len() - 1;
                                    }
                                    OverlayTarget::NewScope => {
                                        persist_new_scope(name)?;
                                        app.scope = TextInput::from(name.to_string());
                                        app.focus = Focus::Description; // move on
                                    }
                                }
                            }
                            app.overlay = None;
                        }
                        KeyCode::Left => ov.input.move_left(),
                        KeyCode::Right => ov.input.move_right(),
                        KeyCode::Home => ov.input.move_home(),
                        KeyCode::End => ov.input.move_end(),
                        KeyCode::Delete => ov.input.delete(),
                        KeyCode::Backspace => ov.input.backspace(),
                        KeyCode::Char(c) => ov.input.insert_char(c),
                        _ => {}
                    }
                    continue;
                }

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

                    // text editing in inputs
                    KeyCode::Left => {
                        if let Some(t) = current_input_mut(&mut app) {
                            t.move_left();
                        }
                    }
                    KeyCode::Right => {
                        if let Some(t) = current_input_mut(&mut app) {
                            t.move_right();
                        }
                    }
                    KeyCode::Home => {
                        if let Some(t) = current_input_mut(&mut app) {
                            t.move_home();
                        }
                    }
                    KeyCode::End => {
                        if let Some(t) = current_input_mut(&mut app) {
                            t.move_end();
                        }
                    }
                    KeyCode::Delete => {
                        if let Some(t) = current_input_mut(&mut app) {
                            t.delete();
                        }
                    }
                    KeyCode::Backspace => {
                        if let Some(t) = current_input_mut(&mut app) {
                            t.backspace();
                        }
                    }

                    // open modal to add type/scope
                    KeyCode::Char('+') => maybe_open_overlay(&mut app),

                    KeyCode::Char(c) => match app.focus {
                        Focus::Scope | Focus::Description | Focus::Body | Focus::Footer => {
                            if let Some(t) = current_input_mut(&mut app) {
                                t.insert_char(c);
                            }
                        }
                        _ => {}
                    },

                    // finish
                    KeyCode::Enter | KeyCode::Esc => break,
                    _ => {}
                }
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
    fs::create_dir_all(hook_dir)
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
            // Accept path from git hook
            let hook_path = env::args()
                .nth(1)
                .or(args.commit_msg_path)
                .context("no hook_path provided; expected path to hooks/prepare-commit-msg")?;
            let hook_path = PathBuf::from(hook_path);
            run_tui(hook_path).context("failed while running TUI for commit message")?;
        }
    }
    Ok(())
}
