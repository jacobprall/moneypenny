use serde::{Deserialize, Serialize};

// =========================================================================
// Channel trait & types
// =========================================================================

/// Capabilities that a channel supports (progressive enhancement).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelCapabilities {
    pub supports_threads: bool,
    pub supports_reactions: bool,
    pub supports_files: bool,
    pub supports_streaming: bool,
    pub max_message_length: Option<usize>,
}

/// An incoming message from a channel.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub channel_id: String,
    pub channel_type: String,
    pub sender: String,
    pub content: String,
    pub thread_id: Option<String>,
    pub attachments: Vec<Attachment>,
}

/// An outgoing response to a channel.
#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub channel_id: String,
    pub content: String,
    pub thread_id: Option<String>,
    pub streaming: bool,
}

#[derive(Debug, Clone)]
pub struct Attachment {
    pub name: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

/// The channel trait — minimal interface for bidirectional message adapters.
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> ChannelCapabilities;
    fn send(&self, message: &OutgoingMessage) -> anyhow::Result<()>;
}

// =========================================================================
// CLI channel
// =========================================================================

pub struct CliChannel;

impl Channel for CliChannel {
    fn name(&self) -> &str { "cli" }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_threads: false,
            supports_reactions: false,
            supports_files: false,
            supports_streaming: true,
            max_message_length: None,
        }
    }

    fn send(&self, message: &OutgoingMessage) -> anyhow::Result<()> {
        println!("{}", message.content);
        Ok(())
    }
}

// =========================================================================
// HTTP API channel (stub)
// =========================================================================

pub struct HttpApiChannel {
    pub host: String,
    pub port: u16,
}

impl Channel for HttpApiChannel {
    fn name(&self) -> &str { "http_api" }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_threads: true,
            supports_reactions: false,
            supports_files: true,
            supports_streaming: true,
            max_message_length: None,
        }
    }

    fn send(&self, _message: &OutgoingMessage) -> anyhow::Result<()> {
        Ok(())
    }
}

// =========================================================================
// Slack channel (stub)
// =========================================================================

pub struct SlackChannel {
    pub bot_token: String,
}

impl Channel for SlackChannel {
    fn name(&self) -> &str { "slack" }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_threads: true,
            supports_reactions: true,
            supports_files: true,
            supports_streaming: false,
            max_message_length: Some(40_000),
        }
    }

    fn send(&self, _message: &OutgoingMessage) -> anyhow::Result<()> {
        Ok(())
    }
}

// =========================================================================
// Discord channel (stub)
// =========================================================================

pub struct DiscordChannel {
    pub bot_token: String,
}

impl Channel for DiscordChannel {
    fn name(&self) -> &str { "discord" }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_threads: true,
            supports_reactions: true,
            supports_files: true,
            supports_streaming: false,
            max_message_length: Some(2000),
        }
    }

    fn send(&self, _message: &OutgoingMessage) -> anyhow::Result<()> {
        Ok(())
    }
}

// =========================================================================
// Channel registry
// =========================================================================

/// Select the appropriate channel by name.
pub fn format_for_channel(content: &str, caps: &ChannelCapabilities) -> String {
    match caps.max_message_length {
        Some(max) if content.len() > max => {
            let truncated: String = content.chars().take(max - 20).collect();
            format!("{truncated}\n... [truncated]")
        }
        _ => content.to_string(),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Channel capabilities
    // ========================================================================

    #[test]
    fn cli_channel_supports_streaming() {
        let cli = CliChannel;
        assert!(cli.capabilities().supports_streaming);
        assert!(!cli.capabilities().supports_threads);
        assert_eq!(cli.name(), "cli");
    }

    #[test]
    fn http_api_supports_all_major_features() {
        let http = HttpApiChannel { host: "0.0.0.0".into(), port: 8080 };
        let caps = http.capabilities();
        assert!(caps.supports_threads);
        assert!(caps.supports_files);
        assert!(caps.supports_streaming);
        assert_eq!(http.name(), "http_api");
    }

    #[test]
    fn slack_has_message_length_limit() {
        let slack = SlackChannel { bot_token: "xoxb-test".into() };
        let caps = slack.capabilities();
        assert!(caps.supports_threads);
        assert!(caps.supports_reactions);
        assert_eq!(caps.max_message_length, Some(40_000));
        assert!(!caps.supports_streaming);
    }

    #[test]
    fn discord_has_2000_char_limit() {
        let discord = DiscordChannel { bot_token: "test".into() };
        assert_eq!(discord.capabilities().max_message_length, Some(2000));
    }

    // ========================================================================
    // Channel trait is object-safe
    // ========================================================================

    #[test]
    fn channel_trait_is_object_safe() {
        let channels: Vec<Box<dyn Channel>> = vec![
            Box::new(CliChannel),
            Box::new(HttpApiChannel { host: "0.0.0.0".into(), port: 8080 }),
        ];
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].name(), "cli");
        assert_eq!(channels[1].name(), "http_api");
    }

    // ========================================================================
    // Message formatting
    // ========================================================================

    #[test]
    fn format_truncates_for_discord() {
        let caps = DiscordChannel { bot_token: "t".into() }.capabilities();
        let long = "a".repeat(3000);
        let formatted = format_for_channel(&long, &caps);
        assert!(formatted.len() <= 2000);
        assert!(formatted.contains("[truncated]"));
    }

    #[test]
    fn format_keeps_short_messages() {
        let caps = DiscordChannel { bot_token: "t".into() }.capabilities();
        let short = "hello world";
        let formatted = format_for_channel(short, &caps);
        assert_eq!(formatted, "hello world");
    }

    #[test]
    fn format_no_limit_passes_through() {
        let caps = CliChannel.capabilities();
        let long = "a".repeat(100_000);
        let formatted = format_for_channel(&long, &caps);
        assert_eq!(formatted.len(), 100_000);
    }

    // ========================================================================
    // Incoming/outgoing message construction
    // ========================================================================

    #[test]
    fn incoming_message_fields() {
        let msg = IncomingMessage {
            channel_id: "ch-1".into(),
            channel_type: "cli".into(),
            sender: "user".into(),
            content: "hello".into(),
            thread_id: None,
            attachments: vec![],
        };
        assert_eq!(msg.channel_type, "cli");
        assert!(msg.attachments.is_empty());
    }

    #[test]
    fn outgoing_message_fields() {
        let msg = OutgoingMessage {
            channel_id: "ch-1".into(),
            content: "response".into(),
            thread_id: Some("thread-1".into()),
            streaming: true,
        };
        assert!(msg.streaming);
        assert_eq!(msg.thread_id.as_deref(), Some("thread-1"));
    }
}
