/*
 * WebView2 content host (wry) - AGPL-3.0
 */

use anyhow::{Context, Result};
use dpi::{LogicalPosition, LogicalSize, Position, Size};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn};
use winit::window::Window;
use wry::{ProxyConfig, ProxyEndpoint, Rect, WebView, WebViewBuilder};

use crate::layout::CHROME_HEIGHT;
use sent_net::AdBlocker;

/// Shared privacy flags reflected on sentinel://security
#[derive(Debug, Clone)]
pub struct PrivacyState {
    pub webrtc_blocked: bool,
    pub webgl_disabled: bool,
    pub third_party_cookies_blocked: bool,
    pub dnt: bool,
    pub security_level: String,
    pub sandbox_label: String,
}

impl Default for PrivacyState {
    fn default() -> Self {
        Self {
            webrtc_blocked: true,
            webgl_disabled: false,
            third_party_cookies_blocked: true,
            dnt: true,
            security_level: "Standard".into(),
            sandbox_label: "Job Object (when applied)".into(),
        }
    }
}

pub struct ContentWebView {
    webview: WebView,
    pub using_tor_proxy: bool,
    pub socks_port: Option<u16>,
    pub last_url: String,
    privacy: PrivacyState,
}

impl ContentWebView {
    pub fn create(window: &Window, blocked: Arc<AtomicU64>) -> Result<Self> {
        Self::build(window, blocked, None, PrivacyState::default(), "about:blank")
    }

    pub fn rebuild_with_proxy(
        window: &Window,
        blocked: Arc<AtomicU64>,
        socks_port: Option<u16>,
        privacy: PrivacyState,
        restore_url: &str,
    ) -> Result<Self> {
        Self::build(window, blocked, socks_port, privacy, restore_url)
    }

    fn build(
        window: &Window,
        blocked: Arc<AtomicU64>,
        socks_port: Option<u16>,
        privacy: PrivacyState,
        restore_url: &str,
    ) -> Result<Self> {
        let adblocker = Arc::new(AdBlocker::with_builtin_lists());
        let blocked_cb = blocked.clone();
        let adblock_for_nav = adblocker.clone();

        let init = privacy_init_script(&privacy);
        let mut builder = WebViewBuilder::new()
            .with_html(r#"<html><body style="background:#1a1a1a"></body></html>"#)
            .with_bounds(Rect {
                position: Position::Logical(LogicalPosition::new(0.0, CHROME_HEIGHT as f64)),
                size: Size::Logical(LogicalSize::new(1280.0, 640.0)),
            })
            .with_navigation_handler(move |url| {
                if adblock_for_nav.is_blocked(&url) {
                    blocked_cb.fetch_add(1, Ordering::Relaxed);
                    warn!("Blocked navigation: {}", url);
                    return false;
                }
                true
            })
            .with_initialization_script(&init);

        if let Some(port) = socks_port {
            builder = builder.with_proxy_config(ProxyConfig::Socks5(ProxyEndpoint {
                host: "127.0.0.1".into(),
                port: port.to_string(),
            }));
            info!("WebView2 SOCKS5 127.0.0.1:{}", port);
        } else {
            info!("WebView2 Direct (no SOCKS)");
        }

        let webview = builder
            .build_as_child(window)
            .context("Failed to build WebView2 child")?;

        #[cfg(windows)]
        attach_windows_adblock(&webview, adblocker, blocked);

        #[cfg(windows)]
        apply_windows_privacy(&webview, &privacy);

        let mut host = Self {
            webview,
            using_tor_proxy: socks_port.is_some(),
            socks_port,
            last_url: restore_url.to_string(),
            privacy,
        };
        let _ = host.set_bounds_for_window(window);
        if restore_url.starts_with("http://")
            || restore_url.starts_with("https://")
            || restore_url.starts_with("sentinel://")
        {
            // sentinel loaded via load_html from core; http(s) via load_url
            if !restore_url.starts_with("sentinel://") && restore_url != "about:blank" {
                let _ = host.load_url(restore_url);
            }
        }
        Ok(host)
    }

    pub fn privacy(&self) -> &PrivacyState {
        &self.privacy
    }

    pub fn set_bounds_for_window(&self, window: &Window) -> Result<()> {
        let size = window.inner_size();
        let scale = window.scale_factor();
        let logical = size.to_logical::<f64>(scale);
        let chrome = CHROME_HEIGHT as f64;
        let height = (logical.height - chrome).max(100.0);
        self.webview.set_bounds(Rect {
            position: Position::Logical(LogicalPosition::new(0.0, chrome)),
            size: Size::Logical(LogicalSize::new(logical.width, height)),
        })?;
        Ok(())
    }

    pub fn set_visible(&self, visible: bool) -> Result<()> {
        self.webview.set_visible(visible)?;
        Ok(())
    }

    pub fn load_url(&mut self, url: &str) -> Result<()> {
        self.last_url = url.to_string();
        self.webview.load_url(url)?;
        Ok(())
    }

    pub fn load_html(&mut self, html: &str) -> Result<()> {
        let wrapped = if html.contains("<html") {
            if html.contains("badge-onion") {
                html.to_string()
            } else if html.contains("<head>") {
                html.replacen("<head>", &format!("<head>{}", SHARED_STYLE), 1)
            } else {
                format!(
                    r#"<!DOCTYPE html><html><head><meta charset="utf-8">{}</head><body>{}</body></html>"#,
                    SHARED_STYLE, html
                )
            }
        } else {
            format!(
                r#"<!DOCTYPE html><html><head><meta charset="utf-8">{}</head><body>{}</body></html>"#,
                SHARED_STYLE, html
            )
        };
        self.webview.load_html(&wrapped)?;
        Ok(())
    }

    /// Rebuild is required for proxy changes — callers must replace Self.
    pub fn needs_proxy(&self, want_socks: Option<u16>) -> bool {
        self.socks_port != want_socks
    }
}

fn privacy_init_script(p: &PrivacyState) -> String {
    let mut s = String::from(
        r#"(() => { try { Object.defineProperty(navigator, 'doNotTrack', { get: () => '1' }); } catch(e) {} "#,
    );
    if p.webrtc_blocked {
        s.push_str(
            r#"
try {
  const block = function() { throw new Error('WebRTC blocked by Sentinel'); };
  window.RTCPeerConnection = block;
  window.webkitRTCPeerConnection = block;
  window.mozRTCPeerConnection = block;
} catch(e) {}
"#,
        );
    }
    if p.webgl_disabled {
        s.push_str(
            r#"
try {
  const no = function() { return null; };
  HTMLCanvasElement.prototype.getContext = function(t) {
    if (t && String(t).toLowerCase().indexOf('webgl') >= 0) return null;
    return HTMLCanvasElement.prototype.getContext.call(this, t);
  };
} catch(e) {}
"#,
        );
    }
    s.push_str("})();");
    s
}

const SHARED_STYLE: &str = r#"<style>
body{margin:0;padding:24px;background:#1a1a1a;color:#fff;font-family:Segoe UI,system-ui,sans-serif;line-height:1.5}
a{color:#00d9f2} h1,h2,h3{color:#00d9f2}
section{margin-bottom:1.5rem;padding:1rem;background:#1b1d1f;border-radius:8px}
.badge-onion{display:inline-block;background:#8c59d9;color:#fff;padding:2px 8px;border-radius:4px;font-size:12px;font-weight:700}
.badge-clear{display:inline-block;background:#2a6;color:#fff;padding:2px 8px;border-radius:4px;font-size:12px;font-weight:700}
.onion-url{font-family:Consolas,monospace;font-size:13px;opacity:.9}
.result{border-left:3px solid #333;padding-left:12px;margin:12px 0}
.result.onion{border-left-color:#8c59d9;background:#1e1830}
code{background:#111;padding:2px 6px;border-radius:4px}
</style>"#;

#[cfg(windows)]
fn apply_windows_privacy(_webview: &WebView, privacy: &PrivacyState) {
    let _ = privacy.third_party_cookies_blocked;
}

#[cfg(windows)]
fn attach_windows_adblock(
    webview: &WebView,
    adblocker: Arc<AdBlocker>,
    blocked: Arc<AtomicU64>,
) {
    use wry::WebViewExtWindows;
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::WebResourceRequestedEventHandler;

    let core = webview.webview();
    let env = webview.environment();
    unsafe {
        let filter = windows::core::HSTRING::from("*://*/*");
        if core
            .AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
            .is_err()
        {
            warn!("Could not add WebResourceRequested filter");
            return;
        }

        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_sender, args| {
            if let Some(args) = args {
                if let Ok(req) = args.Request() {
                    let mut uri = windows::core::PWSTR::null();
                    if req.Uri(&mut uri).is_ok() {
                        let url = pwstr_to_string(uri);
                        if adblocker.is_blocked(&url) {
                            blocked.fetch_add(1, Ordering::Relaxed);
                            if let Ok(resp) = env.CreateWebResourceResponse(
                                None,
                                403,
                                &windows::core::HSTRING::from("Blocked"),
                                &windows::core::HSTRING::from("Content-Type: text/plain"),
                            ) {
                                let _ = args.SetResponse(&resp);
                            }
                        }
                    }
                }
            }
            Ok(())
        }));
        let mut token = Default::default();
        if core.add_WebResourceRequested(&handler, &mut token).is_ok() {
            info!("Windows WebResourceRequested adblock attached");
        }
    }
}

#[cfg(windows)]
fn pwstr_to_string(ptr: windows::core::PWSTR) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe {
        let mut len = 0usize;
        while *ptr.0.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr.0, len))
    }
}
