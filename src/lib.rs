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

#[cfg(feature = "narrator")]
mod narrator;

#[cfg(feature = "player")]
mod player;

#[cfg(feature = "doctor")]
mod doctor;

#[cfg(feature = "scout")]
mod scout;

#[cfg(feature = "comparator")]
mod comparator;

#[cfg(feature = "auditor")]
mod auditor;

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

#[cfg(feature = "narrator")]
pub use narrator::{SessionNarrator, Story};

#[cfg(feature = "player")]
pub use player::SessionPlayer;

#[cfg(feature = "doctor")]
pub use doctor::{Diagnosis, DiagnosisLevel, SessionDoctor};

#[cfg(feature = "scout")]
pub use scout::ProjectScout;

#[cfg(feature = "comparator")]
pub use comparator::{ComparisonReport, SessionComparator};

#[cfg(feature = "auditor")]
pub use auditor::{AuditReport, SessionAuditor};
