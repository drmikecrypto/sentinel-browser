//! Ethical rate-limited seed crawl into Tantivy.

use anyhow::Result;
use scraper::{Html, Selector};
use std::time::Duration;
use tracing::{info, warn};

use crate::seeds::seed_documents;
use crate::{NetworkType, ResultBadge, SearchResult, TantivyIndex};

/// Fetch seed URLs (clearnet via reqwest; onion skipped unless SOCKS provided) and index.
pub async fn crawl_seeds_into(index: &TantivyIndex, socks_port: Option<u16>) -> Result<usize> {
    let mut added = 0usize;
    let client = if let Some(port) = socks_port {
        reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("socks5h://127.0.0.1:{}", port))?)
            .timeout(Duration::from_secs(30))
            .user_agent("SentinelHorusCrawl/0.1 (+ethical; contact: none)")
            .build()?
    } else {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("SentinelHorusCrawl/0.1 (+ethical; contact: none)")
            .build()?
    };

    for seed in seed_documents() {
        let is_onion = seed.url.contains(".onion");
        if is_onion && socks_port.is_none() {
            info!("Skip onion seed until SOCKS ready: {}", seed.url);
            continue;
        }
        // Rate limit: 2s between requests
        tokio::time::sleep(Duration::from_secs(2)).await;
        match client.get(&seed.url).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    warn!("Crawl HTTP {} for {}", resp.status(), seed.url);
                    continue;
                }
                let body = resp.text().await.unwrap_or_default();
                let doc = Html::parse_document(&body);
                let title = Selector::parse("title")
                    .ok()
                    .and_then(|sel| doc.select(&sel).next())
                    .map(|el| el.text().collect::<String>())
                    .unwrap_or_else(|| seed.title.clone());
                let desc = Selector::parse("meta[name=description]")
                    .ok()
                    .and_then(|sel| doc.select(&sel).next())
                    .and_then(|el| el.value().attr("content").map(|s| s.to_string()))
                    .unwrap_or_else(|| seed.description.clone());
                let result = SearchResult {
                    title: title.trim().to_string(),
                    url: seed.url.clone(),
                    description: desc.trim().to_string(),
                    source: if is_onion {
                        NetworkType::Tor
                    } else {
                        NetworkType::SurfaceWeb
                    },
                    verified: true,
                    badge: if is_onion {
                        ResultBadge::Onion
                    } else {
                        ResultBadge::Local
                    },
                };
                if let Err(e) = index.add_document(&result) {
                    warn!("Index add failed: {}", e);
                } else {
                    added += 1;
                    info!("Indexed: {}", result.url);
                }
            }
            Err(e) => warn!("Crawl failed {}: {}", seed.url, e),
        }
    }
    Ok(added)
}
