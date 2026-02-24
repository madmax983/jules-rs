//! Rust client for the Jules API.

mod client;
mod error;
mod exporter;
mod types;

#[cfg(feature = "analytics")]
mod analytics;

#[cfg(feature = "visualizer")]
mod visualizer;

#[cfg(feature = "report")]
mod html_report;

#[cfg(feature = "player")]
mod player;

pub use client::{JulesClient, JulesClientBuilder};
pub use client::{RetryPolicy, TimeoutPolicy};
pub use error::{ApiError, JulesError};
pub use exporter::SessionExporter;
pub use types::*;

#[cfg(feature = "analytics")]
pub use analytics::{SessionAnalyzer, SessionReport};

#[cfg(feature = "visualizer")]
pub use visualizer::SessionVisualizer;

#[cfg(feature = "report")]
pub use html_report::SessionHtmlReporter;

#[cfg(feature = "player")]
pub use player::SessionPlayer;
