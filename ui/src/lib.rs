/*
 * Sentinel Browser UI (Spectre chrome + WebView2 content) - AGPL-3.0
 * Copyright (C) 2026 Sentinel DAO
 */

use anyhow::Result;
use tracing::{error, info, warn};
use winit::{
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    keyboard::{Key, NamedKey},
    window::{CursorIcon, WindowBuilder},
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::Sender;

mod spectre;
mod layout;
mod webview_host;
mod tab_pool;

use spectre::Spectre;
use layout::{Color, DisplayMode, LayoutBox, CHROME_HEIGHT};
use webview_host::ContentWebView;
use tab_pool::TabWebViewPool;

use sent_net::{Protocol, TorBridge, V2RayConfig};

#[derive(Debug, Clone)]
pub enum BrowserCommand {
    Navigate(String),
    Back,
    Forward,
    Refresh,
    ChangeNetwork(Protocol),
    AddBridge(TorBridge),
    AddV2Ray(V2RayConfig),
    NewTab,
    SwitchTab(u32),
    CloseTab(u32),
    SetProxyMode { use_tor: bool, socks_port: u16 },
    /// Push privacy / security level into the WebView host.
    SetPrivacy {
        level: String,
        webrtc_blocked: bool,
        webgl_disabled: bool,
    },
}

#[derive(Debug, Clone)]
pub struct TabInfo {
    pub id: u32,
    pub title: String,
    pub url: String,
    pub active: bool,
}

#[derive(Debug)]
pub enum UiEvent {
    /// Internal pages (sentinel://) rendered as HTML string.
    LoadHtml(String),
    /// External clearnet / onion URL loaded in the real engine.
    LoadUrl(String),
    NetworkStatusChanged(Protocol),
    ConfigurationError(String),
    SetVortex(Arc<sent_net::Vortex>),
    ShieldBlocked(u64),
    SocksReady(u16),
    SetUrlBar(String),
    PrivacyUpdated {
        level: String,
        webrtc_blocked: bool,
        webgl_disabled: bool,
        sandbox_label: String,
    },
    /// Full tab strip sync from Aegis.
    TabsChanged(Vec<TabInfo>),
}

pub struct WindowManager {
    event_loop: Option<EventLoop<UiEvent>>,
    proxy: EventLoopProxy<UiEvent>,
    command_tx: Option<Sender<BrowserCommand>>,
}

impl WindowManager {
    pub fn new() -> Result<Self> {
        info!("Initializing WindowManager");
        let event_loop = EventLoopBuilder::<UiEvent>::with_user_event().build()?;
        let proxy = event_loop.create_proxy();
        Ok(Self {
            event_loop: Some(event_loop),
            proxy,
            command_tx: None,
        })
    }

    pub fn set_command_sender(&mut self, tx: Sender<BrowserCommand>) {
        self.command_tx = Some(tx);
    }

    pub fn get_proxy(&self) -> EventLoopProxy<UiEvent> {
        self.proxy.clone()
    }

    pub fn run(mut self) -> Result<()> {
        let event_loop = self.event_loop.take().unwrap();
        let window = Arc::new(
            WindowBuilder::new()
                .with_title("Sentinel Browser")
                .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
                .build(&event_loop)?,
        );

        info!("Window created successfully");

        let mut spectre = pollster::block_on(Spectre::new(window.clone()));
        let blocked_count = Arc::new(AtomicU64::new(0));
        let mut pool = match ContentWebView::create(&window, blocked_count.clone()) {
            Ok(wv) => {
                let _ = wv.set_bounds_for_window(&window);
                let mut p = TabWebViewPool::new(wv, 0);
                if let Some(wv) = p.active_mut() {
                    let _ = wv.load_html(&welcome_splash());
                }
                Some(p)
            }
            Err(e) => {
                error!("Failed to create WebView2 content engine: {:?}", e);
                None
            }
        };

        let mut url_bar_buffer = String::from("sentinel://home");
        let mut url_focused = true;
        let mut network_label = String::from("Tor");
        let mut shield_blocked: u64 = 0;
        let mut socks_port: Option<u16> = None;
        let mut tabs: Vec<TabInfo> = Vec::new();
        let mut active_tab_id: u32 = 0;
        let mut cursor_blink_visible = false;
        let mut last_blink_toggle = Instant::now();
        let blink_interval = Duration::from_millis(500);
        let mut cursor_position = (0.0, 0.0);
        let command_tx = self.command_tx.take();

        event_loop.run(move |event, elwt| {
            if url_focused {
                elwt.set_control_flow(ControlFlow::WaitUntil(last_blink_toggle + blink_interval));
            } else {
                elwt.set_control_flow(ControlFlow::Wait);
            }

            match event {
                Event::UserEvent(UiEvent::SetVortex(_)) => {
                    info!("Vortex handle received by UI");
                }
                Event::UserEvent(UiEvent::PrivacyUpdated {
                    level,
                    webrtc_blocked,
                    webgl_disabled,
                    sandbox_label,
                }) => {
                    if let Some(ref mut p) = pool {
                        let restore = p.last_url();
                        let mut privacy = p.privacy().clone();
                        privacy.security_level = level;
                        privacy.webrtc_blocked = webrtc_blocked;
                        privacy.webgl_disabled = webgl_disabled;
                        privacy.sandbox_label = sandbox_label;
                        let want = if p.using_tor_proxy() {
                            socks_port
                        } else {
                            None
                        };
                        if let Err(e) = p.rebuild_all(
                            &window,
                            blocked_count.clone(),
                            want,
                            privacy,
                            &restore,
                            active_tab_id,
                        ) {
                            warn!("Privacy rebuild failed: {:?}", e);
                        }
                    }
                }
                Event::UserEvent(UiEvent::SocksReady(port)) => {
                    info!("SOCKS ready on {} — rebuild WebView if needed", port);
                    socks_port = Some(port);
                    if let Some(ref mut p) = pool {
                        let restore = p.last_url();
                        let privacy = p.privacy().clone();
                        let want = if network_label == "Tor" || restore.contains(".onion") {
                            Some(port)
                        } else {
                            None
                        };
                        if let Err(e) = p.rebuild_all(
                            &window,
                            blocked_count.clone(),
                            want,
                            privacy,
                            &restore,
                            active_tab_id,
                        ) {
                            warn!("Rebuild failed: {:?}", e);
                        }
                    }
                }
                Event::UserEvent(UiEvent::ShieldBlocked(n)) => {
                    shield_blocked = n;
                    window.request_redraw();
                }
                Event::UserEvent(UiEvent::SetUrlBar(url)) => {
                    url_bar_buffer = url;
                    window.request_redraw();
                }
                Event::UserEvent(UiEvent::TabsChanged(next)) => {
                    tabs = next;
                    if let Some(active) = tabs.iter().find(|t| t.active) {
                        url_bar_buffer = active.url.clone();
                        let id = active.id;
                        let url = active.url.clone();
                        let live: std::collections::HashSet<u32> =
                            tabs.iter().map(|t| t.id).collect();
                        if let Some(ref mut p) = pool {
                            for stale in p.cached_tab_ids() {
                                if !live.contains(&stale) {
                                    p.close_tab(stale);
                                }
                            }
                            if let Err(e) =
                                p.switch_to(&window, blocked_count.clone(), id, &url)
                            {
                                warn!("Tab switch WebView failed: {:?}", e);
                            }
                        }
                        active_tab_id = id;
                    }
                    window.request_redraw();
                }
                Event::UserEvent(UiEvent::NetworkStatusChanged(protocol)) => {
                    network_label = match protocol {
                        Protocol::Clearweb => "Direct".into(),
                        Protocol::Tor { .. } => "Tor".into(),
                        Protocol::I2P => "I2P".into(),
                        Protocol::V2Ray(_) => "V2Ray".into(),
                        Protocol::WireGuard { .. } => "WG".into(),
                    };
                    if let Some(ref mut p) = pool {
                        let restore = p.last_url();
                        let privacy = p.privacy().clone();
                        let want = if network_label == "Tor" || network_label == "V2Ray" {
                            socks_port
                        } else if restore.contains(".onion") {
                            socks_port
                        } else {
                            None
                        };
                        if let Err(e) = p.rebuild_all(
                            &window,
                            blocked_count.clone(),
                            want,
                            privacy,
                            &restore,
                            active_tab_id,
                        ) {
                            warn!("Rebuild on network change failed: {:?}", e);
                        }
                    }
                    window.request_redraw();
                }
                Event::UserEvent(UiEvent::ConfigurationError(msg)) => {
                    warn!("Config error: {}", msg);
                    if let Some(ref mut p) = pool {
                        let _ = p.load_html_active(&format!(
                            "<html><body style='background:#1a1a1a;color:#fff;font-family:sans-serif;padding:2rem'><h1>Configuration</h1><p>{}</p></body></html>",
                            html_escape(&msg)
                        ));
                    }
                }
                Event::UserEvent(UiEvent::LoadHtml(html)) => {
                    info!("LoadHtml → WebView2");
                    if let Some(ref mut p) = pool {
                        if let Some(wv) = p.active_mut() {
                            wv.last_url = url_bar_buffer.clone();
                        }
                        if let Err(e) = p.load_html_active(&html) {
                            error!("load_html failed: {:?}", e);
                        }
                    }
                    window.request_redraw();
                }
                Event::UserEvent(UiEvent::LoadUrl(url)) => {
                    info!("LoadUrl → WebView2: {}", url);
                    url_bar_buffer = url.clone();
                    let need_socks =
                        url.contains(".onion") || network_label == "Tor" || network_label == "V2Ray";
                    let want = if need_socks { socks_port } else { None };
                    if let Some(ref mut p) = pool {
                        let privacy = p.privacy().clone();
                        if p.needs_proxy(want) {
                            if let Err(e) = p.rebuild_all(
                                &window,
                                blocked_count.clone(),
                                want,
                                privacy,
                                &url,
                                active_tab_id,
                            ) {
                                error!("Rebuild for LoadUrl failed: {:?}", e);
                            }
                        } else if let Err(e) = p.load_url_active(&url) {
                            error!("load_url failed: {:?}", e);
                        }
                    }
                    window.request_redraw();
                }
                Event::WindowEvent {
                    ref event,
                    window_id,
                } if window_id == window.id() => {
                    match event {
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::Resized(physical_size) => {
                            spectre.resize(*physical_size);
                            if let Some(ref p) = pool {
                                let _ = p.set_bounds_for_window(&window);
                            }
                            window.request_redraw();
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            cursor_position = (position.x, position.y);
                            let (cx, cy) = cursor_position;
                            let width = window.inner_size().width as f64;
                            let over_chrome = cy < CHROME_HEIGHT as f64;
                            let over_url = over_chrome
                                && (170.0..=(width - 90.0)).contains(&cx)
                                && (45.0..=75.0).contains(&cy);
                            let over_btn = over_chrome && (
                                ((10.0..=40.0).contains(&cx) && (45.0..=75.0).contains(&cy))
                                    || ((50.0..=160.0).contains(&cx) && (45.0..=75.0).contains(&cy))
                                    || (((width - 90.0)..=(width - 10.0)).contains(&cx)
                                        && (45.0..=75.0).contains(&cy))
                                    || ((0.0..=200.0).contains(&cx) && (0.0..=35.0).contains(&cy))
                                    || ((205.0..=235.0).contains(&cx) && (5.0..=30.0).contains(&cy))
                            );
                            if over_url {
                                window.set_cursor_icon(CursorIcon::Text);
                            } else if over_btn {
                                window.set_cursor_icon(CursorIcon::Pointer);
                            } else {
                                window.set_cursor_icon(CursorIcon::Default);
                            }
                        }
                        WindowEvent::MouseWheel { .. } => {
                            // Content scrolling is handled by WebView2.
                        }
                        WindowEvent::MouseInput { state, button, .. } => {
                            if *state == ElementState::Pressed && *button == MouseButton::Left {
                                let (cx, cy) = cursor_position;
                                if cy > CHROME_HEIGHT as f64 {
                                    return;
                                }
                                let width = window.inner_size().width as f64;
                                let dispatch = |tx: &Option<Sender<BrowserCommand>>, cmd: BrowserCommand| {
                                    if let Some(tx) = tx {
                                        if let Err(e) = tx.blocking_send(cmd) {
                                            error!("Dispatch failed: {:?}", e);
                                        }
                                    }
                                };

                                let strip = tab_strip_hits(&tabs, width as f32);
                                if (10.0..=40.0).contains(&cx) && (45.0..=75.0).contains(&cy) {
                                    dispatch(&command_tx, BrowserCommand::Navigate("sentinel://network_menu".into()));
                                } else if (50.0..=80.0).contains(&cx) && (45.0..=75.0).contains(&cy) {
                                    dispatch(&command_tx, BrowserCommand::Back);
                                } else if (90.0..=120.0).contains(&cx) && (45.0..=75.0).contains(&cy) {
                                    dispatch(&command_tx, BrowserCommand::Forward);
                                } else if (130.0..=160.0).contains(&cx) && (45.0..=75.0).contains(&cy) {
                                    dispatch(&command_tx, BrowserCommand::Refresh);
                                } else if let Some(action) = strip.hit(cx, cy) {
                                    match action {
                                        TabHit::New => dispatch(&command_tx, BrowserCommand::NewTab),
                                        TabHit::Switch(id) => {
                                            dispatch(&command_tx, BrowserCommand::SwitchTab(id))
                                        }
                                        TabHit::Close(id) => {
                                            dispatch(&command_tx, BrowserCommand::CloseTab(id))
                                        }
                                    }
                                } else if ((width - 90.0)..=(width - 10.0)).contains(&cx)
                                    && (45.0..=75.0).contains(&cy)
                                {
                                    dispatch(&command_tx, BrowserCommand::Navigate(resolve_input(&url_bar_buffer)));
                                } else if (170.0..=(width - 90.0)).contains(&cx)
                                    && (45.0..=75.0).contains(&cy)
                                {
                                    url_focused = true;
                                    cursor_blink_visible = true;
                                    last_blink_toggle = Instant::now();
                                    window.request_redraw();
                                } else {
                                    url_focused = false;
                                    window.request_redraw();
                                }
                            }
                        }
                        WindowEvent::KeyboardInput { event, .. } => {
                            if event.state == ElementState::Pressed {
                                if let Key::Named(NamedKey::F1) = &event.logical_key {
                                    if let Some(tx) = &command_tx {
                                        let _ = tx.blocking_send(BrowserCommand::Navigate(
                                            "sentinel://network_menu".into(),
                                        ));
                                    }
                                }
                            }
                            if event.state == ElementState::Pressed && url_focused {
                                match &event.logical_key {
                                    Key::Named(NamedKey::Enter) => {
                                        if let Some(tx) = &command_tx {
                                            let _ = tx.blocking_send(BrowserCommand::Navigate(
                                                resolve_input(&url_bar_buffer),
                                            ));
                                        }
                                    }
                                    Key::Named(NamedKey::Backspace) => {
                                        url_bar_buffer.pop();
                                        window.request_redraw();
                                    }
                                    Key::Character(c) => {
                                        if !c.chars().any(|ch| ch.is_control()) {
                                            url_bar_buffer.push_str(c);
                                            window.request_redraw();
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        WindowEvent::RedrawRequested => {
                            if url_focused && last_blink_toggle.elapsed() >= blink_interval {
                                cursor_blink_visible = !cursor_blink_visible;
                                last_blink_toggle = Instant::now();
                            }
                            shield_blocked = blocked_count.load(Ordering::Relaxed);
                            let display_url = if url_focused && cursor_blink_visible {
                                format!("{}|", url_bar_buffer)
                            } else {
                                url_bar_buffer.clone()
                            };
                            let chrome = build_chrome_boxes(
                                window.inner_size().width as f32,
                                &display_url,
                                &network_label,
                                shield_blocked,
                                &tabs,
                            );
                            match spectre.render_chrome(&chrome) {
                                Ok(_) => {}
                                Err(wgpu::SurfaceError::Lost) => spectre.resize(spectre.size),
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => error!("Render error: {:?}", e),
                            }
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => {
                    if url_focused && last_blink_toggle.elapsed() >= blink_interval {
                        window.request_redraw();
                    }
                    // Sync shield counter from adblock
                    let n = blocked_count.load(Ordering::Relaxed);
                    if n != shield_blocked {
                        shield_blocked = n;
                        window.request_redraw();
                    }
                }
                _ => {}
            }
        })?;

        Ok(())
    }
}

fn resolve_input(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('|');
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("sentinel://")
    {
        trimmed.to_string()
    } else if trimmed.contains('.') && !trimmed.contains(' ') {
        format!("https://{}", trimmed)
    } else {
        format!("sentinel://search?q={}", urlencoding_lite(trimmed))
    }
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn welcome_splash() -> String {
    r#"<!DOCTYPE html><html><head><meta charset="utf-8"><style>
body{margin:0;background:#1a1a1a;color:#fff;font-family:Segoe UI,sans-serif;display:flex;align-items:center;justify-content:center;height:100vh}
.card{text-align:center}h1{color:#00d9f2;font-size:2.5rem;margin:0}p{opacity:.8}
</style></head><body><div class="card"><h1>SENTINEL</h1><p>Loading Horus search &amp; Vortex…</p></div></body></html>"#.into()
}

enum TabHit {
    Switch(u32),
    Close(u32),
    New,
}

struct TabStripHits {
    tabs: Vec<(u32, f32, f32, f32, f32)>, // id, x, y, w, h
    closes: Vec<(u32, f32, f32, f32, f32)>,
    plus: (f32, f32, f32, f32),
}

impl TabStripHits {
    fn hit(&self, cx: f64, cy: f64) -> Option<TabHit> {
        for &(id, x, y, w, h) in &self.closes {
            if (x as f64..=(x + w) as f64).contains(&cx) && (y as f64..=(y + h) as f64).contains(&cy)
            {
                return Some(TabHit::Close(id));
            }
        }
        for &(id, x, y, w, h) in &self.tabs {
            if (x as f64..=(x + w) as f64).contains(&cx) && (y as f64..=(y + h) as f64).contains(&cy)
            {
                return Some(TabHit::Switch(id));
            }
        }
        let (x, y, w, h) = self.plus;
        if (x as f64..=(x + w) as f64).contains(&cx) && (y as f64..=(y + h) as f64).contains(&cy) {
            Some(TabHit::New)
        } else {
            None
        }
    }
}

fn tab_strip_hits(tabs: &[TabInfo], width: f32) -> TabStripHits {
    let brand_w = 90.0;
    let tab_w = 110.0f32;
    let close_w = 18.0;
    let max_tabs = (((width - brand_w - 220.0) / tab_w).floor() as usize).max(1);
    let mut hits = TabStripHits {
        tabs: Vec::new(),
        closes: Vec::new(),
        plus: (brand_w + 5.0, 5.0, 30.0, 25.0),
    };
    let mut x = brand_w + 5.0;
    for tab in tabs.iter().take(max_tabs) {
        hits.tabs.push((tab.id, x, 5.0, tab_w - close_w - 4.0, 25.0));
        hits
            .closes
            .push((tab.id, x + tab_w - close_w - 2.0, 7.0, close_w, 21.0));
        x += tab_w + 4.0;
    }
    hits.plus = (x, 5.0, 30.0, 25.0);
    hits
}

fn build_chrome_boxes(
    width: f32,
    url_text: &str,
    network: &str,
    blocked: u64,
    tabs: &[TabInfo],
) -> Vec<LayoutBox> {
    let col_bg = Color::INPUT;
    let col_accent = Color::ACCENT;
    let col_card = Color::CARD_BG;
    let mut boxes = Vec::new();

    boxes.push(LayoutBox {
        x: 0.0,
        y: 0.0,
        width,
        height: CHROME_HEIGHT,
        color: Color::BG,
        text: None,
        link: None,
        display: DisplayMode::Block,
    });
    boxes.push(LayoutBox {
        x: 0.0,
        y: 0.0,
        width: 90.0,
        height: 35.0,
        color: col_card,
        text: Some("Sentinel".into()),
        link: None,
        display: DisplayMode::Block,
    });

    let strip = tab_strip_hits(tabs, width);
    for tab in tabs.iter().take(strip.tabs.len()) {
        if let Some(&(_, x, y, w, h)) = strip.tabs.iter().find(|(id, ..)| *id == tab.id) {
            let label: String = tab.title.chars().take(12).collect();
            boxes.push(LayoutBox {
                x,
                y,
                width: w,
                height: h,
                color: if tab.active { col_accent } else { col_card },
                text: Some(if label.is_empty() {
                    "Tab".into()
                } else {
                    label
                }),
                link: Some(format!("tab:{}", tab.id)),
                display: DisplayMode::Block,
            });
        }
        if let Some(&(_, x, y, w, h)) = strip.closes.iter().find(|(id, ..)| *id == tab.id) {
            boxes.push(LayoutBox {
                x,
                y,
                width: w,
                height: h,
                color: Color::INPUT,
                text: Some("×".into()),
                link: Some(format!("close:{}", tab.id)),
                display: DisplayMode::Block,
            });
        }
    }
    let (px, py, pw, ph) = strip.plus;
    boxes.push(LayoutBox {
        x: px,
        y: py,
        width: pw,
        height: ph,
        color: col_accent,
        text: Some("+".into()),
        link: Some("newtab".into()),
        display: DisplayMode::Block,
    });
    let status_x = (px + pw + 10.0).max(250.0);
    boxes.push(LayoutBox {
        x: status_x,
        y: 5.0,
        width: 70.0,
        height: 25.0,
        color: col_card,
        text: Some(network.to_string()),
        link: None,
        display: DisplayMode::Block,
    });
    boxes.push(LayoutBox {
        x: status_x + 80.0,
        y: 5.0,
        width: 100.0,
        height: 25.0,
        color: Color::ONION,
        text: Some(format!("Shield {}", blocked)),
        link: None,
        display: DisplayMode::Block,
    });

    for (i, (x, label)) in [(10.0, "="), (50.0, "<"), (90.0, ">"), (130.0, "R")]
        .into_iter()
        .enumerate()
    {
        let _ = i;
        boxes.push(LayoutBox {
            x,
            y: 45.0,
            width: 30.0,
            height: 30.0,
            color: col_bg,
            text: Some(label.into()),
            link: None,
            display: DisplayMode::Block,
        });
    }

    let search_btn_width = 80.0;
    let url_start_x = 170.0;
    let url_bar_width = width - (url_start_x + search_btn_width + 10.0);
    boxes.push(LayoutBox {
        x: url_start_x,
        y: 45.0,
        width: url_bar_width.max(40.0),
        height: 30.0,
        color: col_bg,
        text: Some(url_text.to_string()),
        link: None,
        display: DisplayMode::Block,
    });
    boxes.push(LayoutBox {
        x: url_start_x + url_bar_width.max(40.0) + 10.0,
        y: 45.0,
        width: search_btn_width,
        height: 30.0,
        color: col_accent,
        text: Some("GO".into()),
        link: None,
        display: DisplayMode::Block,
    });
    boxes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_url_and_search() {
        assert_eq!(resolve_input("https://example.com"), "https://example.com");
        assert_eq!(resolve_input("example.com"), "https://example.com");
        assert!(resolve_input("privacy tools").starts_with("sentinel://search?q="));
    }
}
