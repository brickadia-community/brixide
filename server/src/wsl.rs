// thanks cake (reference to Meshiest/omegga-wsl2binds)

use std::{net::IpAddr, path::Path, process::Stdio};

use anyhow::{bail, Result};
use tokio::process::{Child, Command};

pub struct UdpProxy {
    pub child: Child,
}

impl UdpProxy {
    pub async fn spawn(ip: IpAddr, port: i32) -> Result<Self> {
        let child = Command::new("tools/udpprox.exe")
            .arg("-b")
            .arg("0.0.0.0")
            .arg("-r")
            .arg(port.to_string())
            .arg("-l")
            .arg(port.to_string())
            .arg("--host")
            .arg(ip.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        let proxy = UdpProxy { child };

        Ok(proxy)
    }
}

pub fn is_wsl() -> bool {
    Path::new("/run/WSL").exists()
}

pub async fn ip() -> Result<IpAddr> {
    let out = Command::new("tools/ip.sh").output().await?;

    if !out.status.success() {
        bail!("failed to grab WSL IP");
    }

    let str = String::from_utf8(out.stdout)?;

    Ok(str.trim().parse()?)
}
