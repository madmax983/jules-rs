use std::io::{self, Write};
use std::time::Duration;

use crossterm::ExecutableCommand;
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use tokio::time::sleep;

use crate::{Activity, Session};

pub struct SessionPlayer {
    pub speed: f64,
}

impl SessionPlayer {
    #[must_use]
    pub fn new(speed: f64) -> Self {
        Self { speed }
    }

    /// Plays back the session activities.
    ///
    /// # Errors
    ///
    /// Returns `io::Result` if writing to stdout fails.
    pub async fn play(&self, session: &Session, activities: &[Activity]) -> io::Result<()> {
        let mut stdout = io::stdout();
        let sorted = self.sort_activities(activities);

        stdout.execute(SetForegroundColor(Color::Cyan))?;
        writeln!(stdout, "=== PLAYING SESSION: {} ===", session.name)?;
        if let Some(prompt) = &session.prompt {
            writeln!(stdout, "PROMPT: {prompt}")?;
        }
        stdout.execute(ResetColor)?;
        writeln!(stdout)?;

        for activity in sorted {
            self.render_activity(&mut stdout, &activity).await?;
        }

        stdout.execute(SetForegroundColor(Color::Green))?;
        writeln!(stdout, "\n=== SESSION END ===")?;
        stdout.execute(ResetColor)?;

        Ok(())
    }

    #[must_use]
    pub fn sort_activities(&self, activities: &[Activity]) -> Vec<Activity> {
        let mut sorted = activities.to_vec();
        sorted.sort_by(|a, b| {
            let t1 = a.timestamp.as_deref().unwrap_or("");
            let t2 = b.timestamp.as_deref().unwrap_or("");
            t1.cmp(t2)
        });
        sorted
    }

    async fn render_activity(
        &self,
        stdout: &mut io::Stdout,
        activity: &Activity,
    ) -> io::Result<()> {
        // Skip activities with no meaningful content for replay
        if activity.overview.is_none()
            && activity.tool_use.is_none()
            && activity.output.is_none()
            && activity.view_diff.is_none()
            && activity.user_input_request.is_none()
            && activity.commit.is_none()
            && activity.create_pull_request.is_none()
        {
            return Ok(());
        }

        writeln!(stdout, "--------------------------------------------------")?;
        if let Some(ts) = &activity.timestamp {
            stdout.execute(SetForegroundColor(Color::DarkGrey))?;
            writeln!(stdout, "[{ts}] {}", activity.name)?;
            stdout.execute(ResetColor)?;
        }

        if let Some(overview) = &activity.overview {
            if let Some(thoughts) = &overview.thoughts {
                stdout.execute(SetForegroundColor(Color::Blue))?;
                write!(stdout, "🤔 THOUGHT: ")?;
                stdout.execute(ResetColor)?;
                self.typewriter_print(stdout, thoughts).await?;
                writeln!(stdout)?;
            }
        }

        if let Some(tool) = &activity.tool_use {
            stdout.execute(SetForegroundColor(Color::Yellow))?;
            writeln!(stdout, "🛠  TOOL: {}", tool.tool_name)?;
            stdout.execute(ResetColor)?;
            writeln!(stdout, "   Input: {}", tool.input)?;
        }

        if let Some(diff) = &activity.view_diff {
            stdout.execute(SetForegroundColor(Color::Magenta))?;
            writeln!(stdout, "👀 VIEW DIFF")?;
            stdout.execute(ResetColor)?;
            writeln!(stdout, "{}", diff.diff)?;
        }

        if let Some(commit) = &activity.commit {
            stdout.execute(SetForegroundColor(Color::Green))?;
            writeln!(stdout, "💾 COMMIT: {}", commit.commit_message)?;
            stdout.execute(ResetColor)?;
        }

        if let Some(pr) = &activity.create_pull_request {
            stdout.execute(SetForegroundColor(Color::Green))?;
            writeln!(stdout, "🚀 CREATE PR: {}", pr.title)?;
            stdout.execute(ResetColor)?;
            writeln!(stdout, "{}", pr.body)?;
        }

        if let Some(req) = &activity.user_input_request {
            stdout.execute(SetForegroundColor(Color::Red))?;
            writeln!(stdout, "❓ QUESTION: {}", req.question)?;
            stdout.execute(ResetColor)?;
        }

        if let Some(output) = &activity.output {
            if !output.changed_files.is_empty() {
                stdout.execute(SetForegroundColor(Color::Cyan))?;
                writeln!(stdout, "📝 OUTPUT (Changed Files):")?;
                stdout.execute(ResetColor)?;
                for file in &output.changed_files {
                    writeln!(stdout, "   - {}", file.path)?;
                }
            }
        }

        writeln!(stdout)?;
        Ok(())
    }

    async fn typewriter_print(&self, stdout: &mut io::Stdout, text: &str) -> io::Result<()> {
        let delay = if self.speed <= 0.0 {
            Duration::ZERO
        } else {
            // Base delay of 20ms divided by speed
            Duration::from_secs_f64(0.02 / self.speed)
        };

        for char in text.chars() {
            write!(stdout, "{char}")?;
            stdout.flush()?;
            if !delay.is_zero() {
                sleep(delay).await;
            }
        }
        Ok(())
    }
}
