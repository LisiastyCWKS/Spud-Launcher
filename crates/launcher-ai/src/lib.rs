//! Crash-log analysis.
//!
//! Two layers, on purpose:
//!
//! * [`heuristics`] runs completely offline. It is a pure function over the log
//!   text, so it works with no network, no key and no consent prompt, and it is
//!   what the UI shows first.
//! * [`provider`] optionally asks an LLM for a written explanation. It only ever
//!   runs after the user explicitly consents to sending the (redacted) log.
//!
//! [`redact`] sits between them: paths, tokens, session ids and UUIDs are
//! stripped before any bytes leave the machine.

pub mod heuristics;
pub mod provider;
pub mod redact;

pub use heuristics::{analyze, Diagnosis, LocalAnalysis, Severity};
pub use launcher_core::settings::AiProvider;
pub use provider::{AiConfig, AiError};
pub use redact::{redact, RedactionReport};
