//! Moneypenny CLI and agent runtime.
//!
//! This library exposes the agent, helpers, and context for use by integration
//! tests and other consumers.

pub mod agent;
mod adapters;
pub mod cli;
pub mod commands;
pub mod context;
mod domain_tools;
mod tools;
mod docs;
pub mod helpers;
mod intent;
pub mod sidecar;
mod ui;
pub mod worker;
