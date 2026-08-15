/*
 * Small pool of WebView2 instances for faster tab switching.
 * At most MAX_CACHED live views; LRU eviction. Proxy rebuild clears the pool.
 */

use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tracing::{info, warn};
use winit::window::Window;

use super::webview_host::{ContentWebView, PrivacyState};

const MAX_CACHED: usize = 3;

pub struct TabWebViewPool {
    slots: HashMap<u32, ContentWebView>,
    lru: VecDeque<u32>,
    active: Option<u32>,
    socks_port: Option<u16>,
    privacy: PrivacyState,
}

impl TabWebViewPool {
    pub fn new(initial: ContentWebView, tab_id: u32) -> Self {
        let socks_port = initial.socks_port;
        let privacy = initial.privacy().clone();
        let mut slots = HashMap::new();
        slots.insert(tab_id, initial);
        let mut lru = VecDeque::new();
        lru.push_back(tab_id);
        Self {
            slots,
            lru,
            active: Some(tab_id),
            socks_port,
            privacy,
        }
    }

    pub fn cached_tab_ids(&self) -> Vec<u32> {
        self.slots.keys().copied().collect()
    }

    pub fn active_mut(&mut self) -> Option<&mut ContentWebView> {
        let id = self.active?;
        self.slots.get_mut(&id)
    }

    pub fn active(&self) -> Option<&ContentWebView> {
        let id = self.active?;
        self.slots.get(&id)
    }

    pub fn privacy(&self) -> &PrivacyState {
        &self.privacy
    }

    pub fn using_tor_proxy(&self) -> bool {
        self.active().map(|c| c.using_tor_proxy).unwrap_or(false)
    }

    pub fn last_url(&self) -> String {
        self.active()
            .map(|c| c.last_url.clone())
            .unwrap_or_else(|| "about:blank".into())
    }

    pub fn set_bounds_for_window(&self, window: &Window) -> Result<()> {
        for wv in self.slots.values() {
            let _ = wv.set_bounds_for_window(window);
        }
        Ok(())
    }

    fn touch(&mut self, id: u32) {
        self.lru.retain(|x| *x != id);
        self.lru.push_back(id);
    }

    fn hide_all(&mut self) {
        for wv in self.slots.values() {
            let _ = wv.set_visible(false);
        }
    }

    fn evict_if_needed(&mut self, keep: u32) {
        while self.slots.len() > MAX_CACHED {
            let Some(victim) = self.lru.iter().copied().find(|id| *id != keep) else {
                break;
            };
            self.lru.retain(|x| *x != victim);
            if self.slots.remove(&victim).is_some() {
                info!("Evicted cached WebView for tab {}", victim);
            }
        }
    }

    /// Switch to tab_id, creating a WebView if missing (loads restore_url).
    pub fn switch_to(
        &mut self,
        window: &Window,
        blocked: Arc<AtomicU64>,
        tab_id: u32,
        restore_url: &str,
    ) -> Result<()> {
        if self.active == Some(tab_id) && self.slots.contains_key(&tab_id) {
            if let Some(wv) = self.slots.get(&tab_id) {
                let _ = wv.set_visible(true);
            }
            return Ok(());
        }

        self.hide_all();

        if !self.slots.contains_key(&tab_id) {
            let wv = ContentWebView::rebuild_with_proxy(
                window,
                blocked,
                self.socks_port,
                self.privacy.clone(),
                restore_url,
            )?;
            let _ = wv.set_visible(true);
            self.slots.insert(tab_id, wv);
            self.touch(tab_id);
            self.evict_if_needed(tab_id);
        } else {
            self.touch(tab_id);
            if let Some(wv) = self.slots.get_mut(&tab_id) {
                let _ = wv.set_visible(true);
                // If URL drifted (core navigated while suspended), reload
                if wv.last_url != restore_url
                    && (restore_url.starts_with("http://") || restore_url.starts_with("https://"))
                {
                    let _ = wv.load_url(restore_url);
                }
            }
        }
        self.active = Some(tab_id);
        Ok(())
    }

    pub fn close_tab(&mut self, tab_id: u32) {
        self.slots.remove(&tab_id);
        self.lru.retain(|x| *x != tab_id);
        if self.active == Some(tab_id) {
            self.active = self.lru.back().copied();
            if let Some(id) = self.active {
                if let Some(wv) = self.slots.get(&id) {
                    let _ = wv.set_visible(true);
                }
            }
        }
    }

    /// Proxy / privacy change: drop cache and keep one rebuilt active view.
    pub fn rebuild_all(
        &mut self,
        window: &Window,
        blocked: Arc<AtomicU64>,
        socks_port: Option<u16>,
        privacy: PrivacyState,
        restore_url: &str,
        active_tab: u32,
    ) -> Result<()> {
        self.slots.clear();
        self.lru.clear();
        self.socks_port = socks_port;
        self.privacy = privacy.clone();
        let wv = ContentWebView::rebuild_with_proxy(
            window,
            blocked,
            socks_port,
            privacy,
            restore_url,
        )?;
        let _ = wv.set_visible(true);
        self.slots.insert(active_tab, wv);
        self.lru.push_back(active_tab);
        self.active = Some(active_tab);
        Ok(())
    }

    pub fn needs_proxy(&self, want: Option<u16>) -> bool {
        self.socks_port != want
    }

    pub fn load_url_active(&mut self, url: &str) -> Result<()> {
        if let Some(wv) = self.active_mut() {
            wv.load_url(url)?;
        } else {
            warn!("load_url with no active WebView");
        }
        Ok(())
    }

    pub fn load_html_active(&mut self, html: &str) -> Result<()> {
        if let Some(wv) = self.active_mut() {
            wv.load_html(html)?;
        } else {
            warn!("load_html with no active WebView");
        }
        Ok(())
    }
}
