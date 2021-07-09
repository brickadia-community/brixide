use std::env;
use std::process::Stdio;

use log::error;
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
    sync::mpsc,
    task::JoinHandle,
};

pub struct Server {
    pub child: Child,
    pub stdin_task: JoinHandle<()>,
}

impl Server {
    pub fn start(
        args: &Vec<String>,
        mut stdin_receiver: mpsc::UnboundedReceiver<String>,
    ) -> Result<Self, std::io::Error> {
        let mut data_location = env::current_dir()?;
        data_location.push("data");

        let mut child = Command::new("stdbuf")
            .env(
                "LD_LIBRARY_PATH",
                format!("{}:$LD_LIBRARY_PATH", launcher::LAUNCHER_PATH),
            )
            .arg("--output=L")
            .arg("--")
            .arg(format!(
                "{}/{}",
                launcher::LAUNCHER_PATH,
                "main-brickadia-launcher"
            ))
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

        let mut stdin = child.stdin.take().unwrap();
        let stdin_task = tokio::spawn(async move {
            while let Some(mut line) = stdin_receiver.recv().await {
                line.push('\n');

                // write to stdin, killing task if we fail to write
                match stdin.write_all(&line[..].as_bytes()).await {
                    Ok(_) => (),
                    Err(_) => break,
                }
            }
            error!("server stdin task died");
        });

        Ok(Server { child, stdin_task })
    }
}
