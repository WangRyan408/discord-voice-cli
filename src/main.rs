use std::sync::Arc;

use serenity::all::{ChannelId, Client, Context, EventHandler, GatewayIntents, GuildId, Ready};
use serenity::async_trait;
use songbird::{Config, CoreEvent, SerenityInit, Songbird, driver::DecodeMode};
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
    let manager = Songbird::serenity_from_config(
        Config::default().decode_mode(DecodeMode::Decode(Default::default())),
    );
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
            connect_voice(&manager, args.guild_id, args.channel_id).await
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

async fn connect_voice(
    manager: &Songbird,
    guild_id: GuildId,
    channel_id: ChannelId,
) -> Result<(cpal::Stream, cpal::Stream), Error> {
    let (input, microphone_stream) = sender::Sender::capture()?;
    let (receiver, playback_stream) = receiver::Receiver::playback()?;
    let call = manager.join(guild_id, channel_id).await?;
    {
        let mut call = call.lock().await;
        call.add_global_event(CoreEvent::VoiceTick.into(), receiver);
        call.play_only_input(input);
    }
    println!("Voice connected. Press Enter or Ctrl+C to disconnect.");
    Ok((microphone_stream, playback_stream))
}

async fn wait_for_exit() -> Result<(), Error> {
    let (sender, receiver) = oneshot::channel();
    // A detached reader lets Ctrl+C exit even while stdin is waiting for a line.
    std::thread::spawn(move || {
        let result = std::io::stdin().read_line(&mut String::new());
        let _ = sender.send(result);
    });
    tokio::select! {
        result = receiver => { result??; },
        result = tokio::signal::ctrl_c() => result?,
    }
    Ok(())
}
