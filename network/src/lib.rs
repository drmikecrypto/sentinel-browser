/*
 * Sentinel Network Layer (Vortex) - AGPL-3.0 License
 * Copyright (C) 2026 Sentinel DAO
 */

use anyhow::{Context, Result};
use async_trait::async_trait;
use arti_client::TorClient;
use tor_rtcompat::PreferredRuntime;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_native_tls::{TlsConnector, native_tls};

mod dns;
mod adblock;
mod socks;
mod pt;
mod v2ray;

pub use adblock::{AdBlocker, FilterAction, FilterRule};
pub use socks::SocksProxy;
pub use pt::{
    build_tor_client_config, obfs4_available, persist_bridge_line, pt_status_summary,
    snowflake_available,
};
pub use v2ray::{v2ray_ready, V2RayHandler};
use dns::SecureDns;
pub use dns::DnsProvider;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TorBridge {
    None,
    Obfs4 { addr: String, cert: String, iat_mode: u8 },
    Snowflake { broker: String, relay: String },
    Meek { url: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum V2RayConfig {
    VMess { uuid: String, alter_id: u32, security: String },
    VLESS { uuid: String, encryption: String, flow: String },
    Trojan { password: String },
    Shadowsocks { method: String, password: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Protocol {
    Clearweb,
    Tor { bridge: TorBridge },
    I2P,
    V2Ray(V2RayConfig),
    WireGuard { private_key: String, endpoint: String },
}

#[derive(Debug, Clone)]
pub struct NetworkStatusSnapshot {
    pub summary: String,
    pub protocol: String,
    pub socks_port: Option<u16>,
    pub tor_ready: bool,
    pub active_connections: usize,
    pub pt: String,
}

#[async_trait]
pub trait NetworkProxy {
    async fn connect(&self, target: &str) -> Result<()>;
    fn protocol(&self) -> Protocol;
}

use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    pub active_protocol: Protocol,
    pub custom_bridges: Vec<TorBridge>,
    pub v2ray_configs: Vec<V2RayConfig>,
    pub secure_dns_provider: dns::DnsProvider,
    pub secure_dns_provider_custom: String,
    pub profiles: Vec<ConnectionProfile>,
    #[serde(default = "default_socks_port")]
    pub socks_port: u16,
}

fn default_socks_port() -> u16 {
    9050
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            active_protocol: Protocol::Tor { bridge: TorBridge::None },
            custom_bridges: Vec::new(),
            v2ray_configs: Vec::new(),
            // Never Google — Quad9 DoH by default (no SafeSearch / parental filter profile).
            secure_dns_provider: dns::DnsProvider::Quad9,
            secure_dns_provider_custom: String::new(),
            profiles: Vec::new(),
            socks_port: 9050,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub name: String,
    pub protocol: Protocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsManager {
    path: PathBuf,
}

impl SettingsManager {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub async fn load(&self) -> Result<NetworkSettings> {
        if !self.path.exists() {
            return Ok(NetworkSettings::default());
        }
        let content = fs::read_to_string(&self.path).await?;
        let settings = serde_json::from_str(&content)?;
        Ok(settings)
    }

    pub async fn save(&self, settings: &NetworkSettings) -> Result<()> {
        let content = serde_json::to_string_pretty(settings)?;
        fs::write(&self.path, content).await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct Vortex {
    settings: NetworkSettings,
    settings_manager: SettingsManager,
    tor_client: Arc<Mutex<Option<TorClient<PreferredRuntime>>>>,
    dns_resolver: Arc<Mutex<Option<SecureDns>>>,
    v2ray_handler: Arc<Mutex<Option<V2RayHandler>>>,
    monitor: Arc<Mutex<CensorshipMonitor>>,
    ad_blocker: Arc<AdBlocker>,
    active_connections: Arc<std::sync::atomic::AtomicUsize>,
    socks_port: Arc<Mutex<Option<u16>>>,
}

impl std::fmt::Debug for Vortex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vortex")
            .field("settings", &self.settings)
            .field("settings_manager", &self.settings_manager)
            .field("tor_client", &"Option<TorClient>")
            .field("dns_resolver", &self.dns_resolver)
            .field("v2ray_handler", &self.v2ray_handler)
            .field("monitor", &self.monitor)
            .field("active_connections", &self.active_connections.load(std::sync::atomic::Ordering::Relaxed))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConnectivityStatus {
    Optimal,
    Degraded,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct CensorshipMonitor {
    pub status: ConnectivityStatus,
    pub last_check: Instant,
}

impl Default for CensorshipMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl CensorshipMonitor {
    pub fn new() -> Self {
        Self {
            status: ConnectivityStatus::Unknown,
            last_check: Instant::now(),
        }
    }

    pub async fn probe(&mut self, target: &str) -> ConnectivityStatus {
        info!("Probing connectivity to {}...", target);
        let start = Instant::now();
        match TcpStream::connect(target).await {
            Ok(_) => {
                let rtt = start.elapsed();
                if rtt.as_millis() > 1000 {
                    self.status = ConnectivityStatus::Degraded;
                } else {
                    self.status = ConnectivityStatus::Optimal;
                }
            }
            Err(_) => {
                self.status = ConnectivityStatus::Blocked;
            }
        }
        self.last_check = Instant::now();
        self.status.clone()
    }
}

impl Vortex {
    pub async fn new(config_path: PathBuf) -> Result<Self> {
        let settings_manager = SettingsManager::new(config_path);
        let settings = settings_manager.load().await?;

        Ok(Self {
            settings,
            settings_manager,
            tor_client: Arc::new(Mutex::new(None)),
            dns_resolver: Arc::new(Mutex::new(None)),
            v2ray_handler: Arc::new(Mutex::new(None)),
            monitor: Arc::new(Mutex::new(CensorshipMonitor::new())),
            ad_blocker: Arc::new(AdBlocker::with_builtin_lists()),
            active_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            socks_port: Arc::new(Mutex::new(None)),
        })
    }

    pub fn ad_blocker(&self) -> Arc<AdBlocker> {
        self.ad_blocker.clone()
    }

    pub async fn socks_port(&self) -> Option<u16> {
        *self.socks_port.lock().await
    }

    pub fn get_active_connections(&self) -> usize {
        self.active_connections.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub async fn update_config(&mut self, protocol: Protocol) -> Result<()> {
        info!("Updating network configuration to: {:?}", protocol);
        self.settings.active_protocol = protocol;
        self.settings_manager.save(&self.settings).await?;
        self.initialize().await
    }

    pub async fn add_custom_bridge(&mut self, bridge: TorBridge) -> Result<()> {
        info!("Adding custom Tor bridge: {:?}", bridge);
        self.settings.custom_bridges.push(bridge);
        self.settings_manager.save(&self.settings).await
    }

    pub async fn add_v2ray_config(&mut self, config: V2RayConfig) -> Result<()> {
        info!("Adding custom V2Ray configuration: {:?}", config);
        self.settings.v2ray_configs.push(config);
        self.settings_manager.save(&self.settings).await
    }

    pub async fn get_tor_client(&self) -> Option<TorClient<PreferredRuntime>> {
        self.tor_client.lock().await.clone()
    }

    pub async fn add_profile(&mut self, name: String, protocol: Protocol) -> Result<()> {
        let profile = ConnectionProfile { name, protocol };
        self.settings.profiles.push(profile);
        self.settings_manager.save(&self.settings).await
    }

    pub async fn switch_profile(&mut self, name: &str) -> Result<()> {
        if let Some(p) = self.settings.profiles.iter().find(|p| p.name == name) {
            self.update_config(p.protocol.clone()).await
        } else {
            anyhow::bail!("Profile not found")
        }
    }

    pub fn list_profiles(&self) -> Vec<ConnectionProfile> {
        self.settings.profiles.clone()
    }

    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing Vortex Network Layer...");
        
        // 1. Initialize Protocol-specific components first
        let mut client_opt = None;
        match &self.settings.active_protocol {
            Protocol::Tor { bridge } => {
                info!("Bootstrapping Tor with bridge: {:?}", bridge);
                let config_dir = self
                    .settings_manager
                    .path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("config"));

                // Persist requested bridge line(s) for arti pt-client
                match bridge {
                    TorBridge::Obfs4 { addr, cert, iat_mode } => {
                        let bridge_line = format!(
                            "Bridge obfs4 {} cert={} iat-mode={}",
                            addr, cert, iat_mode
                        );
                        let _ = persist_bridge_line(&config_dir, &bridge_line);
                        if !obfs4_available() {
                            warn!(
                                "obfs4 bridge saved but obfs4proxy not on PATH — Tor will try without PT"
                            );
                        }
                    }
                    TorBridge::Snowflake { broker, relay } => {
                        let bridge_line = format!(
                            "Bridge snowflake 192.0.2.3:1 2B280B23E1107BB62ABFC40DDCC8824814F80A72 url={} ice=stun:stun.l.google.com:19302 utls-imitate=hellorandomizedalpn",
                            broker
                        );
                        let _ = relay;
                        let _ = persist_bridge_line(&config_dir, &bridge_line);
                        if !snowflake_available() {
                            warn!(
                                "Snowflake requested but snowflake-client not on PATH — direct bootstrap if PT attach fails"
                            );
                        }
                    }
                    TorBridge::Meek { url } => {
                        let _ = persist_bridge_line(
                            &config_dir,
                            &format!("Bridge meek_lite 0.0.2.0:1 url={}", url),
                        );
                        if !obfs4_available() {
                            warn!("Meek requires obfs4proxy (meek_lite) on PATH");
                        }
                    }
                    TorBridge::None => {
                        info!("No bridge configured. Using bridges.txt if present, else direct.");
                    }
                }

                let config = build_tor_client_config(&config_dir);
                let bootstrap_future = TorClient::create_bootstrapped(config);
                match tokio::time::timeout(Duration::from_secs(60), bootstrap_future).await {
                    Ok(Ok(client)) => {
                        info!("Tor client bootstrapped successfully.");
                        client_opt = Some(client.clone());
                        {
                            let mut tc = self.tor_client.lock().await;
                            *tc = Some(client);
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("Failed to bootstrap Tor client: {}. Some features may be unavailable.", e);
                    }
                    Err(_) => {
                        warn!("Tor bootstrapping timed out after 60s. Continuing in degraded mode.");
                    }
                }
            }
            Protocol::V2Ray(config) => {
                if !v2ray_ready() {
                    warn!("V2Ray selected but not configured — refusing theater start");
                    anyhow::bail!("V2Ray requires V2RAY_PATH + SENTINEL_V2RAY_HOST/PORT");
                }
                let handler = V2RayHandler::new(config.clone());
                let port = handler.start().await?;
                {
                    let mut sp = self.socks_port.lock().await;
                    *sp = Some(port);
                }
                let mut vh = self.v2ray_handler.lock().await;
                *vh = Some(handler);
            }
            Protocol::Clearweb => {
                // Still bootstrap Tor in background so .onion and emergency switch work.
                info!("Clearweb mode — bootstrapping Tor in background for .onion / fallback");
                let config_dir = self
                    .settings_manager
                    .path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("config"));
                let config = build_tor_client_config(&config_dir);
                let bootstrap_future = TorClient::create_bootstrapped(config);
                match tokio::time::timeout(Duration::from_secs(60), bootstrap_future).await {
                    Ok(Ok(client)) => {
                        client_opt = Some(client.clone());
                        let mut tc = self.tor_client.lock().await;
                        *tc = Some(client);
                    }
                    Ok(Err(e)) => warn!("Background Tor bootstrap failed: {}", e),
                    Err(_) => warn!("Background Tor bootstrap timed out"),
                }
            }
            Protocol::I2P | Protocol::WireGuard { .. } => {
                warn!(
                    "{:?} is not implemented for WebView traffic — use Tor or Clearweb",
                    self.settings.active_protocol
                );
                anyhow::bail!("Protocol not available in this build");
            }
        }

        // Start SOCKS5 front-door for WebView2 / system proxy consumers
        if self.tor_client.lock().await.is_some() {
            match SocksProxy::start(self.tor_client.clone(), self.settings.socks_port).await {
                Ok(proxy) => {
                    let mut sp = self.socks_port.lock().await;
                    *sp = Some(proxy.port);
                    info!("SOCKS ready on port {}", proxy.port);
                }
                Err(e) => warn!("Failed to start SOCKS proxy: {}", e),
            }
        }

        // 2. Initialize DNS with the (optional) Tor client — never Google SafeSearch DNS
        let dns_provider = match &self.settings.active_protocol {
            Protocol::Tor { .. } => dns::DnsProvider::Tor,
            _ => {
                let p = self.settings.secure_dns_provider;
                if matches!(p, dns::DnsProvider::Google) {
                    warn!("Google DNS rejected — using Quad9 (no SafeSearch / parental filter)");
                    dns::DnsProvider::Quad9
                } else {
                    p
                }
            }
        };

        let dns = SecureDns::new(dns_provider, client_opt).await?;
        {
            let mut d = self.dns_resolver.lock().await;
            *d = Some(dns);
        }
        
        Ok(())
    }

    pub async fn check_and_auto_switch(&mut self) -> Result<()> {
        let status = {
            let mut monitor = self.monitor.lock().await;
            monitor.probe("1.1.1.1:443").await // Probe a reliable target
        };
        
                if status == ConnectivityStatus::Blocked {
            info!("Current network blocked! Attempting automatic circumvention switch...");
            
            let next_protocol = match &self.settings.active_protocol {
                Protocol::Clearweb => Protocol::Tor { bridge: TorBridge::None },
                Protocol::Tor { bridge: TorBridge::None } => {
                    if crate::snowflake_available() {
                        Protocol::Tor {
                            bridge: TorBridge::Snowflake {
                                broker: "https://snowflake-broker.torproject.net/".to_string(),
                                relay: "snowflake.torproject.net".to_string(),
                            },
                        }
                    } else {
                        warn!("Snowflake PT not on PATH — staying on Tor direct / degraded");
                        Protocol::Tor { bridge: TorBridge::None }
                    }
                }
                other => {
                    warn!("No further auto-switch from {:?}", other);
                    other.clone()
                }
            };
            
            if next_protocol != self.settings.active_protocol {
                self.update_config(next_protocol).await?;
            }
        }
        Ok(())
    }

    pub async fn get_status(&self) -> String {
        let tor_up = self.tor_client.lock().await.is_some();
        let socks = *self.socks_port.lock().await;
        let proto = format!("{:?}", self.settings.active_protocol);
        match (tor_up, socks) {
            (true, Some(p)) => format!("Tor ready · SOCKS 127.0.0.1:{} · {}", p, proto),
            (true, None) => format!("Tor client up · SOCKS not listening · {}", proto),
            (false, Some(p)) => format!("SOCKS on :{} without Tor client · {}", p, proto),
            (false, None) => format!("Tor not bootstrapped · {}", proto),
        }
    }

    /// Snapshot for status UI (no invented bandwidth).
    pub async fn status_snapshot(&self) -> NetworkStatusSnapshot {
        NetworkStatusSnapshot {
            summary: self.get_status().await,
            protocol: format!("{:?}", self.settings.active_protocol),
            socks_port: *self.socks_port.lock().await,
            tor_ready: self.tor_client.lock().await.is_some(),
            active_connections: self.get_active_connections(),
            pt: pt_status_summary(),
        }
    }

    async fn connect_stream(&self, url: &str) -> Result<Box<dyn AsyncReadWrite + Unpin + Send>> {
        self.active_connections.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let result = self.connect_stream_internal(url).await;
        if result.is_err() {
            self.active_connections.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
        result
    }

    async fn connect_stream_internal(&self, url: &str) -> Result<Box<dyn AsyncReadWrite + Unpin + Send>> {
        let url_parsed = url::Url::parse(url)?;
        let host = url_parsed.host_str().context("No host in URL")?;
        let scheme = url_parsed.scheme();
        let port = url_parsed.port_or_known_default().unwrap_or(match scheme {
            "https" => 443,
            "http" => 80,
            _ => 443,
        });

        info!("Connecting to {} via Vortex...", host);
        
        let target_addr = format!("{}:{}", host, port);
        let use_tls = scheme == "https";

        let stream: Box<dyn AsyncReadWrite + Unpin + Send> = match &self.settings.active_protocol {
            Protocol::Tor { .. } => {
                let tc = self.tor_client.lock().await;
                if let Some(client) = tc.as_ref() {
                    let s = client.connect(target_addr).await?;
                    Box::new(s)
                } else {
                    return Err(anyhow::anyhow!("Tor client not initialized"));
                }
            }
            Protocol::Clearweb => {
                let ips = {
                    let dns_guard = self.dns_resolver.lock().await;
                    if let Some(dns) = dns_guard.as_ref() {
                        dns.resolve(host).await?
                    } else {
                        return Err(anyhow::anyhow!("DNS resolver not initialized"));
                    }
                };

                let mut last_error = None;
                let mut connected_stream = None;

                for ip in ips {
                    let addr = format!("{}:{}", ip, port);
                    match TcpStream::connect(&addr).await {
                        Ok(s) => {
                            connected_stream = Some(s);
                            break;
                        }
                        Err(e) => {
                            info!("Failed to connect to {}: {}", addr, e);
                            last_error = Some(e);
                        }
                    }
                }

                if let Some(s) = connected_stream {
                    Box::new(s)
                } else {
                    return Err(anyhow::anyhow!("Connection failed for {}: {:?}", host, last_error));
                }
            }
            _ => return Err(anyhow::anyhow!("Protocol not supported")),
        };

        if use_tls {
            let connector = TlsConnector::from(native_tls::TlsConnector::builder().build()?);
            let tls_stream = connector.connect(host, stream).await
                .map_err(|e| anyhow::anyhow!("TLS handshake failed: {}", e))?;
            Ok(Box::new(tls_stream))
        } else {
            Ok(stream)
        }
    }

    pub async fn fetch(&self, url: &str) -> Result<String> {
        info!("Fetching URL via Vortex: {}", url);
        self.active_connections.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let result = self.fetch_internal(url).await;
        self.active_connections.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        result
    }

    async fn fetch_internal(&self, url: &str) -> Result<String> {
        if self.ad_blocker.is_blocked(url) {
            anyhow::bail!("Blocked by Sentinel Shields: {}", url);
        }
        
        // Prefer internal Tor client for .onion or Tor mode
        let is_tor_mode = matches!(self.settings.active_protocol, Protocol::Tor { .. });
        let is_onion = url.contains(".onion");
        
        if is_tor_mode || is_onion {
            let tc_ready = {
                let tc = self.tor_client.lock().await;
                tc.is_some()
            };
            if tc_ready {
                // Use low-level stream through Tor client
                let url_parsed = url::Url::parse(url)?;
                let host = url_parsed.host_str().context("No host in URL")?;
                let path = url_parsed.path();
                let query = url_parsed.query().map(|q| format!("?{}", q)).unwrap_or_default();
                let req_path = if path.is_empty() { "/" } else { path };
                let target = format!("{}{}{}", req_path, query, "");
                
                let mut stream = self.connect_stream(url).await?;
                let request = format!(
                    "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Sentinel/0.1\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8\r\n\r\n",
                    target, host
                );
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                stream.write_all(request.as_bytes()).await?;
                
                let mut response = Vec::new();
                stream.read_to_end(&mut response).await?;
                
                let body_start = response.windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .map(|i| i + 4)
                    .unwrap_or(0);
                let body = &response[body_start..];
                self.active_connections.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                return Ok(String::from_utf8_lossy(body).to_string());
            } else {
                info!("Tor client not initialized yet; falling back to Clearweb HTTP client for fetch.");
            }
        }
        
        // Clearweb or Tor not ready: use reqwest
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Sentinel/0.1")
            .build()?;
        let res = client.get(url).send().await?;
        let body = res.text().await?;
        Ok(body)
    }

    pub async fn download(&self, url: &str, destination: &std::path::Path) -> Result<()> {
        info!("Downloading URL: {} to {:?}", url, destination);
        self.active_connections.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let result = self.download_internal(url, destination).await;
        self.active_connections.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        result
    }

    async fn download_internal(&self, url: &str, destination: &std::path::Path) -> Result<()> {
        let url_parsed = url::Url::parse(url)?;
        let host = url_parsed.host_str().context("No host in URL")?;
        let path = url_parsed.path();
        
        let download_task = async {
            let mut stream = self.connect_stream(url).await?;

            // Send HTTP GET
            let request = format!("GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\nUser-Agent: Sentinel/0.1\r\n\r\n", 
                                  if path.is_empty() { "/" } else { path }, host);
            
            stream.write_all(request.as_bytes()).await?;
            
            let mut response = Vec::new();
            stream.read_to_end(&mut response).await?;
            
            // Find body start
            let body_start = response.windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|i| i + 4)
                .unwrap_or(0);
                
            let body = &response[body_start..];
            tokio::fs::write(destination, body).await?;
            Ok(())
        };

        tokio::time::timeout(std::time::Duration::from_secs(30), download_task)
            .await
            .context("Download operation timed out (30s)")?
    }
}

// Helper trait to combine AsyncRead and AsyncWrite
pub trait AsyncReadWrite: tokio::io::AsyncRead + tokio::io::AsyncWrite {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite> AsyncReadWrite for T {}

#[async_trait]
impl NetworkProxy for Vortex {
   fn protocol(&self) -> Protocol {
        self.settings.active_protocol.clone()
    }

    async fn connect(&self, target: &str) -> Result<()> {
        let protocol = self.protocol();
        info!("Initiating connection to {} via {:?}", target, protocol);

        match protocol {
            Protocol::Tor { .. } => {
                let client_guard = self.tor_client.lock().await;
                if let Some(client) = client_guard.as_ref() {
                    info!("Routing through Tor circuit...");
                    let _stream = client.connect(target).await
                        .context("Failed to connect via Tor")?;
                    info!("Tor connection established to {}", target);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Tor client not initialized"))
                }
            }
            Protocol::Clearweb => {
                info!("Routing through Clearweb...");
                
                // Parse host and port
                let parts: Vec<&str> = target.split(':').collect();
                if parts.len() != 2 {
                    return Err(anyhow::anyhow!("Invalid target format. Expected host:port"));
                }
                let host = parts[0];
                let port = parts[1];

                // Resolve via DoH
                let ips = {
                    let dns_guard = self.dns_resolver.lock().await;
                    if let Some(dns) = dns_guard.as_ref() {
                        dns.resolve(host).await?
                    } else {
                        return Err(anyhow::anyhow!("DNS resolver not initialized"));
                    }
                };

                if let Some(ip) = ips.first() {
                    let addr = format!("{}:{}", ip, port);
                    info!("Connecting to resolved IP: {}", addr);
                    let _stream = TcpStream::connect(&addr).await
                        .context("Failed to connect via Clearweb")?;
                    info!("Clearweb connection established to {}", target);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Could not resolve hostname: {}", host))
                }
            }
            _ => Err(anyhow::anyhow!("Protocol {:?} not yet implemented", protocol)),
        }
    }
}

#[cfg(test)]
mod tests;
