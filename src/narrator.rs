use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};

use crate::{Activity, ActivityStatus, Session, SessionState};

/// Transforms a session into a narrative story.
#[derive(Debug, Default)]
pub struct SessionNarrator;

/// A structured story representing the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Story {
    pub title: String,
    pub overview: String,
    pub chapters: Vec<Chapter>,
    pub epilogue: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    pub title: String,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub timestamp: Option<String>,
    pub description: String,
    pub sentiment: Sentiment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sentiment {
    Neutral,
    Positive,
    Negative,
    Heroic,
}

#[allow(clippy::unused_self)]
impl SessionNarrator {
    /// Creates a new narrator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Narrates the session.
    #[must_use]
    pub fn narrate(&self, session: &Session, activities: &[Activity]) -> Story {
        let title = session
            .title
            .clone()
            .unwrap_or_else(|| "The Untitled Quest".to_string());

        let overview = if let Some(prompt) = &session.prompt {
            format!("Our story begins with a request: \"{prompt}\"")
        } else {
            "Our story begins in media res, with no recorded prompt.".to_string()
        };

        let mut chapters_map: HashMap<String, Vec<&Activity>> = HashMap::new();
        let mut stage_order: Vec<String> = Vec::new();
        let mut untitled_activities = Vec::new();

        // Group by stage name
        for activity in activities {
            if let Some(stage) = &activity.stage_name {
                if !chapters_map.contains_key(stage) {
                    stage_order.push(stage.clone());
                }
                chapters_map
                    .entry(stage.clone())
                    .or_default()
                    .push(activity);
            } else {
                untitled_activities.push(activity);
            }
        }

        let mut chapters = Vec::new();

        // Process named stages in order of appearance
        for stage_name in stage_order {
            if let Some(acts) = chapters_map.get(&stage_name) {
                chapters.push(self.create_chapter(&stage_name, acts));
            }
        }

        // Process untitled activities if any
        if !untitled_activities.is_empty() {
            chapters.push(self.create_chapter("The Uncharted Territory", &untitled_activities));
        }

        // If no chapters were created (e.g. no stages), just group all into one
        if chapters.is_empty() && !activities.is_empty() {
            chapters
                .push(self.create_chapter("The Journey", &activities.iter().collect::<Vec<_>>()));
        }

        let epilogue = match session.state {
            Some(SessionState::Completed) => "And so, the task was completed. The code was committed, and peace returned to the repository.".to_string(),
            Some(SessionState::Failed) => "Alas, the quest failed. But every failure is a lesson for the next attempt.".to_string(),
            Some(SessionState::InProgress) => "The story is not over yet. The agent continues its work...".to_string(),
            _ => "The ending is yet to be written.".to_string(),
        };

        Story {
            title,
            overview,
            chapters,
            epilogue,
        }
    }

    fn create_chapter(&self, title: &str, activities: &[&Activity]) -> Chapter {
        let events = activities
            .iter()
            .map(|act| self.create_event(act))
            .collect();
        Chapter {
            title: title.to_string(),
            events,
        }
    }

    fn create_event(&self, activity: &Activity) -> Event {
        let (description, sentiment) = if let Some(tool) = &activity.tool_use {
            (
                format!("The agent wielded the tool `{}`.", tool.tool_name),
                Sentiment::Neutral,
            )
        } else if activity.view_diff.is_some() {
            (
                "The agent examined the changes in the code, looking for truth.".to_string(),
                Sentiment::Neutral,
            )
        } else if let Some(commit) = &activity.commit {
            (
                format!(
                    "With confidence, the agent committed the changes: \"{}\"",
                    commit.commit_message
                ),
                Sentiment::Heroic,
            )
        } else if let Some(pr) = &activity.create_pull_request {
            (
                format!("A Pull Request was forged: \"{}\"", pr.title),
                Sentiment::Heroic,
            )
        } else if let Some(req) = &activity.user_input_request {
            (
                format!("The agent paused to ask the oracle: \"{}\"", req.question),
                Sentiment::Neutral,
            )
        } else if let Some(overview) = &activity.overview {
            if let Some(thoughts) = &overview.thoughts {
                let short_thought = thoughts
                    .lines()
                    .next()
                    .unwrap_or("thinking...")
                    .chars()
                    .take(100)
                    .collect::<String>();
                (
                    format!("The agent pondered: \"{short_thought}...\""),
                    Sentiment::Neutral,
                )
            } else {
                (activity.name.clone(), Sentiment::Neutral)
            }
        } else {
            (activity.name.clone(), Sentiment::Neutral)
        };

        // Override sentiment based on status
        let final_sentiment = match activity.status {
            Some(ActivityStatus::Failed) => Sentiment::Negative,
            Some(ActivityStatus::Success) if sentiment == Sentiment::Neutral => Sentiment::Positive,
            _ => sentiment,
        };

        // Append error note if failed
        let final_description = if final_sentiment == Sentiment::Negative {
            format!("{description} But it failed.")
        } else {
            description
        };

        Event {
            timestamp: activity.timestamp.clone(),
            description: final_description,
            sentiment: final_sentiment,
        }
    }
}

impl Display for Story {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "# {}\n", self.title)?;
        writeln!(f, "{}\n", self.overview)?;

        for chapter in &self.chapters {
            writeln!(f, "## {}\n", chapter.title)?;
            for event in &chapter.events {
                let icon = match event.sentiment {
                    Sentiment::Neutral => "🔹",
                    Sentiment::Positive => "✅",
                    Sentiment::Negative => "❌",
                    Sentiment::Heroic => "🏆",
                };
                let ts = event.timestamp.as_deref().unwrap_or("Unknown time");
                writeln!(f, "{} **{}** - {}", icon, ts, event.description)?;
            }
            writeln!(f)?;
        }

        writeln!(f, "### Epilogue\n")?;
        writeln!(f, "{}", self.epilogue)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Activity, ActivityStatus, Commit, SessionState, ToolUse};

    fn mock_session() -> Session {
        Session {
            name: "sessions/test-1".to_string(),
            id: Some("test-1".to_string()),
            prompt: Some("Fix the bug".to_string()),
            title: Some("The Bug Fix".to_string()),
            description: None,
            state: Some(SessionState::Completed),
            source_context: None,
            require_plan_approval: None,
            automation_mode: None,
            create_time: None,
            update_time: None,
            url: None,
            source: None,
            plan: None,
            output: None,
            outputs: vec![],
        }
    }

    #[test]
    fn test_narrate_simple_story() {
        let session = mock_session();
        let activities = vec![
            Activity {
                name: "Act 1".to_string(),
                status: Some(ActivityStatus::Success),
                stage_name: Some("Investigation".to_string()),
                activity_type: None,
                detail: None,
                timestamp: Some("10:00".to_string()),
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: Some(ToolUse {
                    tool_name: "ls".to_string(),
                    input: ".".to_string(),
                }),
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
            Activity {
                name: "Act 2".to_string(),
                status: Some(ActivityStatus::Success),
                stage_name: Some("Resolution".to_string()),
                activity_type: None,
                detail: None,
                timestamp: Some("10:05".to_string()),
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: None,
                view_diff: None,
                commit: Some(Commit {
                    commit_message: "Fixed it".to_string(),
                }),
                create_pull_request: None,
                output: None,
            },
        ];

        let narrator = SessionNarrator::new();
        let story = narrator.narrate(&session, &activities);

        assert_eq!(story.title, "The Bug Fix");
        assert_eq!(story.chapters.len(), 2);
        assert_eq!(story.chapters[0].title, "Investigation");
        assert_eq!(story.chapters[1].title, "Resolution");
        assert_eq!(story.chapters[1].events[0].sentiment, Sentiment::Heroic);
    }

    #[test]
    fn test_narrate_chapter_order() {
        let session = mock_session();
        let activities = vec![
            Activity {
                name: "Act 1".to_string(),
                status: Some(ActivityStatus::Success),
                stage_name: Some("Phase B".to_string()),
                activity_type: None,
                detail: None,
                timestamp: Some("10:00".to_string()),
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: None,
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
            Activity {
                name: "Act 2".to_string(),
                status: Some(ActivityStatus::Success),
                stage_name: Some("Phase A".to_string()),
                activity_type: None,
                detail: None,
                timestamp: Some("10:05".to_string()),
                overview: None,
                plan: None,
                user_input_request: None,
                tool_use: None,
                view_diff: None,
                commit: None,
                create_pull_request: None,
                output: None,
            },
        ];

        let narrator = SessionNarrator::new();
        let story = narrator.narrate(&session, &activities);

        assert_eq!(story.chapters.len(), 2);
        assert_eq!(story.chapters[0].title, "Phase B");
        assert_eq!(story.chapters[1].title, "Phase A");
    }
}
