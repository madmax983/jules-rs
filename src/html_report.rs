use std::fmt::Write;

use crate::{Activity, Session, SessionAnalyzer, SessionVisualizer};

/// Generates interactive HTML reports for sessions.
#[derive(Debug, Default)]
pub struct SessionHtmlReporter;

impl SessionHtmlReporter {
    /// Creates a new reporter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generates a complete HTML report for the session.
    #[must_use]
    pub fn generate_report(&self, session: &Session, activities: &[Activity]) -> String {
        let analyzer = SessionAnalyzer::new();
        let analytics = analyzer.analyze(session, activities);

        let visualizer = SessionVisualizer::new();
        let mermaid_diagram = visualizer.to_mermaid(session, activities);

        let title = session.title.as_deref().unwrap_or("Untitled Session");
        let session_id = &session.name;
        let status = session
            .state
            .as_ref()
            .map_or_else(|| "Unknown".to_string(), |s| format!("{s:?}"));

        let mut out = String::new();
        Self::write_header(&mut out, title, session_id);
        Self::write_overview(
            &mut out,
            title,
            &status,
            session.url.as_deref(),
            session.prompt.as_deref(),
        );
        Self::write_analytics(&mut out, &analytics);
        Self::write_visualization(&mut out, &mermaid_diagram);
        Self::write_activity_log(&mut out, activities);
        Self::write_footer(&mut out);

        out
    }

    fn write_header(out: &mut String, title: &str, session_id: &str) {
        let _ = writeln!(out, "<!DOCTYPE html>");
        let _ = writeln!(out, "<html lang=\"en\">");
        let _ = writeln!(out, "<head>");
        let _ = writeln!(out, "  <meta charset=\"UTF-8\">");
        let _ = writeln!(
            out,
            "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">"
        );
        let _ = writeln!(out, "  <title>Session Report: {title}</title>");
        let _ = writeln!(
            out,
            "  <link href=\"https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/css/bootstrap.min.css\" rel=\"stylesheet\">"
        );
        let _ = writeln!(out, "  <style>");
        let _ = writeln!(out, "    body {{ background-color: #f8f9fa; }}");
        let _ = writeln!(
            out,
            "    .card {{ margin-bottom: 1.5rem; border: none; box-shadow: 0 2px 4px rgba(0,0,0,0.05); }}"
        );
        let _ = writeln!(out, "    .status-badge {{ font-size: 0.9em; }}");
        let _ = writeln!(
            out,
            "    pre {{ background: #f1f3f5; padding: 1rem; border-radius: 4px; overflow-x: auto; }}"
        );
        let _ = writeln!(out, "    .mermaid {{ text-align: center; }}");
        let _ = writeln!(
            out,
            "    .activity-log .list-group-item {{ border-left: 4px solid transparent; }}"
        );
        let _ = writeln!(
            out,
            "    .activity-log .tool-use {{ border-left-color: #198754; }}"
        );
        let _ = writeln!(
            out,
            "    .activity-log .error {{ border-left-color: #dc3545; }}"
        );
        let _ = writeln!(
            out,
            "    .activity-log .thoughts {{ background-color: #e9ecef; font-style: italic; padding: 0.5rem; border-left: 3px solid #6c757d; }}"
        );
        let _ = writeln!(out, "  </style>");
        let _ = writeln!(out, "</head>");
        let _ = writeln!(out, "<body>");

        // Navbar
        let _ = writeln!(out, "  <nav class=\"navbar navbar-dark bg-dark mb-4\">");
        let _ = writeln!(out, "    <div class=\"container-fluid\">");
        let _ = writeln!(
            out,
            "      <span class=\"navbar-brand mb-0 h1\">Jules Session Report</span>"
        );
        let _ = writeln!(
            out,
            "      <span class=\"navbar-text text-light\">{session_id}</span>"
        );
        let _ = writeln!(out, "    </div>");
        let _ = writeln!(out, "  </nav>");

        let _ = writeln!(out, "  <div class=\"container\">");
    }

    fn write_overview(
        out: &mut String,
        title: &str,
        status: &str,
        url: Option<&str>,
        prompt: Option<&str>,
    ) {
        let _ = writeln!(out, "    <div class=\"row mb-4\">");
        let _ = writeln!(out, "      <div class=\"col\">");
        let _ = writeln!(out, "        <h1 class=\"display-6\">{title}</h1>");
        let _ = writeln!(
            out,
            "        <span class=\"badge bg-primary status-badge\">{status}</span>"
        );
        if let Some(url) = url {
            let _ = writeln!(
                out,
                "        <a href=\"{url}\" target=\"_blank\" class=\"btn btn-sm btn-outline-secondary ms-2\">View in Jules</a>"
            );
        }
        let _ = writeln!(out, "      </div>");
        let _ = writeln!(out, "    </div>");

        if let Some(prompt) = prompt {
            let _ = writeln!(out, "    <div class=\"card\">");
            let _ = writeln!(
                out,
                "      <div class=\"card-header fw-bold\">Original Prompt</div>"
            );
            let _ = writeln!(out, "      <div class=\"card-body\">");
            let _ = writeln!(
                out,
                "        <p class=\"card-text\">{}</p>",
                Self::escape_html(prompt)
            );
            let _ = writeln!(out, "      </div>");
            let _ = writeln!(out, "    </div>");
        }
    }

    fn write_analytics(out: &mut String, analytics: &crate::SessionReport) {
        let _ = writeln!(out, "    <div class=\"row mb-4\">");
        let _ = writeln!(out, "      <div class=\"col-md-3\">");
        let _ = writeln!(out, "        <div class=\"card text-center h-100\">");
        let _ = writeln!(out, "          <div class=\"card-body\">");
        let _ = writeln!(
            out,
            "            <h5 class=\"card-title text-muted\">Activities</h5>"
        );
        let _ = writeln!(
            out,
            "            <p class=\"display-4\">{}</p>",
            analytics.total_activities
        );
        let _ = writeln!(out, "          </div>");
        let _ = writeln!(out, "        </div>");
        let _ = writeln!(out, "      </div>");
        let _ = writeln!(out, "      <div class=\"col-md-3\">");
        let _ = writeln!(out, "        <div class=\"card text-center h-100\">");
        let _ = writeln!(out, "          <div class=\"card-body\">");
        let _ = writeln!(
            out,
            "            <h5 class=\"card-title text-muted\">Steps Completed</h5>"
        );
        let _ = writeln!(
            out,
            "            <p class=\"display-4\">{}/{}</p>",
            analytics.completed_steps, analytics.total_steps
        );
        let _ = writeln!(out, "          </div>");
        let _ = writeln!(out, "        </div>");
        let _ = writeln!(out, "      </div>");
        let _ = writeln!(out, "      <div class=\"col-md-3\">");
        let _ = writeln!(out, "        <div class=\"card text-center h-100\">");
        let _ = writeln!(out, "          <div class=\"card-body\">");
        let _ = writeln!(
            out,
            "            <h5 class=\"card-title text-muted\">Files Changed</h5>"
        );
        let _ = writeln!(
            out,
            "            <p class=\"display-4\">{}</p>",
            analytics.modified_files.len()
        );
        let _ = writeln!(out, "          </div>");
        let _ = writeln!(out, "        </div>");
        let _ = writeln!(out, "      </div>");
        let _ = writeln!(out, "      <div class=\"col-md-3\">");
        let _ = writeln!(out, "        <div class=\"card text-center h-100\">");
        let _ = writeln!(out, "          <div class=\"card-body\">");
        let _ = writeln!(
            out,
            "            <h5 class=\"card-title text-muted\">Tools Used</h5>"
        );
        let _ = writeln!(
            out,
            "            <p class=\"display-4\">{}</p>",
            analytics.tool_usage_counts.len()
        );
        let _ = writeln!(out, "          </div>");
        let _ = writeln!(out, "        </div>");
        let _ = writeln!(out, "      </div>");
        let _ = writeln!(out, "    </div>");
    }

    fn write_visualization(out: &mut String, mermaid_diagram: &str) {
        let _ = writeln!(out, "    <div class=\"card mb-4\">");
        let _ = writeln!(
            out,
            "      <div class=\"card-header fw-bold\">Session Flow</div>"
        );
        let _ = writeln!(out, "      <div class=\"card-body\">");
        let _ = writeln!(out, "        <div class=\"mermaid\">");
        let _ = writeln!(out, "{mermaid_diagram}");
        let _ = writeln!(out, "        </div>");
        let _ = writeln!(out, "      </div>");
        let _ = writeln!(out, "    </div>");
    }

    fn write_activity_log(out: &mut String, activities: &[Activity]) {
        let _ = writeln!(out, "    <h3 class=\"mb-3\">Activity Log</h3>");
        let _ = writeln!(out, "    <div class=\"list-group activity-log mb-5\">");

        for (i, activity) in activities.iter().enumerate() {
            let act_id = format!("act-{i}");
            let name = Self::escape_html(&activity.name);
            let detail = activity.detail.as_deref().unwrap_or("");
            let status_class = if activity.status == Some(crate::ActivityStatus::Failed) {
                "error"
            } else if activity.tool_use.is_some() {
                "tool-use"
            } else {
                ""
            };

            let _ = writeln!(out, "      <div class=\"list-group-item {status_class}\">");
            let _ = writeln!(
                out,
                "        <div class=\"d-flex w-100 justify-content-between\">"
            );
            let _ = writeln!(out, "          <h5 class=\"mb-1\">{name}</h5>");
            if let Some(ts) = &activity.timestamp {
                let _ = writeln!(out, "          <small class=\"text-muted\">{ts}</small>");
            }
            let _ = writeln!(out, "        </div>");

            if !detail.is_empty() {
                let _ = writeln!(
                    out,
                    "        <p class=\"mb-1\">{}</p>",
                    Self::escape_html(detail)
                );
            }

            // Collapsible details
            let _ = writeln!(
                out,
                "        <button class=\"btn btn-sm btn-link text-decoration-none ps-0\" type=\"button\" data-bs-toggle=\"collapse\" data-bs-target=\"#{act_id}-details\" aria-expanded=\"false\" aria-controls=\"{act_id}-details\">"
            );
            let _ = writeln!(out, "          Show Details");
            let _ = writeln!(out, "        </button>");

            let _ = writeln!(
                out,
                "        <div class=\"collapse\" id=\"{act_id}-details\">"
            );
            let _ = writeln!(out, "          <div class=\"mt-2\">");

            // Thoughts
            if let Some(overview) = &activity.overview {
                if let Some(thoughts) = &overview.thoughts {
                    let _ = writeln!(
                        out,
                        "            <div class=\"thoughts mb-2\"><strong>Thoughts:</strong><br>{}</div>",
                        Self::escape_html(thoughts).replace('\n', "<br>")
                    );
                }
            }

            // Tool Input
            if let Some(tool) = &activity.tool_use {
                let _ = writeln!(out, "            <div class=\"mb-2\">");
                let _ = writeln!(
                    out,
                    "              <strong>Tool:</strong> <code>{}</code>",
                    tool.tool_name
                );
                let _ = writeln!(
                    out,
                    "              <pre class=\"mt-1\"><code>{}</code></pre>",
                    Self::escape_html(&tool.input)
                );
                let _ = writeln!(out, "            </div>");
            }

            // Diffs
            if let Some(diff) = &activity.view_diff {
                let _ = writeln!(out, "            <div class=\"mb-2\">");
                let _ = writeln!(out, "              <strong>Diff:</strong>");
                let _ = writeln!(
                    out,
                    "              <pre class=\"mt-1\"><code>{}</code></pre>",
                    Self::escape_html(&diff.diff)
                );
                let _ = writeln!(out, "            </div>");
            }

            // Output
            if let Some(output) = &activity.output {
                if !output.changed_files.is_empty() {
                    let _ = writeln!(out, "            <div class=\"mb-2\">");
                    let _ = writeln!(out, "              <strong>Changed Files:</strong>");
                    let _ = writeln!(out, "              <ul>");
                    for file in &output.changed_files {
                        let _ =
                            writeln!(out, "                <li><code>{}</code></li>", file.path);
                    }
                    let _ = writeln!(out, "              </ul>");
                    let _ = writeln!(out, "            </div>");
                }
            }

            let _ = writeln!(out, "          </div>");
            let _ = writeln!(out, "        </div>"); // End collapse
            let _ = writeln!(out, "      </div>"); // End list-group-item
        }
        let _ = writeln!(out, "    </div>"); // End list-group
    }

    fn write_footer(out: &mut String) {
        let _ = writeln!(out, "  </div>"); // End container

        // Scripts
        let _ = writeln!(
            out,
            "  <script src=\"https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/js/bootstrap.bundle.min.js\"></script>"
        );
        let _ = writeln!(out, "  <script type=\"module\">");
        let _ = writeln!(
            out,
            "    import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@10/dist/mermaid.esm.min.mjs';"
        );
        let _ = writeln!(out, "    mermaid.initialize({{ startOnLoad: true }});");
        let _ = writeln!(out, "  </script>");
        let _ = writeln!(out, "</body>");
        let _ = writeln!(out, "</html>");
    }

    fn escape_html(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityStatus, Plan, PlanStep, SessionState, ToolUse};

    #[test]
    fn test_generate_report() {
        let session = Session {
            name: "sessions/test-1".to_string(),
            id: Some("test-1".to_string()),
            prompt: Some("Create a report".to_string()),
            title: Some("Report Generation".to_string()),
            description: None,
            state: Some(SessionState::Completed),
            source_context: None,
            require_plan_approval: None,
            automation_mode: None,
            create_time: None,
            update_time: None,
            url: Some("http://localhost".to_string()),
            source: None,
            plan: Some(Plan {
                steps: vec![PlanStep {
                    description: "Step 1".to_string(),
                    status: "completed".to_string(),
                }],
            }),
            output: None,
            outputs: vec![],
        };

        let activities = vec![Activity {
            name: "Act 1".to_string(),
            status: Some(ActivityStatus::Success),
            stage_name: None,
            activity_type: None,
            detail: Some("Did something".to_string()),
            timestamp: Some("2023-01-01T00:00:00Z".to_string()),
            overview: None,
            plan: None,
            user_input_request: None,
            tool_use: Some(ToolUse {
                tool_name: "test_tool".to_string(),
                input: "{}".to_string(),
            }),
            view_diff: None,
            commit: None,
            create_pull_request: None,
            output: None,
        }];

        let reporter = SessionHtmlReporter::new();
        let html = reporter.generate_report(&session, &activities);

        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Report Generation"));
        assert!(html.contains("mermaid.initialize"));
        assert!(html.contains("Act 1"));
        assert!(html.contains("test_tool"));
    }
}
