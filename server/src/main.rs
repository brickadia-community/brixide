use std::{collections::HashMap, fs, path::Path, process::exit};

use clap::{App, Arg, SubCommand};
use dialoguer::{Input, Password, theme::ColorfulTheme};
use fern::{Dispatch, colors::{Color, ColoredLevelConfig}};
use log::{debug, error, info, warn};
use plugin::{payloads::*, rpc};
use regex::Regex;
use tokio::{io::{self, AsyncBufReadExt}, sync::mpsc};

use crate::{matchers::*, plugins::PluginInstance, server::Server};

mod matchers;
mod plugins;
mod server;
mod wsl;


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
        .arg(Arg::with_name("server-verbose")
            .long("server-verbose")
            .help("Display all logs from the Brickadia server"))
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

    // prepare the stdin channel
    let (stdin_sender, stdin_receiver) = mpsc::unbounded_channel::<String>();

    let plugins = plugins::scan().await;
    let mut instances = vec![];
    for plugin_config in plugins {
        let instance = match PluginInstance::start(plugin_config, stdin_sender.clone()) {
            Ok(i) => i,
            Err(x) => {
                warn!("Plugin failed to start: {:?}", x);
                continue;
            }
        };
        instances.push(instance);
    }

    info!("Started {} plugins", instances.len());

    // check if we're rocking WSL, and if we are, start the udp proxy
    let mut _udp_proxy: Option<wsl::UdpProxy> = None;

    if wsl::is_wsl() {
        let ip = wsl::ip().await.expect("Failed to get WSL IP");
        info!("Detected WSL, starting UDP proxy on {}", ip);
        _udp_proxy = Some(wsl::UdpProxy::spawn(ip, port).await.unwrap());
    }

    // at this point the launcher should be installed, so we can create a server instance and start reading from it
    let mut launch_args = vec![];
    if let Some(email) = credentials.0 { launch_args.push(format!("-User={}", email)); }
    if let Some(password) = credentials.1 { launch_args.push(format!("-Password={}", password)); }

    let is_server_verbose = matches.is_present("server-verbose");
    let mut server = Server::start(&launch_args, stdin_receiver).unwrap();
    
    info!("Server active");

    let stdout = server.child.stdout.take().unwrap();
    let reader = io::BufReader::new(stdout);
    let mut lines = reader.lines();

    let log_matcher = Regex::new("^\\[[\\d\\.\\-:]+\\]\\[\\s*(?P<index>\\d+)\\](?P<body>.+)$").unwrap();
    
    let grouped_regex_matchers: Vec<Box<dyn GroupedRegexMatcher>> = vec![Box::new(ChatRegexMatcher), Box::new(JoinRegexMatcher)];
    let mut grouped_regex_instances: Vec<GroupedRegexMatches<'_>> = vec![];

    // repeatedly listen to stdout for new content
    while let Some(line) = lines.next_line().await.unwrap() {
        if is_server_verbose {
            debug!(":: {}", line);
        }

        let log_match = match log_matcher.captures(line.as_str()) {
            Some(x) => x,
            None => continue
        };

        let index: i32 = (&log_match["index"]).parse().unwrap();
        let body = &log_match["body"];

        // handle each grouped regex instance, and break if one is matched
        let mut i: usize = 0;
        for instance in grouped_regex_instances.iter_mut() {
            if index == instance.index {
                let regexes = instance.matcher.regexes();
                let next_regex = &regexes[instance.captures.len()];

                let capture_names = next_regex.capture_names();
                if let Some(captures) = next_regex.captures(body) {
                    // clone out captures into a map for ownership
                    let mut map = HashMap::new();

                    for group_name in capture_names {
                        let group_name = match group_name {
                            Some(x) => x,
                            None => continue
                        };
                        let m = captures.name(group_name).unwrap().as_str().clone();
                        map.insert(String::from(group_name), String::from(m));
                    }
                    
                    // we have our map, update the instance
                    instance.captures.push(map);

                    // if our captures count is >= the regex count, our job here is done:
                    // submit the rpc message
                    if instance.captures.len() >= regexes.len() {
                        let rpc_message = instance.matcher.convert(&instance).await;

                        for instance in instances.iter() {
                            instance.stdin.send(serde_json::to_string(&rpc_message).unwrap()).unwrap();
                        }

                        break;
                    }
                }
            }

            i += 1;
        }

        // loop terminated early somewhere, so we remove it at its index
        if i < grouped_regex_instances.len() {
            grouped_regex_instances.remove(i);
            continue;
        }

        // handle each grouped regex matcher, trying to start new instances if possible
        for matcher in grouped_regex_matchers.iter() {
            let matcher_regexes = matcher.regexes();
            let first_regex = &matcher_regexes[0];

            let capture_names = first_regex.capture_names();
            if let Some(captures) = first_regex.captures(body) {
                // effectively clone out captures into a map for ownership
                let mut map = HashMap::new();

                for group_name in capture_names {
                    let group_name = match group_name {
                        Some(x) => x,
                        None => continue
                    };
                    let m = captures.name(group_name).unwrap().as_str().clone();
                    map.insert(String::from(group_name), String::from(m));
                }

                // we match with the first regex, so let's start making a new instance
                let instance = GroupedRegexMatches { index, matcher, captures: vec![map] };

                // if the grouped regex actually only has one regex, we can early-terminate and avoid adding it to the array
                if matcher_regexes.len() == 1 {
                    let rpc_message = matcher.convert(&instance).await;

                    for instance in instances.iter() {
                        instance.stdin.send(serde_json::to_string(&rpc_message).unwrap()).unwrap();
                    }

                    break;
                } else {
                    // add it to the instances array
                    grouped_regex_instances.push(instance);
                }
            }
        }
    }
}
