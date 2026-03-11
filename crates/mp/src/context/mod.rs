//! Shared context types — reduce parameter threading.

use anyhow::Result;
use mp_core::config::Config;
use std::path::Path;

/// Shared context for command execution.
pub struct CommandContext<'a> {
    pub config: &'a Config,
    pub config_path: &'a Path,
    pub project_dir: std::path::PathBuf,
}

impl<'a> CommandContext<'a> {
    pub fn new(config: &'a Config, config_path: &'a Path) -> Result<Self> {
        let project_dir = config_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        Ok(Self {
            config,
            config_path,
            project_dir,
        })
    }
}
