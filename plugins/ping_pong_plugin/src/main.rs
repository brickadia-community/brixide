use std::{convert::TryInto, time::Duration};

use log::{debug, error, info, warn};
use plugin::{Plugin, payloads::ChatPayload, player::Player, rpc};
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    Plugin::use_plugin_logger().unwrap();

    info!("Test log from console");

    let mut receiver = Plugin::spawn_listener();

    while let Some(message) = receiver.recv().await {
        match message.method() {
            Some("join") => {
                // a player joins
                let player: Player = match message {
                    rpc::Message::Notification { params, .. } => serde_json::from_value(params.unwrap()).unwrap(),
                    _ => continue
                };

                info!("A player has joined the game (name: {}, id: {})", player.name, player.uuid);
            },
            Some("chat") => {
                // a user chats
                let payload: ChatPayload = message.try_into().unwrap();

                match payload.message.as_str() {
                    "ping" => {
                        Plugin::broadcast("Pong!");
                    },
                    "pong" => {
                        Plugin::broadcast("Ping!");
                    },
                    _ => ()
                }
            },
            _ => ()
        }
    }
}
