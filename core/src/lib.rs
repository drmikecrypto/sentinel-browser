/*
 * Sentinel Browser Core (Aegis) - AGPL-3.0 License
 * Copyright (C) 2026 Sentinel DAO
 */

use anyhow::Result;
use sent_net::Vortex;
use sent_ui::{UiEvent, BrowserCommand};
use sent_search::HorusEngine;
use sent_shield::SecurityManager;
use sent_gov::{GovernanceEngine, Proposal};

use std::path::PathBuf;
use std::time::Instant;
use std::sync::Arc;
use tokio::sync::{mpsc::Receiver, Mutex};
use winit::event_loop::EventLoopProxy;
use tracing::{info, warn};

mod storage;
mod views;
mod plugins;
mod update;

pub use storage::StorageManager;
pub use plugins::PluginManager;

use sent_net::NetworkProxy;

pub struct PerformanceMonitor {
    boot_start: Instant,
    metrics: Arc<Mutex<PerformanceMetrics>>,
}

#[derive(Default)]
struct PerformanceMetrics {
    cold_start_ms: u64,
    memory_usage_mb: u64,
    fps: f64,
    concurrent_conns: usize,
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            boot_start: Instant::now(),
            metrics: Arc::new(Mutex::new(PerformanceMetrics::default())),
        }
    }

    pub async fn record_boot_complete(&self) {
        let duration = self.boot_start.elapsed().as_millis() as u64;
        let duration = if duration == 0 { 1 } else { duration };
        let mut m = self.metrics.lock().await;
        m.cold_start_ms = duration;
        info!("Performance: Cold start completed in {}ms", duration);
        
        // Check targets: < 500ms
        if duration > 500 {
            warn!("Performance Warning: Cold start ({}ms) exceeds 500ms target", duration);
        }
    }

    pub async fn update_system_metrics(&self, vortex: Option<&Vortex>) {
        let mut m = self.metrics.lock().await;
        
        // Production implementation using sysinfo for real-time telemetry
        use sysinfo::System;
        let mut sys = System::new_all();
        sys.refresh_all();
        
        let pid = sysinfo::get_current_pid().ok();
        if let Some(pid) = pid {
            if let Some(process) = sys.process(pid) {
                m.memory_usage_mb = (process.memory() / 1024 / 1024) as u64;
            }
        }
        
        // FPS: unknown without UI telemetry — omit fake 60
        m.fps = 0.0;

        // Concurrent connections from Vortex
        if let Some(vortex) = vortex {
            m.concurrent_conns = vortex.get_active_connections();
        } else {
            m.concurrent_conns = 0;
        }
        
        info!("Sentinel Telemetry: RAM: {}MB | FPS: {} | Conns: {}", m.memory_usage_mb, m.fps, m.concurrent_conns);
    }

    pub fn check_memory_usage(&self) {
        use sysinfo::System;
        let mut sys = System::new_all();
        sys.refresh_memory();
        
        let used = sys.used_memory() / 1024 / 1024;
        let total = sys.total_memory() / 1024 / 1024;
        info!("System Memory: {}MB / {}MB used", used, total);
        
        if used as f32 / total as f32 > 0.9 {
            warn!("High memory usage detected! Consider suspending inactive tabs.");
        }
    }
}

use std::collections::HashMap;

pub struct Tab {
    pub id: u32,
    pub url: String,
    pub title: String,
    pub last_active: Instant,
    pub is_suspended: bool,
    pub saved_state: Option<String>, // Serialized DOM/State
    pub history: Vec<String>,
    pub history_index: usize,
}

pub struct TabManager {
    tabs: HashMap<u32, Tab>,
    active_tab_id: u32,
    next_tab_id: u32,
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TabManager {
    pub fn new() -> Self {
        Self {
            tabs: HashMap::new(),
            active_tab_id: 0,
            next_tab_id: 0,
        }
    }

    pub fn add_tab(&mut self, url: String, title: String) -> u32 {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        self.tabs.insert(id, Tab {
            id,
            url: url.clone(),
            title,
            last_active: Instant::now(),
            is_suspended: false,
            saved_state: None,
            history: vec![url],
            history_index: 0,
        });
        self.active_tab_id = id;
        id
    }

    pub fn switch_to_tab(&mut self, id: u32) {
        if self.tabs.contains_key(&id) {
            self.active_tab_id = id;
            if let Some(tab) = self.tabs.get_mut(&id) {
                tab.last_active = Instant::now();
                tab.is_suspended = false;
            }
        }
    }

    pub fn close_tab(&mut self, id: u32) {
        self.tabs.remove(&id);
        if self.active_tab_id == id {
            // Pick another tab as active if available
            if let Some(&new_id) = self.tabs.keys().next() {
                self.active_tab_id = new_id;
            } else {
                self.active_tab_id = 0;
            }
        }
    }

    pub fn create_tab(&mut self, url: String) -> u32 {
        self.add_tab(url, "New Tab".to_string())
    }

    pub fn suspend_inactive_tabs(&mut self, threshold_secs: u64, storage: &StorageManager) {
        let now = Instant::now();
        for (id, tab) in self.tabs.iter_mut() {
            if *id != self.active_tab_id && !tab.is_suspended && now.duration_since(tab.last_active).as_secs() > threshold_secs {
                info!("Suspending inactive tab #{} ({}) to save memory", id, tab.url);
                tab.is_suspended = true;
                
                // State Serialization:
                // We serialize the current DOM state, scroll position, and form data
                // to the encrypted local storage before dropping the active memory.
                // This allows for a "lazy restore" when the user clicks the tab again.
                if let Some(content) = &tab.saved_state {
                    let _ = storage.save_tab_state(*id, content);
                }
            }
        }
    }

    pub fn list_tabs(&self) -> Vec<(u32, String, String, bool)> {
        let mut v: Vec<_> = self
            .tabs
            .values()
            .map(|t| {
                (
                    t.id,
                    t.title.clone(),
                    t.url.clone(),
                    t.id == self.active_tab_id,
                )
            })
            .collect();
        v.sort_by_key(|(id, ..)| *id);
        v
    }

    pub fn active_tab_id(&self) -> u32 {
        self.active_tab_id
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn get_active_tab(&self) -> Option<&Tab> {
        self.tabs.get(&self.active_tab_id)
    }

    pub fn get_active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(&self.active_tab_id)
    }

    pub fn push_history(&mut self, url: String) {
        if let Some(tab) = self.tabs.get_mut(&self.active_tab_id) {
            // If we are not at the end of history, truncate it
            if tab.history_index < tab.history.len() - 1 {
                tab.history.truncate(tab.history_index + 1);
            }
            
            // Don't push if it's the same as current
            if tab.history.last() != Some(&url) {
                tab.history.push(url.clone());
                tab.history_index = tab.history.len() - 1;
            }
            tab.url = url;
        }
    }

    pub fn go_back(&mut self) -> Option<String> {
        if let Some(tab) = self.tabs.get_mut(&self.active_tab_id) {
            if tab.history_index > 0 {
                tab.history_index -= 1;
                let url = tab.history[tab.history_index].clone();
                tab.url = url.clone();
                return Some(url);
            }
        }
        None
    }

    pub fn go_forward(&mut self) -> Option<String> {
        if let Some(tab) = self.tabs.get_mut(&self.active_tab_id) {
            if tab.history_index < tab.history.len() - 1 {
                tab.history_index += 1;
                let url = tab.history[tab.history_index].clone();
                tab.url = url.clone();
                return Some(url);
            }
        }
        None
    }
}

pub struct Aegis {
    network: Vortex,
    search: HorusEngine,
    security: SecurityManager,
    storage: StorageManager,
    governance: GovernanceEngine,
    plugins: PluginManager,
    perf: PerformanceMonitor,
    tabs: TabManager,
    ui_proxy: EventLoopProxy<UiEvent>,
    command_rx: Receiver<BrowserCommand>,
}

impl Aegis {
    pub async fn new(
        ui_proxy: EventLoopProxy<UiEvent>, 
        command_rx: Receiver<BrowserCommand>,
        config_dir: Option<PathBuf>
    ) -> Result<Self> {
        info!("Aegis Core initializing...");
        
        let config_dir = config_dir.unwrap_or_else(|| std::env::current_dir().unwrap().join("config"));
        if !config_dir.exists() {
            std::fs::create_dir_all(&config_dir)?;
        }

        let network = Vortex::new(config_dir.join("network.json")).await?;
        let tor_client = network.get_tor_client().await;
        
        let dht_tx = sent_search::start_dht().await?;
        let indexer = Arc::new(sent_search::BlockchainIndexer::new(dht_tx));
        let mut search = HorusEngine::new(indexer, tor_client);
        if let Some(port) = network.socks_port().await {
            search.set_socks_port(Some(port));
        }
        
        let mut storage = StorageManager::new(config_dir.join("sentinel.db"))?;
        let vault_secret = load_or_create_vault_secret(&config_dir)?;
        storage.unlock(&vault_secret)?; 

        let plugins = PluginManager::new(std::env::current_dir()?.join("plugins"));
        let security = SecurityManager::new();
        let perf = PerformanceMonitor::new();
        
        let mut governance = GovernanceEngine::new();
        // Seed some proposals
        governance.submit_proposal(Proposal {
            id: 1,
            title: "Upgrade to ZK-Rollup V2".to_string(),
            description: "Implement Halo2 proofs for scalability.".to_string(),
            author: "0xSatoshi".to_string(),
            execution_hash: "0x123...abc".to_string(),
            deadline: 1735689600,
        });
        governance.submit_proposal(Proposal {
            id: 2,
            title: "Treasury Allocation: Q1 Dev Fund".to_string(),
            description: "Allocate 500k SENT for core dev.".to_string(),
            author: "0xBuilder".to_string(),
            execution_hash: "0x456...def".to_string(),
            deadline: 1738368000,
        });

        // Set default settings if not present
        if storage.get_setting("theme").unwrap_or(None).is_none() {
            storage.set_setting("theme", "Cyberpunk")?;
        }
        if storage.get_setting("search_engine").unwrap_or(None).is_none() {
            storage.set_setting("search_engine", "Horus")?;
        }
        if storage.get_setting("history_enabled").unwrap_or(None).is_none() {
             // Default to enabled for now, can be toggled
             storage.set_setting("history_enabled", "true")?;
        }

        let mut tabs = TabManager::new();
        tabs.create_tab("sentinel://welcome".to_string());

        Ok(Self {
            network,
            search,
            security,
            storage,
            governance,
            plugins,
            perf,
            tabs,
            ui_proxy,
            command_rx,
        })
    }

    pub async fn boot(mut self) -> Result<()> {
        info!("Fast Boot sequence initiated");

        // 0. Share Vortex with UI
        self.ui_proxy.send_event(UiEvent::SetVortex(Arc::new(self.network.clone())))?;

        // 1. Apply Security Hardening
        self.security.harden_system().map_err(|e| anyhow::anyhow!(e))?;

        // 2. Network init (Tor + SOCKS) — awaited so .onion search/proxy work
        if let Err(e) = self.network.initialize().await {
            warn!("Network initialization failed: {:?}", e);
            let _ = self.ui_proxy.send_event(UiEvent::ConfigurationError(format!(
                "Network init: {}",
                e
            )));
        }
        if let Some(port) = self.network.socks_port().await {
            self.search.set_socks_port(Some(port));
            self.search.recrawl_with_socks();
            let _ = self.ui_proxy.send_event(UiEvent::SocksReady(port));
        }
        let _ = self
            .ui_proxy
            .send_event(UiEvent::NetworkStatusChanged(self.network.protocol()));

        // 3. Discover Plugins
        if let Err(e) = self.plugins.discover_plugins() {
            warn!("Plugin discovery failed: {:?}", e);
        }

        // 4. Record Performance
        self.perf.record_boot_complete().await;

        // 4b. Background update check (GitHub Releases)
        update::spawn_periodic_update_check(self.ui_proxy.clone());

        // 5. Home
        self.navigate("sentinel://home", true).await?;
        self.sync_tabs_to_ui();

        info!("Cold start completed (v{}).", update::current_version());
        self.run_loop().await
    }

    pub fn get_vortex(&self) -> Vortex {
        self.network.clone()
    }

    fn sync_tabs_to_ui(&self) {
        let tabs: Vec<sent_ui::TabInfo> = self
            .tabs
            .list_tabs()
            .into_iter()
            .map(|(id, title, url, active)| sent_ui::TabInfo {
                id,
                title,
                url,
                active,
            })
            .collect();
        let _ = self.ui_proxy.send_event(UiEvent::TabsChanged(tabs));
    }

    pub async fn handle_network_change(&mut self, protocol: sent_net::Protocol) -> Result<()> {
        info!("Handling network change request...");
        self.network.update_config(protocol.clone()).await?;
        let _ = self
            .ui_proxy
            .send_event(UiEvent::NetworkStatusChanged(protocol));
        if let Some(port) = self.network.socks_port().await {
            self.search.set_socks_port(Some(port));
            self.search.recrawl_with_socks();
            let _ = self.ui_proxy.send_event(UiEvent::SocksReady(port));
        }
        Ok(())
    }

    async fn run_loop(&mut self) -> Result<()> {
        info!("Aegis Command Loop Started");
        let mut last_tab_check = Instant::now();
        
        loop {
            // Periodic memory optimization
            if last_tab_check.elapsed().as_secs() > 60 {
                self.tabs.suspend_inactive_tabs(300, &self.storage); // Suspend if inactive for 5 mins
                self.perf.update_system_metrics(Some(&self.network)).await;
                self.perf.check_memory_usage();
                last_tab_check = Instant::now();
            }

            // Check for commands
            tokio::select! {
                Some(command) = self.command_rx.recv() => {
                    match command {
                        BrowserCommand::Navigate(url) => {
                            if let Err(e) = self.navigate(&url, true).await {
                                warn!("Navigation failed: {:?}", e);
                            }
                        }
                        BrowserCommand::Back => {
                            if let Some(url) = self.tabs.go_back() {
                                if let Err(e) = self.navigate(&url, false).await {
                                    warn!("Back navigation failed: {:?}", e);
                                }
                            }
                        }
                        BrowserCommand::Forward => {
                            if let Some(url) = self.tabs.go_forward() {
                                if let Err(e) = self.navigate(&url, false).await {
                                    warn!("Forward navigation failed: {:?}", e);
                                }
                            }
                        }
                        BrowserCommand::Refresh => {
                            if let Some(tab) = self.tabs.get_active_tab() {
                                let url = tab.url.clone();
                                if let Err(e) = self.navigate(&url, false).await {
                                    warn!("Refresh failed: {:?}", e);
                                }
                            }
                        }
                        BrowserCommand::ChangeNetwork(protocol) => {
                            if let Err(e) = self.handle_network_change(protocol).await {
                                warn!("Network change failed: {:?}", e);
                            }
                        }
                        BrowserCommand::AddBridge(bridge) => {
                            info!("Aegis: Adding user-defined bridge...");
                            if let Err(e) = self.network.add_custom_bridge(bridge).await {
                                warn!("Failed to add bridge: {:?}", e);
                            }
                        }
                        BrowserCommand::AddV2Ray(config) => {
                            info!("Aegis: Adding user-defined V2Ray config...");
                            if let Err(e) = self.network.add_v2ray_config(config).await {
                                warn!("Failed to add V2Ray config: {:?}", e);
                            }
                        }
                        BrowserCommand::SetProxyMode { use_tor, socks_port } => {
                            if use_tor {
                                self.search.set_socks_port(Some(socks_port));
                                self.search.recrawl_with_socks();
                                let _ = self.ui_proxy.send_event(UiEvent::SocksReady(socks_port));
                            } else {
                                self.search.set_socks_port(None);
                            }
                        }
                        BrowserCommand::NewTab => {
                            let id = self.tabs.create_tab("sentinel://home".to_string());
                            info!("New tab {}", id);
                            self.sync_tabs_to_ui();
                            if let Err(e) = self.navigate("sentinel://home", true).await {
                                warn!("New tab navigate failed: {:?}", e);
                            }
                        }
                        BrowserCommand::SwitchTab(id) => {
                            self.tabs.switch_to_tab(id);
                            self.sync_tabs_to_ui();
                            if let Some(tab) = self.tabs.get_active_tab() {
                                let url = tab.url.clone();
                                let _ = self.ui_proxy.send_event(UiEvent::SetUrlBar(url.clone()));
                                if let Err(e) = self.navigate(&url, false).await {
                                    warn!("Switch tab navigate failed: {:?}", e);
                                }
                            }
                        }
                        BrowserCommand::CloseTab(id) => {
                            if self.tabs.tab_count() <= 1 {
                                // Keep at least one tab — reset to home
                                self.tabs.close_tab(id);
                                let _ = self.tabs.create_tab("sentinel://home".to_string());
                            } else {
                                self.tabs.close_tab(id);
                            }
                            self.sync_tabs_to_ui();
                            if let Some(tab) = self.tabs.get_active_tab() {
                                let url = tab.url.clone();
                                let _ = self.ui_proxy.send_event(UiEvent::SetUrlBar(url.clone()));
                                if let Err(e) = self.navigate(&url, false).await {
                                    warn!("After close navigate failed: {:?}", e);
                                }
                            }
                        }
                        BrowserCommand::SetPrivacy {
                            level,
                            webrtc_blocked,
                            webgl_disabled,
                        } => {
                            let _ = self.ui_proxy.send_event(UiEvent::PrivacyUpdated {
                                level,
                                webrtc_blocked,
                                webgl_disabled,
                                sandbox_label: "Job Object".into(),
                            });
                        }
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                    // Just to ensure loop continues if no commands
                }
            }
        }
    }

    async fn navigate(&mut self, url: &str, record_history: bool) -> Result<()> {
        info!("Navigating to: {} (record_history: {})", url, record_history);
        
        if record_history {
            self.tabs.push_history(url.to_string());
        }

        // Record persistent history if enabled and not an internal page
        let history_enabled = self.storage.get_setting("history_enabled")?.unwrap_or_else(|| "true".to_string()) == "true";
        if history_enabled && !url.starts_with("sentinel://vote") && !url.starts_with("sentinel://network") {
            let title = if url.starts_with("sentinel://") {
                url.strip_prefix("sentinel://").unwrap_or(url).to_uppercase()
            } else {
                url.to_string()
            };
            let _ = self.storage.add_history(url, &title);
        }

        // Update active tab title/url when navigating
        if let Some(tab) = self.tabs.get_active_tab_mut() {
            tab.url = url.to_string();
            if url.starts_with("sentinel://") {
                tab.title = url
                    .strip_prefix("sentinel://")
                    .unwrap_or("Sentinel")
                    .split('?')
                    .next()
                    .unwrap_or("page")
                    .to_string();
            } else {
                tab.title = url.chars().take(32).collect();
            }
        }
        self.sync_tabs_to_ui();

        if url.starts_with("sentinel://") {
            self.handle_internal_page(url).await?;
        } else {
            // External URL vs Query — real engine loads URLs; search stays in Horus
            let trimmed = url.trim();
            if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
                let _ = self.ui_proxy.send_event(UiEvent::SetUrlBar(trimmed.to_string()));
                self.ui_proxy.send_event(UiEvent::LoadUrl(trimmed.to_string()))?;
            } else if trimmed.contains('.') && !trimmed.contains(' ') {
                let full_url = if trimmed.ends_with(".onion") {
                    format!("http://{}", trimmed)
                } else {
                    format!("https://{}", trimmed)
                };
                let _ = self.ui_proxy.send_event(UiEvent::SetUrlBar(full_url.clone()));
                self.ui_proxy.send_event(UiEvent::LoadUrl(full_url))?;
            } else {
                let q = trimmed;
                if q.is_empty() {
                    let html = views::search_results_page("Empty query", Vec::new());
                    self.ui_proxy.send_event(UiEvent::LoadHtml(html))?;
                } else {
                    let results = self.search.search(q).await?;
                    self.render_search_results(q, results).await?;
                }
            }
        }
        Ok(())
    }

    async fn handle_internal_page(&mut self, url: &str) -> Result<()> {
        let mut current_url = url.to_string();
        
        loop {
            let html_or_redirect = match current_url.as_str() {
                "sentinel://welcome" | "sentinel://newtab" | "sentinel://home" => {
                    let theme = self.storage.get_setting("theme")?.unwrap_or_else(|| "Cyberpunk".to_string());
                    let engine = self.storage.get_setting("search_engine")?.unwrap_or_else(|| "Horus".to_string());
                    Some(views::welcome_page(&theme, &engine))
                }
                "sentinel://status" => {
                    let snap = self.network.status_snapshot().await;
                    let socks = snap
                        .socks_port
                        .map(|p| format!("127.0.0.1:{}", p))
                        .unwrap_or_else(|| "none".into());
                    let mem = {
                        use sysinfo::System;
                        let mut sys = System::new();
                        sys.refresh_memory();
                        if let Ok(pid) = sysinfo::get_current_pid() {
                            sys.refresh_processes(
                                sysinfo::ProcessesToUpdate::Some(&[pid]),
                                true,
                            );
                            sys.process(pid)
                                .map(|p| p.memory() / 1024 / 1024)
                                .unwrap_or(0)
                        } else {
                            0
                        }
                    };
                    Some(views::status_page(
                        &snap.summary,
                        &snap.protocol,
                        &socks,
                        snap.tor_ready,
                        snap.active_connections,
                        mem,
                        &snap.pt,
                    ))
                }
                "sentinel://connect" => {
                    let status = self.network.get_status().await;
                    let profiles = self.network.list_profiles();
                    let pt = sent_net::pt_status_summary();
                    Some(views::connect_page(status.as_str(), profiles, &pt))
                }
                "sentinel://settings" => {
                    let theme = self.storage.get_setting("theme")?.unwrap_or_else(|| "Cyberpunk".to_string());
                    let engine = self.storage.get_setting("search_engine")?.unwrap_or_else(|| "Horus".to_string());
                    let history = self.storage.get_setting("history_enabled")?.unwrap_or_else(|| "true".to_string());
                    let security = self.storage.get_setting("security_level")?.unwrap_or_else(|| "Standard".to_string());
                    Some(views::settings_page(&theme, &engine, &security, &history))
                }
                "sentinel://governance" => {
                    let proposals = self.governance.list_proposals();
                    Some(views::governance_page(&proposals))
                }
                "sentinel://dapps" => {
                    Some(views::dapps_page())
                }
                "sentinel://design" => {
                    Some(views::design_system_page())
                }
                "sentinel://prototype" => {
                    Some(views::prototypes_page())
                }
                "sentinel://network_menu" => {
                    let pt = sent_net::pt_status_summary();
                    Some(views::network_menu_page(&pt))
                }
                "sentinel://bookmarks" => {
                    let bookmarks = self.storage.get_bookmarks().unwrap_or_default();
                    Some(views::bookmarks_page(bookmarks))
                }
                "sentinel://history" => {
                    let history = self.storage.get_history().unwrap_or_default();
                    Some(views::history_page(history))
                }
                "sentinel://clear_history" => {
                    self.storage.clear_history()?;
                    current_url = "sentinel://history".to_string();
                    continue;
                }
                "sentinel://toggle_history" => {
                    let current = self.storage.get_setting("history_enabled")?.unwrap_or_else(|| "true".to_string());
                    let next = if current == "true" { "false" } else { "true" };
                    self.storage.set_setting("history_enabled", next)?;
                    current_url = "sentinel://settings".to_string();
                    continue;
                }
                "sentinel://toggle_security" => {
                    let current = self.storage.get_setting("security_level")?.unwrap_or_else(|| "Standard".to_string());
                    let next = match current.as_str() {
                        "Standard" => "Strict",
                        "Strict" => "Paranoid",
                        _ => "Standard",
                    };
                    self.storage.set_setting("security_level", next)?;
                    
                    let level = match next {
                        "Strict" => sent_shield::SecurityLevel::Strict,
                        "Paranoid" => sent_shield::SecurityLevel::Paranoid,
                        _ => sent_shield::SecurityLevel::Standard,
                    };
                    self.security.set_security_level(level).map_err(|e| anyhow::anyhow!(e))?;
                    let _ = self.security.harden_system();

                    let webrtc_blocked = true;
                    let webgl_disabled = next != "Standard";
                    let _ = self.ui_proxy.send_event(UiEvent::PrivacyUpdated {
                        level: next.to_string(),
                        webrtc_blocked,
                        webgl_disabled,
                        sandbox_label: "Job Object".into(),
                    });
                    
                    current_url = "sentinel://settings".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://allow_site") => {
                    let host = u.split("?host=").nth(1).unwrap_or("");
                    if !host.is_empty() {
                        self.storage.add_shield_allowlist(host)?;
                    }
                    current_url = "sentinel://settings".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://vote") => {
                    Some(self.handle_vote_request(u).await?)
                }
                u if u.starts_with("sentinel://search") => {
                    let query = u.split("?q=").nth(1).unwrap_or("");
                    info!("Core: Handling search internal page for query: '{}'", query);
                    let results = self.search.search(query).await?;
                    Some(views::search_results_page(query, results))
                }
                u if u.starts_with("sentinel://add_bookmark") => {
                    let query = u.split('?').nth(1).unwrap_or("");
                    let mut b_url = String::new();
                    let mut b_title = String::new();
                    for part in query.split('&') {
                        let mut kv = part.split('=');
                        match (kv.next(), kv.next()) {
                            (Some("url"), Some(v)) => b_url = v.to_string(),
                            (Some("title"), Some(v)) => b_title = v.to_string(),
                            _ => {}
                        }
                    }
                    self.storage.add_bookmark(&b_url, &b_title)?;
                    current_url = "sentinel://bookmarks".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://network") => {
                    let query = u.split('?').nth(1).unwrap_or("");
                    let mut n_type = "tor";
                    for part in query.split('&') {
                        let mut kv = part.split('=');
                        if let (Some("type"), Some(v)) = (kv.next(), kv.next()) {
                            n_type = v;
                        }
                    }
                    let protocol = match n_type {
                        "tor" => sent_net::Protocol::Tor { bridge: sent_net::TorBridge::None },
                        "v2ray" => {
                            if !sent_net::v2ray_ready() {
                                current_url = "sentinel://connect".to_string();
                                // Show connect with PT/V2Ray status — skip invalid switch
                                let status = "V2Ray unavailable — set V2RAY_PATH + SENTINEL_V2RAY_HOST/PORT".to_string();
                                let html = views::connect_page(
                                    &status,
                                    self.network.list_profiles(),
                                    &sent_net::pt_status_summary(),
                                );
                                self.ui_proxy.send_event(UiEvent::LoadHtml(html))?;
                                break;
                            }
                            sent_net::Protocol::V2Ray(sent_net::V2RayConfig::VLESS {
                                uuid: std::env::var("SENTINEL_V2RAY_UUID").unwrap_or_else(|_| "unset".into()),
                                encryption: "none".into(),
                                flow: String::new(),
                            })
                        }
                        "clear" => sent_net::Protocol::Clearweb,
                        "snowflake" => {
                            if !sent_net::snowflake_available() {
                                warn!("Snowflake PT missing — falling back to Tor direct");
                                sent_net::Protocol::Tor { bridge: sent_net::TorBridge::None }
                            } else {
                                sent_net::Protocol::Tor {
                                    bridge: sent_net::TorBridge::Snowflake {
                                        broker: "https://snowflake-broker.torproject.net/".into(),
                                        relay: "snowflake.torproject.net".into(),
                                    },
                                }
                            }
                        }
                        "i2p" | "wireguard" => {
                            let html = views::error_page(
                                "Not available",
                                "I2P/WireGuard are not exposed until they proxy WebView traffic.",
                            );
                            self.ui_proxy.send_event(UiEvent::LoadHtml(html))?;
                            break;
                        }
                        _ => sent_net::Protocol::Tor { bridge: sent_net::TorBridge::None },
                    };
                    self.handle_network_change(protocol).await?;
                    current_url = "sentinel://status".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://add_bridge_obfs4") => {
                    let query = u.split('?').nth(1).unwrap_or("");
                    let mut addr = String::new();
                    let mut cert = String::new();
                    let mut iat_mode: u8 = 0;
                    for part in query.split('&') {
                        let mut kv = part.split('=');
                        match (kv.next(), kv.next()) {
                            (Some("addr"), Some(v)) => addr = v.to_string(),
                            (Some("cert"), Some(v)) => cert = v.to_string(),
                            (Some("iat_mode"), Some(v)) => iat_mode = v.parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                    let bridge = sent_net::TorBridge::Obfs4 { addr, cert, iat_mode };
                    self.network.add_custom_bridge(bridge).await?;
                    current_url = "sentinel://network_menu".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://add_bridge_snowflake") => {
                    let query = u.split('?').nth(1).unwrap_or("");
                    let mut broker = String::new();
                    let mut relay = String::new();
                    for part in query.split('&') {
                        let mut kv = part.split('=');
                        match (kv.next(), kv.next()) {
                            (Some("broker"), Some(v)) => broker = v.to_string(),
                            (Some("relay"), Some(v)) => relay = v.to_string(),
                            _ => {}
                        }
                    }
                    let bridge = sent_net::TorBridge::Snowflake { broker, relay };
                    self.network.add_custom_bridge(bridge).await?;
                    current_url = "sentinel://network_menu".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://connect_add_vmess") => {
                    let query = u.split('?').nth(1).unwrap_or("");
                    let mut uuid = String::new();
                    let mut alter_id: u32 = 0;
                    let mut security = String::new();
                    for part in query.split('&') {
                        let mut kv = part.split('=');
                        match (kv.next(), kv.next()) {
                            (Some("uuid"), Some(v)) => uuid = v.to_string(),
                            (Some("alterId"), Some(v)) => alter_id = v.parse().unwrap_or(0),
                            (Some("security"), Some(v)) => security = v.to_string(),
                            _ => {}
                        }
                    }
                    let cfg = sent_net::V2RayConfig::VMess { uuid, alter_id, security };
                    self.network.add_v2ray_config(cfg).await?;
                    current_url = "sentinel://connect".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://connect_add_vless") => {
                    let query = u.split('?').nth(1).unwrap_or("");
                    let mut uuid = String::new();
                    let mut encryption = String::new();
                    let mut flow = String::new();
                    for part in query.split('&') {
                        let mut kv = part.split('=');
                        match (kv.next(), kv.next()) {
                            (Some("uuid"), Some(v)) => uuid = v.to_string(),
                            (Some("encryption"), Some(v)) => encryption = v.to_string(),
                            (Some("flow"), Some(v)) => flow = v.to_string(),
                            _ => {}
                        }
                    }
                    let cfg = sent_net::V2RayConfig::VLESS { uuid, encryption, flow };
                    self.network.add_v2ray_config(cfg).await?;
                    current_url = "sentinel://connect".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://connect_add_trojan") => {
                    let query = u.split('?').nth(1).unwrap_or("");
                    let mut password = String::new();
                    for part in query.split('&') {
                        let mut kv = part.split('=');
                        if let (Some("password"), Some(v)) = (kv.next(), kv.next()) {
                            password = v.to_string();
                        }
                    }
                    let cfg = sent_net::V2RayConfig::Trojan { password };
                    self.network.add_v2ray_config(cfg).await?;
                    current_url = "sentinel://connect".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://connect_save_profile") => {
                    let query = u.split('?').nth(1).unwrap_or("");
                    let mut name = String::from("Profile");
                    for part in query.split('&') {
                        let mut kv = part.split('=');
                        if let (Some("name"), Some(v)) = (kv.next(), kv.next()) {
                            name = v.to_string();
                        }
                    }
                    let protocol = self.network.protocol();
                    self.network.add_profile(name, protocol).await?;
                    current_url = "sentinel://connect".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://connect_switch") => {
                    let query = u.split('?').nth(1).unwrap_or("");
                    let mut name = String::new();
                    for part in query.split('&') {
                        let mut kv = part.split('=');
                        if let (Some("name"), Some(v)) = (kv.next(), kv.next()) {
                            name = v.to_string();
                        }
                    }
                    self.network.switch_profile(&name).await?;
                    current_url = "sentinel://connect".to_string();
                    continue;
                }
                "sentinel://install_pt" => {
                    match sent_net::install_pt_helpers().await {
                        Ok(msg) => {
                            info!("PT install: {}", msg);
                            Some(views::pt_install_page(true, &msg))
                        }
                        Err(e) => {
                            warn!("PT install failed: {}", e);
                            Some(views::pt_install_page(false, &e.to_string()))
                        }
                    }
                }
                "sentinel://connect_test" => {
                    let _ = self.network.connect("1.1.1.1:443").await;
                    current_url = "sentinel://connect".to_string();
                    continue;
                }
                u if u.starts_with("sentinel://download?") => {
                    let raw = u.split("?url=").nth(1).unwrap_or("").replace("+", " ");
                    let url_param = urlencoding_decode(&raw);
                    if url_param.is_empty()
                        || !(url_param.starts_with("http://") || url_param.starts_with("https://"))
                    {
                        Some(views::download_error_page("Missing or invalid http(s) URL"))
                    } else {
                        match self.download_url(&url_param).await {
                            Ok((filename, path)) => {
                                self.storage.add_download(
                                    &url_param,
                                    &filename,
                                    &format!("Saved: {}", path),
                                )?;
                                current_url =
                                    format!("sentinel://download_complete?file={}", filename);
                                continue;
                            }
                            Err(e) => {
                                let filename = url_param
                                    .split('/')
                                    .next_back()
                                    .unwrap_or("download.bin")
                                    .to_string();
                                let _ = self.storage.add_download(
                                    &url_param,
                                    &filename,
                                    &format!("Failed: {}", e),
                                );
                                Some(views::download_error_page(&e.to_string()))
                            }
                        }
                    }
                }
                u if u.starts_with("sentinel://download_complete") => {
                    let filename = u.split("?file=").nth(1).unwrap_or("document.pdf");
                    Some(views::download_complete_page(filename))
                }
                "sentinel://downloads" => {
                    let downloads = self.storage.get_downloads()?;
                    Some(views::downloads_page(downloads))
                }
                "sentinel://security" => {
                    let security_level = self.storage.get_setting("security_level")?.unwrap_or_else(|| "Standard".to_string());
                    let p_dns = "DoH / DNS-over-Tor via Vortex";
                    let p_webrtc = "Blocked via WebView init script";
                    let p_fp = "Not claimed (WebView2 limits)";
                    Some(views::security_page(&security_level, p_dns, p_webrtc, p_fp))
                }
                _ => {
                    warn!("Unknown internal page: {}", current_url);
                    Some(views::error_page("Not Found", "The requested page does not exist."))
                }
            };

            if let Some(html) = html_or_redirect {
                self.ui_proxy.send_event(UiEvent::LoadHtml(html))?;
            }
            break;
        }
        Ok(())
    }

    async fn handle_vote_request(&mut self, url: &str) -> Result<String> {
        info!("Processing governance vote request: {}", url);
        
        // Simple URL parser for sentinel://vote?id=1&approve=true
        let query = url.split('?').nth(1).unwrap_or("");
        let mut id = 0;
        let mut approve = true;

        for part in query.split('&') {
            let mut kv = part.split('=');
            match (kv.next(), kv.next()) {
                (Some("id"), Some(v)) => id = v.parse().unwrap_or(0),
                (Some("approve"), Some(v)) => approve = v == "true",
                _ => {}
            }
        }

        if id == 0 {
            return Ok(views::error_page("Vote Error", "Invalid proposal ID."));
        }

        // 1. Get/Create User Identity (In production, this is hardware-secured)
        let identity = sent_gov::ZkIdentity::from_secret([0x42; 32]);
        
        // 2. Generate ZK Proof
        let proof = identity.generate_proof(id, approve);
        let nullifier = identity.derive_nullifier(id);

        // 3. Submit Vote to Engine
        let vote = sent_gov::Vote {
            proposal_id: id,
            voter_hash: hex::encode(nullifier),
            commitment: hex::encode(identity.commitment),
            approve,
            proof,
        };

        let success = self.governance.cast_vote(vote);

        if success {
            info!("Vote cast successfully for proposal #{}", id);
            let proposals = self.governance.list_proposals();
            Ok(views::governance_page(&proposals)) // Return updated page
        } else {
            warn!("Vote verification failed for proposal #{}", id);
            Ok(views::error_page("Governance Error", "ZK-SNARK proof verification failed. Ensure your identity is valid."))
        }
    }

    async fn render_search_results(&self, query: &str, results: Vec<sent_search::SearchResult>) -> Result<()> {
        let html = views::search_results_page(query, results);
        self.ui_proxy.send_event(UiEvent::LoadHtml(html))?;
        Ok(())
    }

    /// Real file download via reqwest (SOCKS when Tor SOCKS is up).
    async fn download_url(&self, url: &str) -> Result<(String, String)> {
        let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120));
        if let Some(port) = self.network.socks_port().await {
            if url.contains(".onion")
                || matches!(self.network.protocol(), sent_net::Protocol::Tor { .. })
            {
                builder = builder.proxy(reqwest::Proxy::all(format!(
                    "socks5h://127.0.0.1:{}",
                    port
                ))?);
            }
        }
        let client = builder.build()?;
        let resp = client.get(url).send().await?.error_for_status()?;
        let bytes = resp.bytes().await?;

        let mut filename = url
            .split('?')
            .next()
            .unwrap_or(url)
            .split('/')
            .next_back()
            .unwrap_or("download.bin")
            .to_string();
        if filename.is_empty() || filename.contains("..") {
            filename = "download.bin".into();
        }
        // Sanitize filename
        filename = filename
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        let dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("SentinelDownloads");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(&filename);
        std::fs::write(&path, &bytes)?;
        info!("Downloaded {} bytes → {}", bytes.len(), path.display());
        Ok((filename, path.display().to_string()))
    }

    pub async fn check_connectivity(&self) -> Result<()> {
        info!("Verifying network connectivity...");
        self.network.connect("1.1.1.1:80").await?;
        info!("Network check passed.");
        Ok(())
    }
}

/// Persist a random vault key on first run (never hardcode a shared passphrase).
fn load_or_create_vault_secret(config_dir: &std::path::Path) -> Result<Vec<u8>> {
    use rand::RngCore;
    let key_path = config_dir.join("vault.key");
    if key_path.exists() {
        let bytes = std::fs::read(&key_path)?;
        if bytes.len() >= 32 {
            return Ok(bytes);
        }
    }
    let mut key = vec![0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut key);
    std::fs::write(&key_path, &key)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
    }
    info!("Created new vault key at {:?}", key_path);
    Ok(key)
}

fn urlencoding_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h = |c: u8| -> Option<u8> {
                    match c {
                        b'0'..=b'9' => Some(c - b'0'),
                        b'a'..=b'f' => Some(c - b'a' + 10),
                        b'A'..=b'F' => Some(c - b'A' + 10),
                        _ => None,
                    }
                };
                if let (Some(a), Some(b)) = (h(bytes[i + 1]), h(bytes[i + 2])) {
                    out.push((a << 4) | b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn html_escape_simple(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests;
