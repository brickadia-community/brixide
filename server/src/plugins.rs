use std::{convert::TryInto, path::PathBuf, process::Stdio, sync::Arc};

use anyhow::{Result, anyhow, bail};
use log::{debug, error, info, trace, warn};
use plugin::{Plugin, logging::LogSeverity, payloads, rpc};
use serde::Deserialize;
use serde_json::Value;
use tokio::{fs::{self, File}, io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt}, process::{Child, ChildStdin, Command}, sync::Mutex};

pub struct ServerStdinContainer {
    pub stdin: Option<ChildStdin>
}

impl ServerStdinContainer {
    pub fn new() -> Self {
        ServerStdinContainer { stdin: None }
    }

    pub fn set(&mut self, stdin: ChildStdin) {
        self.stdin = Some(stdin);
    }
}

/// Represents the configuration of the plugin.
#[derive(Deserialize)]
pub struct PluginConfig {
    plugin: Plugin,
    #[serde(skip)]
    path: Option<PathBuf>
}

impl PluginConfig {
    pub fn plugin(&self) -> &Plugin {
        &self.plugin
    }

    pub fn path(&self) -> &Option<PathBuf> {
        &self.path
    }
}

/// Represents an instance of the plugin running.
pub struct PluginInstance {
    pub config: Arc<PluginConfig>,
    pub process: Arc<Mutex<Child>>,
    pub stdin: Arc<Mutex<ChildStdin>>
}

impl PluginInstance {
    async fn send(stdin: Arc<Mutex<ServerStdinContainer>>, content: &mut String) -> Result<()> {
        content.push('\n');
        let mut locked = stdin.lock().await;
        let stdin = &mut locked.stdin.as_mut().ok_or(anyhow!("no server stdin is available"))?;
        stdin.write_all(&content[..].as_bytes()).await?;
        Ok(())
    }

    pub fn start(config: PluginConfig, stdin_arc: Arc<Mutex<ServerStdinContainer>>) -> Result<PluginInstance> {
        if config.path.is_none() {
            bail!("no plugin path found");
        }

        // the path should be the target path
        let mut path = config.path().to_owned().unwrap();
        path.push(config.plugin.target());

        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut child_stdin = child.stdin.take().unwrap();
        let mut child_stdout = child.stdout.take().unwrap(); // this will be moved into the thread handling the plugin

        let config_arc = Arc::new(config);
        let child_mtx = Arc::new(Mutex::new(child));
        
        let config_thread_arc = config_arc.clone();
        let child_thread_mtx = child_mtx.clone();
        tokio::spawn(async move {
            let reader = io::BufReader::new(child_stdout);
            let mut lines = reader.lines();

            // truth be told, if this thread panics, it doesn't really matter because the plugin died in some regard
            // todo: handle this a little bit better
            while let Some(line) = lines.next_line().await.unwrap() {
                let rpc_message: rpc::Message = match serde_json::from_str(&line[..]) {
                    Ok(m) => m,
                    Err(_) => continue
                };
                
                // handle rpc messages sent by the plugin
                match rpc_message.method() {
                    Some("log") => {
                        // log messages
                        let payload: payloads::LogPayload = rpc_message.try_into().unwrap();
                        match payload.severity {
                            LogSeverity::Debug => debug!("[{}] {}", config_thread_arc.plugin.name(), payload.content),
                            LogSeverity::Info => info!("[{}] {}", config_thread_arc.plugin.name(), payload.content),
                            LogSeverity::Warn => warn!("[{}] {}", config_thread_arc.plugin.name(), payload.content),
                            LogSeverity::Error => error!("[{}] {}", config_thread_arc.plugin.name(), payload.content),
                            LogSeverity::Trace => trace!("[{}] {}", config_thread_arc.plugin.name(), payload.content)
                        }
                    },
                    Some("broadcast") => {
                        // broadcast text
                        if let rpc::Message::Notification { params, .. } = rpc_message {
                            match params {
                                Some(Value::String(str)) => {
                                    Self::send(stdin_arc.clone(), &mut format!("Chat.Broadcast {}", str)).await.unwrap();
                                },
                                _ => ()
                            }
                        }
                    },
                    _ => ()
                }
            }
        });

        Ok(PluginInstance { config: config_arc, process: child_mtx, stdin: Arc::new(Mutex::new(child_stdin)) })
    }
}

/// Scan the plugins folder for plugins, and generate a list of them
pub async fn scan() -> Vec<PluginConfig> {
    let mut plugins = vec![];

    let paths = fs::read_dir("plugins").await;
    if let Err(_) = paths {
        warn!("Plugins folder doesn't exist, couldn't find any plugins");
        return vec![];
    }

    let mut paths = paths.unwrap();
    while let Some(child) = paths.next_entry().await.unwrap() {
        let path = child.path();
        let metadata_path = path.join("plugin.toml");

        if !metadata_path.exists() || !metadata_path.is_file() {
            // the plugin.toml either doesn't exist or isn't a file
            continue;
        }

        let file = File::open(&metadata_path).await;
        if let Err(_) = file {
            warn!("Failed to read plugin metadata at {} (opening)", metadata_path.to_str().unwrap());
            continue;
        }
        
        let mut file = file.unwrap();
        let mut contents = String::new();
        match file.read_to_string(&mut contents).await {
            Err(_) => {
                warn!("Failed to read plugin metadata at {} (reading)", metadata_path.to_str().unwrap());
                continue;
            },
            _ => ()
        }

        let mut plugin: PluginConfig = match toml::from_str(&contents[..]) {
            Ok(p) => p,
            Err(_) => {
                warn!("Bad plugin metadata at {}", metadata_path.to_str().unwrap());
                continue;
            }
        };

        plugin.path = Some(path);
        plugins.push(plugin);
    }

    plugins
}
