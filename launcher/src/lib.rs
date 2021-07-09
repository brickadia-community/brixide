use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::exit;

use log::{error, info, warn};
use reqwest::header;
use tokio::process::Command;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36"; // otherwise cloudflare throws a 1020 :(
const LAUNCHER_URL: &str = "https://static.brickadia.com/launcher/1.4/brickadia-launcher.tar.xz";
const LAUNCHER_FILE: &str = "launcher.tar.xz";
pub const DATA_PATH: &str = "./data";
pub const LAUNCHER_PATH: &str = "./data/brickadia-launcher";

#[cfg(target_os = "windows")]
pub const INSTALL_LOCATION: &str = "C:/Program Files/Brickadia";

pub fn is_installed<'a>(_matches: &clap::ArgMatches<'a>) -> bool {
    #[cfg(target_os = "windows")]
    return Path::new(
        _matches
            .value_of("install-location")
            .unwrap_or(INSTALL_LOCATION),
    )
    .exists();

    #[cfg(not(target_os = "windows"))]
    return Path::new(LAUNCHER_PATH).exists();
}

#[cfg(target_os = "windows")]
pub async fn install<'a>(_matches: &clap::ArgMatches<'a>) {
    // for windows installations, we can't programatically install the launcher, but we
    // can expect the user to already have it installed
    let install_location = _matches
        .value_of("install-location")
        .unwrap_or(INSTALL_LOCATION);
    if !Path::new(install_location).exists() {
        error!("Brickadia is not installed! Please install it from https://brickadia.com/download");
        exit(1);
    }
}

#[cfg(not(target_os = "windows"))]
pub async fn install<'a>(_matches: &clap::ArgMatches<'a>) {
    info!("Downloading launcher");

    // at this point, we assume we can use the .tar.xz archive from the website
    let client = reqwest::Client::new();
    let response = client
        .get(LAUNCHER_URL)
        .header(header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .expect("Failed to download the launcher!");

    if !response.status().is_success() {
        error!("Failed to download the launcher!");
        exit(1);
    }

    let mut file = File::create("launcher.tar.xz").expect("Failed to create launcher file");
    let bytes = response.bytes().await.unwrap();
    file.write_all(&bytes[..])
        .expect("Failed to write to launcher file");

    info!("Downloaded launcher, extracting");

    if Path::new(LAUNCHER_PATH).exists() {
        // remove existing launcher dir
        fs::remove_dir_all(LAUNCHER_PATH).unwrap();
    } else if !Path::new(DATA_PATH).exists() {
        fs::create_dir_all(DATA_PATH).unwrap();
    }

    let extract = Command::new("tar")
        .arg("xJf")
        .arg(LAUNCHER_FILE)
        .arg("-C")
        .arg(DATA_PATH)
        .output()
        .await;

    let extract_out = match extract {
        Ok(x) => x,
        Err(_) => {
            error!("Failed to run extract command (is tar installed?)");
            exit(1);
        }
    };

    if !extract_out.status.success() {
        error!("Failed to extract launcher");
        exit(1);
    }

    // clean up launcher archive
    match fs::remove_file(LAUNCHER_FILE) {
        Ok(_) => (),
        Err(_) => {
            warn!("Failed to clean up launcher file")
        }
    }

    info!("Launcher installed successfully!");
}
