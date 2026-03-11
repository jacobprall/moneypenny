//! Doc generation — shared content and format-specific generators.

mod content;
mod generators;

pub use generators::{generate_agent_instructions, generate_claude_md, generate_cortex_skill};
