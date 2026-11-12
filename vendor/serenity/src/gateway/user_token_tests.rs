#![allow(clippy::unwrap_used)]

#[cfg(feature = "user_token")]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "user_token")]
use futures::SinkExt;
use futures::StreamExt;
use reqwest::header::AUTHORIZATION;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::time::timeout;
#[cfg(feature = "user_token")]
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, WebSocketStream};

#[cfg(feature = "user_token")]
use super::ConnectionStage;
use super::{PresenceData, Shard};
use crate::http::{Http, HttpBuilder, LightMethod, Request, Route};
use crate::json::{from_str, from_value, json, Value};
use crate::model::event::{Event, GatewayEvent};
use crate::model::gateway::{GatewayIntents, Ready, ShardInfo};
use crate::model::id::ShardId;

#[test]
fn http_authorization_uses_the_selected_account_mode() {
    let http = Http::new("  test.user.token  ");
    let request = Request::new(Route::UserMe, LightMethod::Get)
        .build(&http.client, http.token(), None)
        .unwrap()
        .build()
        .unwrap();
    let expected = if cfg!(feature = "user_token") {
        "test.user.token"
    } else {
        "Bot test.user.token"
    };
    assert_eq!(request.headers()[AUTHORIZATION], expected);
}

#[test]
fn token_setters_use_the_same_authentication_mode() {
    let http = HttpBuilder::new("old.token.value")
        .token("  replacement.user.token  ")
        .build();
    let expected = if cfg!(feature = "user_token") {
        "replacement.user.token"
    } else {
        "Bot replacement.user.token"
    };
    assert_eq!(http.token(), expected);
    assert!(!format!("{http:?}").contains("replacement.user.token"));
}

#[test]
fn explicit_authorization_schemes_are_preserved() {
    for token in ["Bot explicit.bot.token", "Bearer explicit.oauth.token"] {
        assert_eq!(Http::new(token).token(), token);
    }
}

async fn gateway_pair(token: &str) -> (Shard, WebSocketStream<TcpStream>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    let accept = async {
        let (stream, _) = listener.accept().await.unwrap();
        accept_async(stream).await.unwrap()
    };
    let connect = Shard::new(
        Arc::new(Mutex::new(url)),
        token,
        ShardInfo::new(ShardId(0), 1),
        GatewayIntents::GUILDS | GatewayIntents::GUILD_VOICE_STATES,
        Some(PresenceData::default()),
    );
    let (shard, socket) = timeout(Duration::from_secs(5), async {
        tokio::join!(connect, accept)
    })
    .await
    .unwrap();
    (shard.unwrap(), socket)
}

async fn receive_json(socket: &mut WebSocketStream<TcpStream>) -> Value {
    let message = timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    from_str(message.to_text().unwrap()).unwrap()
}

#[tokio::test]
async fn identify_and_resume_preserve_authentication_on_the_wire() {
    let http = Http::new("test.user.token");
    let (mut shard, mut socket) = gateway_pair(http.token()).await;
    shard.identify().await.unwrap();
    let identify = receive_json(&mut socket).await;
    assert_eq!(identify["op"], 2);
    assert_eq!(identify["d"]["token"], http.token());
    assert_eq!(identify["d"]["properties"]["browser"], "serenity");
    assert_eq!(identify["d"]["properties"]["device"], "serenity");
    let payload = identify["d"].as_object().unwrap();
    for field in ["intents", "shard", "presence"] {
        assert_eq!(
            payload.contains_key(field),
            !cfg!(feature = "user_token"),
            "{field}"
        );
    }

    shard
        .client
        .send_resume(&shard.shard_info(), "test-session", 42, http.token())
        .await
        .unwrap();
    let resume = receive_json(&mut socket).await;
    assert_eq!(resume["op"], 6);
    assert_eq!(resume["d"]["token"], http.token());
    assert_eq!(resume["d"]["session_id"], "test-session");
    assert_eq!(resume["d"]["seq"], 42);
}

fn user_ready_payload() -> Value {
    json!({
        "v": 10,
        "user": { "id": "123", "username": "voice-test", "discriminator": "0", "avatar": null },
        "guilds": [{ "id": "456", "name": "Test guild" }],
        "session_id": "test-session",
        "relationships": [],
        "private_channels": []
    })
}

#[cfg(feature = "user_token")]
#[test]
fn user_ready_accepts_missing_and_null_bot_metadata() {
    for include_null in [false, true] {
        let mut payload = user_ready_payload();
        if include_null {
            payload["application"] = Value::Null;
        }
        let ready: Ready = from_value(payload).unwrap();
        assert!(ready.application.is_none());
        assert!(!ready.user.bot);
        assert_eq!(ready.user.id.get(), 123);
        assert_eq!(ready.guilds[0].id.get(), 456);
        assert_eq!(ready.resume_gateway_url, "wss://gateway.discord.gg");
    }
}

#[cfg(feature = "user_token")]
#[test]
fn user_ready_still_requires_a_session_id() {
    let mut payload = user_ready_payload();
    payload.as_object_mut().unwrap().remove("session_id");
    assert!(from_value::<Ready>(payload).is_err());
}

#[test]
fn ready_preserves_application_metadata_when_present() {
    let mut payload = user_ready_payload();
    payload["application"] = json!({ "id": "789", "flags": 0 });
    payload["resume_gateway_url"] = json!("wss://gateway-us-east1-b.discord.gg");
    payload["user"]["bot"] = json!(true);
    let ready: Ready = from_value(payload).unwrap();
    #[cfg(feature = "user_token")]
    let application = ready.application.as_ref().unwrap();
    #[cfg(not(feature = "user_token"))]
    let application = &ready.application;
    assert_eq!(application.id.get(), 789);
    assert_eq!(
        ready.resume_gateway_url,
        "wss://gateway-us-east1-b.discord.gg"
    );
}

#[cfg(feature = "user_token")]
#[tokio::test]
async fn user_ready_connects_without_inventing_an_application_id() {
    let http = Http::new("test.user.token");
    let (mut shard, mut socket) = gateway_pair(http.token()).await;
    let application_id = Arc::new(AtomicU64::new(0));
    let recorded_id = Arc::clone(&application_id);
    shard.set_application_id_callback(move |id| recorded_id.store(id.get(), Ordering::SeqCst));
    let payload = json!({ "op": 0, "s": 1, "t": "READY", "d": user_ready_payload() });
    socket
        .send(Message::Text(payload.to_string()))
        .await
        .unwrap();
    let event = shard.client.recv_json().await.unwrap().unwrap();
    assert!(matches!(&event, GatewayEvent::Dispatch(1, Event::Ready(_))));
    shard.handle_event(&Ok(event)).unwrap();
    assert_eq!(shard.stage(), ConnectionStage::Connected);
    assert_eq!(application_id.load(Ordering::SeqCst), 0);
}

#[test]
fn voice_dispatches_retain_the_fields_songbird_needs() {
    let state: GatewayEvent = from_value(json!({
        "op": 0, "s": 2, "t": "VOICE_STATE_UPDATE", "d": {
            "user_id": "123", "guild_id": "456", "channel_id": "789",
            "session_id": "voice-session", "deaf": false, "mute": false,
            "self_deaf": false, "self_mute": false, "self_video": false, "suppress": false
        }
    }))
    .unwrap();
    let GatewayEvent::Dispatch(_, Event::VoiceStateUpdate(state)) = state else {
        panic!("Expected a voice state dispatch");
    };
    assert_eq!(state.voice_state.user_id.get(), 123);
    assert_eq!(state.voice_state.guild_id.unwrap().get(), 456);
    assert_eq!(state.voice_state.session_id, "voice-session");

    let server: GatewayEvent = from_value(json!({
        "op": 0, "s": 3, "t": "VOICE_SERVER_UPDATE", "d": {
            "guild_id": "456", "token": "test-voice-token", "endpoint": "voice.example.invalid:443"
        }
    }))
    .unwrap();
    let GatewayEvent::Dispatch(_, Event::VoiceServerUpdate(server)) = server else {
        panic!("Expected a voice server dispatch");
    };
    assert_eq!(server.guild_id.unwrap().get(), 456);
    assert_eq!(server.token, "test-voice-token");
    assert_eq!(
        server.endpoint.as_deref(),
        Some("voice.example.invalid:443")
    );
}
