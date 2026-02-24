use std::env;
use std::fs;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use jules_rs::SessionState;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DirectorState {
    version: u32,
    product_goal: String,
    source: String,
    created_at_unix: u64,
    updated_at_unix: u64,
    policy: DirectorPolicy,
    tasks: Vec<TaskState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct DirectorPolicy {
    auto_feedback_message: String,
    max_auto_feedback_messages: u32,
    max_polls_per_task: u32,
    quality_gate_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TaskState {
    id: String,
    title: String,
    description: String,
    acceptance_criteria: Vec<String>,
    status: TaskStatus,
    attempts: u32,
    poll_attempts: u32,
    plan_approved: bool,
    feedback_messages_sent: u32,
    session_name: Option<String>,
    session_url: Option<String>,
    last_session_state: Option<SessionState>,
    commit_hash: Option<String>,
    pr_url: Option<String>,
    last_error: Option<String>,
    remediation_for_task_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum TaskStatus {
    Pending,
    Running,
    PrCandidate,
    Completed,
    NeedsRemediation,
    Failed,
    Blocked,
}

const DEFAULT_STATE_FILE: &str = ".jules-director-state.json";

struct MonitorApp {
    state_path: PathBuf,
    director_state: Option<DirectorState>,
    last_poll: Instant,
    should_quit: bool,
    error_msg: Option<String>,
}

impl MonitorApp {
    fn new(state_path: PathBuf) -> Self {
        Self {
            state_path,
            director_state: None,
            last_poll: Instant::now()
                .checked_sub(Duration::from_secs(10))
                .unwrap_or_else(Instant::now), // Force immediate poll
            should_quit: false,
            error_msg: None,
        }
    }

    fn on_tick(&mut self) {
        if self.last_poll.elapsed() >= Duration::from_secs(1) {
            self.poll_state();
            self.last_poll = Instant::now();
        }
    }

    fn poll_state(&mut self) {
        if !self.state_path.exists() {
            self.error_msg = Some(format!(
                "State file not found: {}",
                self.state_path.display()
            ));
            return;
        }

        match fs::read(&self.state_path) {
            Ok(bytes) => match serde_json::from_slice::<DirectorState>(&bytes) {
                Ok(state) => {
                    self.director_state = Some(state);
                    self.error_msg = None;
                }
                Err(e) => {
                    self.error_msg = Some(format!("Failed to parse state: {e}"));
                }
            },
            Err(e) => {
                self.error_msg = Some(format!("Failed to read state: {e}"));
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let state_path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        env::var("JULES_DIRECTOR_STATE")
            .map_or_else(|_| PathBuf::from(DEFAULT_STATE_FILE), PathBuf::from)
    };

    // Setup Terminal
    let mut stdout = io::stdout();
    let _ = enable_raw_mode();
    let _ = stdout.execute(EnterAlternateScreen);
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to init terminal: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut app = MonitorApp::new(state_path);

    let res = run_app(&mut terminal, &mut app);

    // Restore Terminal
    let _ = disable_raw_mode();
    let _ = io::stdout().execute(LeaveAlternateScreen);

    if let Err(e) = res {
        eprintln!("Error: {e}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut MonitorApp,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                        _ => {}
                    }
                }
            }
        }

        app.on_tick();

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn ui(f: &mut ratatui::Frame, app: &MonitorApp) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Body
            Constraint::Length(3), // Footer
        ])
        .split(size);

    // Header
    let title = if let Some(state) = &app.director_state {
        format!("Jules Director: {}", state.product_goal)
    } else {
        "Jules Director Monitor (Waiting for state...)".to_string()
    };

    let header = Paragraph::new(title)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL).title("Monitor"));
    f.render_widget(header, chunks[0]);

    // Body
    if let Some(msg) = &app.error_msg {
        let p = Paragraph::new(msg.as_str())
            .style(Style::default().fg(Color::Red))
            .block(Block::default().borders(Borders::ALL).title("Error"));
        f.render_widget(p, chunks[1]);
    } else if let Some(state) = &app.director_state {
        let tasks: Vec<ListItem> = state
            .tasks
            .iter()
            .map(|t| {
                let status_style = match t.status {
                    TaskStatus::Completed | TaskStatus::PrCandidate => {
                        Style::default().fg(Color::Green)
                    }
                    TaskStatus::Running => Style::default().fg(Color::Yellow),
                    TaskStatus::Failed | TaskStatus::Blocked => Style::default().fg(Color::Red),
                    TaskStatus::NeedsRemediation => Style::default().fg(Color::Magenta),
                    TaskStatus::Pending => Style::default().fg(Color::Gray),
                };

                let content = format!(
                    "[{:?}] {} ({}) {}",
                    t.status,
                    t.title,
                    t.id,
                    t.session_name.as_deref().unwrap_or("")
                );
                ListItem::new(content).style(status_style)
            })
            .collect();

        let list = List::new(tasks)
            .block(Block::default().borders(Borders::ALL).title("Tasks"))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));
        f.render_widget(list, chunks[1]);
    }

    // Footer
    let info = if let Some(state) = &app.director_state {
        let updated = state.updated_at_unix;
        format!("Last update: UNIX {updated} | Press 'q' to quit")
    } else {
        "Press 'q' to quit".to_string()
    };
    let footer = Paragraph::new(info).block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_state() {
        let json = r#"{
  "version": 1,
  "product_goal": "Test Goal",
  "source": "sources/test",
  "created_at_unix": 100,
  "updated_at_unix": 200,
  "policy": {
    "auto_feedback_message": "msg",
    "max_auto_feedback_messages": 5,
    "max_polls_per_task": 10,
    "quality_gate_commands": []
  },
  "tasks": [
    {
      "id": "t-1",
      "title": "Task 1",
      "description": "Desc",
      "acceptance_criteria": [],
      "status": "PENDING",
      "attempts": 0,
      "poll_attempts": 0,
      "plan_approved": false,
      "feedback_messages_sent": 0,
      "session_name": null,
      "session_url": null,
      "last_session_state": null,
      "commit_hash": null,
      "pr_url": null,
      "last_error": null,
      "remediation_for_task_id": null
    }
  ]
}"#;
        let state: DirectorState = serde_json::from_str(json).expect("deserialize");
        assert_eq!(state.product_goal, "Test Goal");
        assert_eq!(state.tasks.len(), 1);
        assert_eq!(state.tasks[0].status, TaskStatus::Pending);
    }
}
