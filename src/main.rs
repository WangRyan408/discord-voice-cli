use std::sync::Arc;

use serenity::all::{Client, Context, EventHandler, GatewayIntents, Ready};
use serenity::async_trait;
use songbird::{Call, SerenityInit, Songbird};
use tokio::sync::{Notify, oneshot};

mod args;
mod audio;
mod receiver;
mod sender;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

struct Handler(Arc<Notify>);

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        self.0.notify_one();
    }
}

fn main() -> Result<(), Error> {
    // Load the environment before Tokio starts any worker threads.
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        // Dotenv parser errors can contain the full line, including credentials.
        return Err("Failed to load .env; check its syntax and read permissions".into());
    }
    run(args::get_args())
}

#[tokio::main]
async fn run(args: args::Args) -> Result<(), Error> {
    let ready = Arc::new(Notify::new());
    let manager =
        Songbird::serenity_from_config(audio::voice_config(args.audio_mode, args.self_deafen));
    // The patched user-account gateway omits bot-only intents from IDENTIFY.
    let intents = GatewayIntents::empty();
    let mut client = Client::builder(&args.token, intents)
        .event_handler(Handler(Arc::clone(&ready)))
        .register_songbird_with(Arc::clone(&manager))
        .await?;
    let shard_manager = Arc::clone(&client.shard_manager);
    let mut client_task = tokio::spawn(async move { client.start().await });
    let exit = wait_for_exit();
    tokio::pin!(exit);

    let session = async {
        let connect = async {
            ready.notified().await;
            connect_voice(&manager, &args).await
        };
        let _audio_streams = tokio::select! {
            result = connect => result?,
            result = &mut exit => return result,
        };
        exit.await
    };
    let result = tokio::select! {
        result = session => result,
        result = &mut client_task => match result {
            Ok(Ok(())) => Err("Discord client stopped unexpectedly".into()),
            Ok(Err(error)) => Err(error.into()),
            Err(error) => Err(error.into()),
        },
    };

    let leave_result = if manager.get(args.guild_id).is_some() {
        manager.remove(args.guild_id).await.map_err(Error::from)
    } else {
        Ok(())
    };
    shard_manager.shutdown_all().await;
    client_task.abort();
    result?;
    leave_result?;
    println!("Disconnected");
    Ok(())
}

async fn connect_voice(manager: &Songbird, args: &args::Args) -> Result<audio::Streams, Error> {
    let audio = audio::Session::open(args.audio_mode, args.self_mute, args.self_deafen)?;
    let call = manager.get_or_insert(args.guild_id);
    {
        let mut call = call.lock().await;
        configure_voice_state(&mut call, args.self_mute, args.self_deafen).await?;
    }
    // Apply the flags before joining so the first channel join has the requested state.
    let call = manager.join(args.guild_id, args.channel_id).await?;
    let streams = {
        let mut call = call.lock().await;
        audio.attach(&mut call)
    };
    println!(
        "Voice connected (audio: {:?}, self-mute: {}, self-deafen: {}). Press Enter or Ctrl+C to disconnect.",
        args.audio_mode, args.self_mute, args.self_deafen,
    );
    Ok(streams)
}

async fn configure_voice_state(
    call: &mut Call,
    self_mute: bool,
    self_deafen: bool,
) -> Result<(), Error> {
    call.mute(self_mute).await?;
    call.deafen(self_deafen).await?;
    Ok(())
}

async fn wait_for_exit() -> Result<(), Error> {
    let (sender, receiver) = oneshot::channel();
    // A detached reader lets Ctrl+C exit even while stdin is waiting for a line.
    std::thread::spawn(move || {
        let result = std::io::stdin().read_line(&mut String::new());
        let _ = sender.send(result);
    });
    tokio::select! {
        result = wait_for_stdin(receiver) => result?,
        result = tokio::signal::ctrl_c() => result?,
    }
    Ok(())
}

async fn wait_for_stdin(receiver: oneshot::Receiver<std::io::Result<usize>>) -> Result<(), Error> {
    match receiver.await?? {
        // Services and containers often have stdin connected to /dev/null.
        // EOF is not a shutdown request; keep waiting for Ctrl+C instead.
        0 => std::future::pending().await,
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenity::all::{ChannelId, GuildId, UserId};
    use songbird::{
        error::JoinResult,
        shards::{Shard, VoiceUpdate},
    };
    use std::{
        future::Future,
        sync::Mutex,
        task::{Context, Waker},
    };

    #[derive(Debug, PartialEq)]
    struct VoiceState {
        guild_id: songbird::id::GuildId,
        channel_id: Option<songbird::id::ChannelId>,
        self_deaf: bool,
        self_mute: bool,
    }

    #[derive(Default)]
    struct VoiceUpdates(Mutex<Vec<VoiceState>>);

    #[async_trait]
    impl VoiceUpdate for VoiceUpdates {
        async fn update_voice_state(
            &self,
            guild_id: songbird::id::GuildId,
            channel_id: Option<songbird::id::ChannelId>,
            self_deaf: bool,
            self_mute: bool,
        ) -> JoinResult<()> {
            self.0.lock().unwrap().push(VoiceState {
                guild_id,
                channel_id,
                self_deaf,
                self_mute,
            });
            Ok(())
        }
    }

    #[tokio::test]
    async fn first_channel_join_uses_requested_mute_and_deafen_flags() {
        for self_mute in [false, true] {
            for self_deafen in [false, true] {
                let updates = Arc::new(VoiceUpdates::default());
                let guild = GuildId::new(123);
                let channel = ChannelId::new(456);
                let mut call = Call::new(guild, Shard::Generic(updates.clone()), UserId::new(789));
                configure_voice_state(&mut call, self_mute, self_deafen)
                    .await
                    .unwrap();
                let _join = call.join(channel).await.unwrap();
                let updates = updates.0.lock().unwrap();
                let joined: Vec<_> = updates
                    .iter()
                    .filter(|update| update.channel_id.is_some())
                    .collect();
                assert_eq!(joined.len(), 1);
                assert_eq!(
                    *joined[0],
                    VoiceState {
                        guild_id: guild.into(),
                        channel_id: Some(channel.into()),
                        self_deaf: self_deafen,
                        self_mute
                    }
                );
                assert_eq!(call.is_mute(), self_mute);
                assert_eq!(call.is_deaf(), self_deafen);
            }
        }
    }

    #[test]
    fn closed_stdin_keeps_the_service_running() {
        let (sender, receiver) = oneshot::channel();
        sender.send(Ok(0)).unwrap();
        let mut wait = Box::pin(wait_for_stdin(receiver));
        assert!(
            wait.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }

    #[tokio::test]
    async fn enter_still_requests_shutdown() {
        let (sender, receiver) = oneshot::channel();
        sender.send(Ok(1)).unwrap();
        wait_for_stdin(receiver).await.unwrap();
    }
}
