use std::env;
use std::process::Stdio;

use tokio::process::{Command, Child};

pub struct Server {
    pub child: Child
}

impl Server {
    #[cfg(not(target_os = "windows"))]
    pub fn start(args: &Vec<String>) -> Result<Self, std::io::Error> {
        let mut data_location = env::current_dir()?;
        data_location.push("data");

        let child = Command::new("stdbuf")
            .env("LD_LIBRARY_PATH", format!("{}:$LD_LIBRARY_PATH", launcher::LAUNCHER_PATH))
            .arg("--output=L")
            .arg("--")
            .arg(format!("{}/{}", launcher::LAUNCHER_PATH, "main-brickadia-launcher"))
            .arg("--server")
            .arg("--")
            .arg("-NotInstalled")
            .arg("-log")
            .arg(format!("-UserDir={}", data_location.to_str().unwrap()))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        Ok(Server { child })
    }

    #[cfg(target_os = "windows")]
    pub fn start(args: &Vec<String>) -> Result<Self, std::io::Error> {
        use std::os::windows::process::CommandExt;

        let mut data_location = env::current_dir()?;
        data_location.push("data");

        let child = Command::new(format!("{}{}", launcher::INSTALL_LOCATION, "/BrickadiaLauncher/BrickadiaLauncher.exe"))
            .arg("--server")
            .arg("--")
            .arg("-NotInstalled")
            .arg("-log")
            .arg(format!("-UserDir={}", data_location.to_str().unwrap()))
            .args(args)
            //.stdout(Stdio::piped())
            //.stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        
        Ok(Server { child })
    }
}
