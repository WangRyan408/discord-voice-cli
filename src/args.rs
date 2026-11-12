use clap::Parser;
use serenity::model::id::{ChannelId, GuildId};

#[derive(Parser)]
#[command(
    name = "discord-voice-cli",
    author,
    version,
    about = "A headless client for Discord voice chat"
)]
pub struct Args {
    /// Discord user token (without a Bot or Bearer prefix)
    #[arg(long, env = "DISCORD_TOKEN", hide_env_values = true)]
    pub token: String,

    /// ID of the guild to connect to
    #[arg(long = "guild_id", visible_alias = "guild-id", env = "DISCORD_GUILD")]
    pub guild_id: GuildId,

    /// ID of the voice channel to connect to
    #[arg(
        long = "channel_id",
        visible_alias = "channel-id",
        env = "DISCORD_CHANNEL"
    )]
    pub channel_id: ChannelId,
}

pub fn get_args() -> Args {
    Args::parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_existing_flags() {
        let args = Args::try_parse_from([
            "discord-voice-cli",
            "--token",
            "test-token",
            "--guild_id",
            "123",
            "--channel_id",
            "456",
        ])
        .unwrap();
        assert_eq!(args.token, "test-token");
        assert_eq!(args.guild_id.get(), 123);
        assert_eq!(args.channel_id.get(), 456);
    }

    #[test]
    fn accepts_hyphenated_aliases() {
        let args = Args::try_parse_from([
            "discord-voice-cli",
            "--token",
            "test-token",
            "--guild-id",
            "123",
            "--channel-id",
            "456",
        ])
        .unwrap();
        assert_eq!(args.guild_id.get(), 123);
        assert_eq!(args.channel_id.get(), 456);
    }

    #[test]
    fn rejects_invalid_ids() {
        for id in ["0", "invalid", "-1", "18446744073709551616"] {
            assert!(
                Args::try_parse_from([
                    "discord-voice-cli",
                    "--token",
                    "test-token",
                    "--guild_id",
                    id,
                    "--channel_id",
                    "456",
                ])
                .is_err()
            );
        }
    }
}
