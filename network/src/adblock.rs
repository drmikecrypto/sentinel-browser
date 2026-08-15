/*
 * Sentinel AdBlocker (Brave-class filter engine) - AGPL-3.0
 */

use adblock::engine::Engine;
use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tracing::info;

/// Built-in EasyList / EasyPrivacy subset shipped with the browser (no Google SafeSearch, no phone-home).
const BUILTIN_FILTERS: &str = include_str!("../filters/sentinel-easylist.txt");

pub struct AdBlocker {
    engine: Mutex<Engine>,
    blocked: AtomicU64,
    whitelist: Vec<String>,
}

impl Default for AdBlocker {
    fn default() -> Self {
        Self::with_builtin_lists()
    }
}

impl AdBlocker {
    pub fn with_builtin_lists() -> Self {
        let mut filter_set = FilterSet::new(false);
        filter_set.add_filter_list(BUILTIN_FILTERS.to_string(), ParseOptions::default());
        let engine = Engine::new_with_filter_set(filter_set);
        info!(
            "AdBlocker loaded {} filter lines",
            BUILTIN_FILTERS
                .lines()
                .filter(|l| !l.is_empty() && !l.starts_with('!'))
                .count()
        );
        Self {
            engine: Mutex::new(engine),
            blocked: AtomicU64::new(0),
            whitelist: vec![
                "sentinel.dao".into(),
                "localhost".into(),
                "127.0.0.1".into(),
            ],
        }
    }

    pub fn blocked_count(&self) -> u64 {
        self.blocked.load(Ordering::Relaxed)
    }

    pub fn reset_count(&self) {
        self.blocked.store(0, Ordering::Relaxed);
    }

    pub fn is_blocked(&self, url: &str) -> bool {
        for domain in &self.whitelist {
            if url.contains(domain) {
                return false;
            }
        }
        // Dynamic allowlist hosts (from SQLite via caller sync)
        // Additional hosts can be injected via add_whitelist_host

        let lower = url.to_lowercase();
        const HARD_BLOCK: &[&str] = &[
            "doubleclick.net",
            "google-analytics.com",
            "googletagmanager.com",
            "googlesyndication.com",
            "googleadservices.com",
            "facebook.com/tr",
            "connect.facebook.net",
            "scorecardresearch.com",
            "adservice.google",
            "pagead2.googlesyndication",
            "hotjar.com",
            "clarity.ms",
        ];
        for d in HARD_BLOCK {
            if lower.contains(d) {
                self.blocked.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        }

        let source = "https://sentinel.local/";
        if let Ok(req) = Request::new(url, source, "script", "get") {
            if let Ok(engine) = self.engine.lock() {
                let result = engine.check_network_request(&req);
                if result.filter.is_some() && result.exception.is_none() {
                    self.blocked.fetch_add(1, Ordering::Relaxed);
                    return true;
                }
            }
        }
        false
    }

    pub fn add_whitelist_host(&mut self, host: String) {
        if !self.whitelist.contains(&host) {
            self.whitelist.push(host);
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FilterAction {
    Block,
    Allow,
    Redirect(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilterRule {
    pub pattern: String,
    pub action: FilterAction,
    #[serde(skip)]
    pub regex: Option<regex::Regex>,
}
