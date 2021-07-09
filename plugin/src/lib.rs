use logging::PluginLogger;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{io::{self, AsyncBufReadExt, BufReader}, sync::mpsc::{self, UnboundedReceiver}};

pub mod logging;
pub mod payloads;
pub mod player;
pub mod rpc;

#[derive(Serialize, Deserialize, Debug)]
pub struct Plugin {
    name: String,
    author: String,
    description: String,
    #[serde(default = "Plugin::default_target")]
    target: String
}

static PLUGIN_LOGGER: PluginLogger = PluginLogger;

impl Plugin {
    // static methods

    pub fn use_plugin_logger() -> Result<(), log::SetLoggerError> {
        log::set_logger(&PLUGIN_LOGGER)
            .map(|()| log::set_max_level(log::LevelFilter::Debug))
    }

    pub fn send(message: &rpc::Message) {
        println!("{}", serde_json::to_string(message).unwrap())
    }

    pub fn spawn_listener() -> UnboundedReceiver<rpc::Message> {
        let (sender, receiver) = mpsc::unbounded_channel::<rpc::Message>();

        tokio::spawn(async move {
            let reader = BufReader::new(io::stdin());
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await.unwrap() {
                let rpc_message: rpc::Message = match serde_json::from_str(line.as_str()) {
                    Ok(m) => m,
                    Err(_) => continue
                };

                sender.send(rpc_message).unwrap();
            }
        });

        receiver
    }

    fn default_target() -> String {
        "plugin".into()
    }

    // abstraction stuff

    pub fn broadcast(content: &str) {
        Self::send(&rpc::Message::notification("broadcast", Some(json!(content))));
    }

    pub fn writeln(line: &str) {
        Self::send(&rpc::Message::notification("writeln", Some(json!(line))));
    }

    // instance methods/constructors

    pub fn name(&self) -> &str {
        &self.name[..]
    }

    pub fn author(&self) -> &str {
        &self.author[..]
    }

    pub fn description(&self) -> &str {
        &self.description[..]
    }

    pub fn target(&self) -> &str {
        &self.target[..]
    }
}
