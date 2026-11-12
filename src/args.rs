use clap::{ArgAction, Parser, ValueEnum};
use serenity::model::id::{ChannelId, GuildId};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum AudioMode {
    /// Silent input and discarded output, without opening any audio devices
    #[default]
    Virtual,
    /// Use the system's default microphone and output device
    Device,
}

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

    /// Audio devices to use (virtual mode needs no sound hardware or audio server)
    #[arg(long, env = "DISCORD_AUDIO_MODE", value_enum, default_value_t = AudioMode::Virtual)]
    pub audio_mode: AudioMode,

    /// Set the account's self-mute state when joining voice
    #[arg(long, env = "DISCORD_SELF_MUTE", default_value_t = true, action = ArgAction::Set)]
    pub self_mute: bool,

    /// Set the account's self-deafen state when joining voice
    #[arg(long, env = "DISCORD_SELF_DEAFEN", default_value_t = true, action = ArgAction::Set)]
    pub self_deafen: bool,
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
        assert_eq!(args.audio_mode, AudioMode::Virtual);
        assert!(args.self_mute);
        assert!(args.self_deafen);
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

    #[test]
    fn accepts_device_mode_and_explicit_voice_states() {
        let args = Args::try_parse_from([
            "discord-voice-cli",
            "--token",
            "test-token",
            "--guild-id",
            "123",
            "--channel-id",
            "456",
            "--audio-mode",
            "device",
            "--self-mute",
            "false",
            "--self-deafen",
            "false",
        ])
        .unwrap();
        assert_eq!(args.audio_mode, AudioMode::Device);
        assert!(!args.self_mute);
        assert!(!args.self_deafen);
    }

    #[test]
    fn rejects_invalid_audio_mode_and_voice_states() {
        for (flag, value) in [
            ("--audio-mode", "unknown"),
            ("--self-mute", "sometimes"),
            ("--self-deafen", "sometimes"),
        ] {
            assert!(
                Args::try_parse_from([
                    "discord-voice-cli",
                    "--token",
                    "test-token",
                    "--guild-id",
                    "123",
                    "--channel-id",
                    "456",
                    flag,
                    value,
                ])
                .is_err()
            );
        }
    }
}
