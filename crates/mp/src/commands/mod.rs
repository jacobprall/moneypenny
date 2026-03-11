//! Command handlers — each subcommand has its own module.

pub use crate::context::CommandContext;

pub mod init;
pub mod setup;
pub mod hook;
pub mod start;
pub mod stop;
pub mod serve;
pub mod agent;
pub mod brain;
pub mod chat;
pub mod experience;
pub mod focus;
pub mod send;
pub mod facts;
pub mod ingest;
pub mod knowledge;
pub mod skill;
pub mod policy;
pub mod job;
pub mod embeddings;
pub mod audit;
pub mod sync;
pub mod fleet;
pub mod mpq;
pub mod db;
pub mod session;
pub mod health;
pub mod doctor;
