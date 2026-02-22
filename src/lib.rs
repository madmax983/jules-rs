//! Rust client for the Jules API.

mod client;
mod error;
mod exporter;
mod types;

#[cfg(feature = "analytics")]
mod analytics;

#[cfg(feature = "visualizer")]
mod visualizer;

pub use client::{JulesClient, JulesClientBuilder};
pub use client::{RetryPolicy, TimeoutPolicy};
pub use error::{ApiError, JulesError};
pub use exporter::SessionExporter;
pub use types::*;

#[cfg(feature = "analytics")]
pub use analytics::{SessionAnalyzer, SessionStats};

#[cfg(feature = "visualizer")]
pub use visualizer::SessionVisualizer;
