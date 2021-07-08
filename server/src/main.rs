use std::{fs, io::Read, path::Path, process::exit, sync::Arc, time::Duration};

use clap::{App, Arg, SubCommand};
use dialoguer::{Input, Password, theme::ColorfulTheme};
use fern::{Dispatch, colors::{Color, ColoredLevelConfig}};
use log::{debug, error, info, warn};
use plugin::{payloads::*, rpc};
use regex::Regex;
use tokio::{io::{self, AsyncBufReadExt, AsyncWriteExt}, process::ChildStdin, sync::Mutex};

use crate::{plugins::{PluginInstance, ServerStdinContainer}, server::Server};

mod server;
mod wsl;
mod plugins;

#[tokio::main]
async fn main() {
    // configure the logger
    let colors = ColoredLevelConfig::new()
        .debug(Color::BrightBlue)
        .info(Color::Green)
        .warn(Color::Yellow)
        .error(Color::Red);

    Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!("{} {}",
                format!(
                    "\x1B[{}m>>\x1B[0m",
                    colors.get_color(&record.level()).to_fg_str()
                ),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()
        .expect("Failed to apply logger");

    let matches = App::new("brust (working title)")
        .version("0.1.0")
        .author("voximity")
        .about("Server wrapper for Brickadia")
        .arg(Arg::with_name("no-install")
            .long("no-install")
            .help("Exit if the launcher is not installed"))
        .arg(Arg::with_name("port")
            .long("port")
            .short("p")
            .help("Change the port the server will run on")
            .default_value("7777"))
        .subcommand(SubCommand::with_name("install")
            .about("Forcefully install the Brickadia launcher"))
        .subcommand(SubCommand::with_name("uninstall")
            .about("Forcefully uninstall the launcher and server data, if applicable")
            .arg(Arg::with_name("i-understand")
                .long("i-understand")
                .help("You understand the consequences by running this command: your server and all its data will be lost")))
        .get_matches();
    
    // install subcommand
    if let Some(matches) = matches.subcommand_matches("install") {
        launcher::install(matches).await;
        exit(0);
    }

    // uninstall subcommand
    if let Some(matches) = matches.subcommand_matches("uninstall") {
        if matches.is_present("i-understand") {
            if let Err(_) = fs::remove_dir_all("data") {
                error!("An error occurred uninstalling the server (are enough permissions granted?)");
                exit(1)
            } else {
                info!("Server has been uninstalled successfully");
            }
        } else {
            warn!("Your server and all its data will be lost if you proceed.");
            warn!("To confirm, run the command again with --i-understand");
        }
        exit(0);
    }

    // run the server
    let port: i32 = matches.value_of("port").unwrap().parse().expect("Invalid port number");

    // check if the launcher is installed. if it's not, let's install it first
    if !launcher::is_installed(&matches) {
        if matches.is_present("no-install") {
            warn!("The launcher is not installed, exiting");
            exit(0);
        }

        #[cfg(not(target_os = "windows"))]
        warn!("The launcher is not installed, it will be downloaded now");

        launcher::install(&matches).await;
    }

    // if we're on windows create the data folder if it doesn't exist
    if !Path::new("data").exists() {
        fs::create_dir("data").expect("Unable to create data directory");
    }

    // get one-time account credentials if we don't have auth already
    let mut credentials: (Option<String>, Option<String>) = (None, None);
    if !Path::new("data/Saved/Auth").exists() {
        info!("Please enter your account information to host the server (this is a one-time process)");

        let email: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("Account email")
            .interact()
            .unwrap();
        
        let password: String = Password::with_theme(&ColorfulTheme::default())
            .with_prompt("Account password")
            .interact()
            .unwrap();
        
        credentials.0 = Some(email);
        credentials.1 = Some(password);

        warn!("If the server asks for your credentials next time, your credentials were inputted incorrectly");
    }

    let mut server_stdin_arc = Arc::new(Mutex::new(ServerStdinContainer::new()));

    info!("Scanning for plugins...");
    let plugins = plugins::scan().await;
    let instances = Arc::new(Mutex::new(vec![]));
    for plugin_config in plugins {
        let instance = match PluginInstance::start(plugin_config, server_stdin_arc.clone()) {
            Ok(i) => i,
            Err(x) => {
                warn!("Plugin failed to start: {:?}", x);
                continue;
            }
        };
        instances.lock().await.push(instance);
    }

    async fn broadcast_rpc(instances: Arc<Mutex<Vec<PluginInstance>>>, message: rpc::Message) {
        let mut line = serde_json::to_string(&message).unwrap();
        line.push('\n');

        for instance in instances.lock().await.iter() {
            instance.stdin.lock().await.write_all(&line[..].as_bytes()).await.unwrap();
        }
    }

    // check if we're rocking WSL, and if we are, start the udp proxy
    let mut udp_proxy: Option<wsl::UdpProxy> = None;

    if wsl::is_wsl() {
        let ip = wsl::ip().await.expect("Failed to get WSL IP");
        info!("Detected WSL, starting UDP proxy on {}", ip);
        udp_proxy = Some(wsl::UdpProxy::spawn(ip, port).await.unwrap());
    }

    // at this point the launcher should be installed, so we can create a server instance and start reading from it
    let mut launch_args = vec![];
    if let Some(email) = credentials.0 { launch_args.push(format!("-User={}", email)); }
    if let Some(password) = credentials.1 { launch_args.push(format!("-Password={}", password)); }

    let mut server = Server::start(&launch_args).unwrap();
    
    info!("Server active");

    let stdout = server.child.stdout.take().unwrap();
    let stdin = server.child.stdin.take().unwrap();
    let reader = io::BufReader::new(stdout);
    let mut lines = reader.lines();

    server_stdin_arc.lock().await.set(stdin);

    let chat_regex = Regex::new("LogChat: (?P<user>[^:]+): (?P<message>.*)$").unwrap();

    while let Some(line) = lines.next_line().await.unwrap() {
        if let Some(capture) = chat_regex.captures(line.as_str()) {
            let user = &capture["user"];
            let message = &capture["message"];

            info!("{}: {}", user, message);

            let rpc_message: rpc::Message = ChatPayload { user: user.into(), message: message.into() }.into();
            broadcast_rpc(instances.clone(), rpc_message).await;
        }
    }
}
