//! V2Ray handler — only runs when V2RAY_PATH exists and a real outbound is configured.

use anyhow::{bail, Context, Result};
use std::sync::Arc;
use tokio::process::Child;
use tokio::sync::Mutex;
use tracing::info;

use crate::V2RayConfig;

#[derive(Debug, Clone)]
pub struct V2RayHandler {
    config: V2RayConfig,
    process: Arc<Mutex<Option<Child>>>,
    pub socks_port: u16,
}

impl V2RayHandler {
    pub fn new(config: V2RayConfig) -> Self {
        Self {
            config,
            process: Arc::new(Mutex::new(None)),
            socks_port: 10808,
        }
    }

    /// Returns Ok(socks_port) when a real process with SOCKS inbound is running.
    pub async fn start(&self) -> Result<u16> {
        let v2ray_path = std::env::var("V2RAY_PATH").unwrap_or_default();
        if v2ray_path.is_empty() || !std::path::Path::new(&v2ray_path).exists() {
            bail!(
                "V2Ray disabled: set V2RAY_PATH to a real v2ray binary (not exposed in UI until then)"
            );
        }

        let host = std::env::var("SENTINEL_V2RAY_HOST").unwrap_or_default();
        let port: u16 = std::env::var("SENTINEL_V2RAY_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        if host.is_empty() || host == "127.0.0.1" || port == 0 || port == 10086 {
            bail!(
                "V2Ray disabled: set SENTINEL_V2RAY_HOST and SENTINEL_V2RAY_PORT to a real outbound (placeholder 127.0.0.1:10086 rejected)"
            );
        }

        let socks_port = self.socks_port;
        let outbound = match &self.config {
            V2RayConfig::VMess {
                uuid,
                alter_id,
                security,
            } => serde_json::json!({
                "protocol": "vmess",
                "settings": {
                    "vnext": [{
                        "address": host,
                        "port": port,
                        "users": [{ "id": uuid, "alterId": alter_id, "security": security }]
                    }]
                }
            }),
            V2RayConfig::VLESS {
                uuid,
                encryption,
                flow,
            } => serde_json::json!({
                "protocol": "vless",
                "settings": {
                    "vnext": [{
                        "address": host,
                        "port": port,
                        "users": [{ "id": uuid, "encryption": encryption, "flow": flow }]
                    }]
                }
            }),
            V2RayConfig::Trojan { password } => serde_json::json!({
                "protocol": "trojan",
                "settings": {
                    "servers": [{ "address": host, "port": port, "password": password }]
                }
            }),
            V2RayConfig::Shadowsocks { method, password } => serde_json::json!({
                "protocol": "shadowsocks",
                "settings": {
                    "servers": [{ "address": host, "port": port, "method": method, "password": password }]
                }
            }),
        };

        let config_json = serde_json::json!({
            "inbounds": [{
                "tag": "socks-in",
                "port": socks_port,
                "listen": "127.0.0.1",
                "protocol": "socks",
                "settings": { "udp": true }
            }],
            "outbounds": [outbound]
        });

        if std::env::var("SENTINEL_TEST_MODE").is_ok() {
            info!("Skipping V2Ray process spawn in test mode.");
            return Ok(socks_port);
        }

        let mut command = tokio::process::Command::new(&v2ray_path);
        command
            .arg("run")
            .arg("-config")
            .arg("stdin:")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());

        let mut child = command.spawn().context("Failed to spawn v2ray process")?;
        let mut stdin = child.stdin.take().context("v2ray stdin")?;
        use tokio::io::AsyncWriteExt;
        let config_str = serde_json::to_string(&config_json)?;
        stdin.write_all(config_str.as_bytes()).await?;
        drop(stdin);

        let mut process_guard = self.process.lock().await;
        *process_guard = Some(child);

        info!(
            "V2Ray SOCKS inbound 127.0.0.1:{} → {}:{}",
            socks_port, host, port
        );
        Ok(socks_port)
    }
}

/// True when env is configured for a real V2Ray session.
pub fn v2ray_ready() -> bool {
    let path = std::env::var("V2RAY_PATH").unwrap_or_default();
    if path.is_empty() || !std::path::Path::new(&path).exists() {
        return false;
    }
    let host = std::env::var("SENTINEL_V2RAY_HOST").unwrap_or_default();
    let port: u16 = std::env::var("SENTINEL_V2RAY_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    !host.is_empty() && host != "127.0.0.1" && port != 0 && port != 10086
}
