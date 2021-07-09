use std::{convert::TryInto, path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use anyhow::{bail, Result};

use log::{debug, error, info, trace, warn};
use plugin::{logging::LogSeverity, payloads, rpc, Plugin};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use tokio::{
    fs::{self, File},
    io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    sync::{
        mpsc::{self, UnboundedSender},
        Mutex,
    },
    time::Instant,
};

use crate::matchers::{GroupedRegexMatches, PluginRegexMatcher, RegexCaptures};

/// Represents the configuration of the plugin.
#[derive(Deserialize)]
pub struct PluginConfig {
    plugin: Plugin,
    #[serde(skip)]
    path: Option<PathBuf>,
}

impl PluginConfig {
    pub fn plugin(&self) -> &Plugin {
        &self.plugin
    }

    pub fn path(&self) -> &Option<PathBuf> {
        &self.path
    }
}

/// A group of channels each plugin should have access to.
#[derive(Clone)]
pub struct PluginChannels<'a> {
    pub stdin: mpsc::UnboundedSender<String>,
    pub matchers: mpsc::UnboundedSender<GroupedRegexMatches<'a>>,
}

/// Represents an instance of the plugin running.
pub struct PluginInstance {
    pub config: Arc<PluginConfig>,
    pub process: Arc<Mutex<Child>>,
    pub stdin: mpsc::UnboundedSender<String>,
}

impl PluginInstance {
    pub fn start(config: PluginConfig, channels: &PluginChannels<'_>) -> Result<PluginInstance> {
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

        let mut child_stdin = child.stdin.take().unwrap(); // this will be moved into the task that listens for stdin
        let child_stdout = child.stdout.take().unwrap(); // this will be moved into the task handling the plugin

        // sending to stdin task
        let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            while let Some(mut x) = receiver.recv().await {
                x.push('\n');
                match child_stdin.write_all(&x[..].as_bytes()).await {
                    Ok(_) => (),
                    Err(_) => break,
                }
            }
        });

        let config_arc = Arc::new(config);
        let child_mtx = Arc::new(Mutex::new(child));

        // reading stdout task
        let config_thread_arc = config_arc.clone();
        let _child_thread_mtx = child_mtx.clone(); // is this necessary?

        let game_stdin = channels.stdin.clone();
        let _regex_matchers = channels.matchers.clone();
        tokio::spawn(async move {
            let reader = io::BufReader::new(child_stdout);
            let mut lines = reader.lines();

            async fn match_regex(
                matchers_channel: UnboundedSender<GroupedRegexMatches<'_>>,
                regexes: Vec<Regex>,
                timeout: Duration,
            ) -> Option<RegexCaptures> {
                let (sender, mut receiver) = mpsc::channel(1);
                let matcher = PluginRegexMatcher {
                    regexes,
                    capture_sender: sender,
                };
                let matcher_arc = Arc::new(matcher);
                let instance = GroupedRegexMatches {
                    matcher: matcher_arc.clone(),
                    index: None,
                    captures: RegexCaptures::default(),
                    last: Instant::now(),
                    timeout,
                };
                matchers_channel.send(instance).unwrap();

                receiver.recv().await
            }

            // truth be told, if this thread panics, it doesn't really matter because the plugin died in some regard
            // todo: handle this a little bit better
            while let Some(line) = lines.next_line().await.unwrap() {
                let rpc_message: rpc::Message = match serde_json::from_str(&line[..]) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                // handle rpc messages sent by the plugin
                match rpc_message.method() {
                    Some("log") => {
                        // log messages
                        let payload: payloads::LogPayload = rpc_message.try_into().unwrap();
                        match payload.severity {
                            LogSeverity::Debug => {
                                debug!("[{}] {}", config_thread_arc.plugin.name(), payload.content)
                            }
                            LogSeverity::Info => {
                                info!("[{}] {}", config_thread_arc.plugin.name(), payload.content)
                            }
                            LogSeverity::Warn => {
                                warn!("[{}] {}", config_thread_arc.plugin.name(), payload.content)
                            }
                            LogSeverity::Error => {
                                error!("[{}] {}", config_thread_arc.plugin.name(), payload.content)
                            }
                            LogSeverity::Trace => {
                                trace!("[{}] {}", config_thread_arc.plugin.name(), payload.content)
                            }
                        }
                    }
                    Some("broadcast") => {
                        // broadcast text
                        if let rpc::Message::Notification { params, .. } = rpc_message {
                            match params {
                                Some(Value::String(str)) => {
                                    game_stdin.send(format!("Chat.Broadcast {}", str)).unwrap();
                                }
                                _ => (),
                            }
                        }
                    }
                    Some("writeln") => {
                        // write a line directly to the server stdin
                        if let rpc::Message::Notification { params, .. } = rpc_message {
                            match params {
                                Some(Value::String(str)) => game_stdin.send(str).unwrap(),
                                _ => (),
                            }
                        }
                    }
                    _ => (),
                }
            }
        });

        Ok(PluginInstance {
            config: config_arc,
            process: child_mtx,
            stdin: sender,
        })
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
            warn!(
                "Failed to read plugin metadata at {} (opening)",
                metadata_path.to_str().unwrap()
            );
            continue;
        }

        let mut file = file.unwrap();
        let mut contents = String::new();
        match file.read_to_string(&mut contents).await {
            Err(_) => {
                warn!(
                    "Failed to read plugin metadata at {} (reading)",
                    metadata_path.to_str().unwrap()
                );
                continue;
            }
            _ => (),
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
