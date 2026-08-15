/*
 * Ahmia.fi onion search — queried over Tor (never clearnet Google).
 */

use anyhow::{Context, Result};
use arti_client::{TorAddr, TorClient};
use std::time::Duration;
use tor_rtcompat::PreferredRuntime;
use tracing::info;

use crate::{NetworkType, ResultBadge, SearchResult};

/// Ahmia clearnet front that redirects; prefer onion when Tor is up.
const AHMIA_ONION: &str = "juhanurmihxlp77nkq76byazcldy2hlmovfu2epvl5ankdibsot4csyd.onion";
const AHMIA_CLEAR: &str = "https://ahmia.fi/search/";

pub async fn search_ahmia(
    query: &str,
    tor: Option<&TorClient<PreferredRuntime>>,
    socks_port: Option<u16>,
) -> Result<Vec<SearchResult>> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    // Prefer SOCKS (WebView/shared) then raw arti stream
    if let Some(port) = socks_port {
        if let Ok(results) = fetch_via_socks(query, port).await {
            if !results.is_empty() {
                return Ok(results);
            }
        }
    }

    if let Some(client) = tor {
        if let Ok(results) = fetch_via_arti(query, client).await {
            return Ok(results);
        }
    }

    // Last resort: ahmia.fi over clearnet (still not Google; may be blocked in censored regions)
    fetch_ahmia_html(
        &format!("{}?q={}", AHMIA_CLEAR, urlencoding::encode(query)),
        false,
    )
    .await
}

async fn fetch_via_socks(query: &str, port: u16) -> Result<Vec<SearchResult>> {
    let proxy = format!("socks5h://127.0.0.1:{}", port);
    let client = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(&proxy)?)
        .timeout(Duration::from_secs(45))
        .user_agent("SentinelHorus/0.1")
        .build()?;
    let url = format!(
        "http://{}/search/?q={}",
        AHMIA_ONION,
        urlencoding::encode(query)
    );
    info!("Ahmia via SOCKS: {}", url);
    let body = client.get(&url).send().await?.text().await?;
    parse_ahmia_html(&body)
}

async fn fetch_via_arti(query: &str, client: &TorClient<PreferredRuntime>) -> Result<Vec<SearchResult>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let host = AHMIA_ONION;
    let path = format!("/search/?q={}", urlencoding::encode(query));
    let target = TorAddr::from((host, 80)).map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut stream = client.connect(target).await.context("ahmia onion connect")?;
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: SentinelHorus/0.1\r\n\r\n",
        path, host
    );
    stream.write_all(req.as_bytes()).await?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let body_start = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(0);
    let body = String::from_utf8_lossy(&buf[body_start..]);
    parse_ahmia_html(&body)
}

async fn fetch_ahmia_html(url: &str, _onion: bool) -> Result<Vec<SearchResult>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("SentinelHorus/0.1")
        .build()?;
    let body = client.get(url).send().await?.text().await?;
    parse_ahmia_html(&body)
}

fn parse_ahmia_html(body: &str) -> Result<Vec<SearchResult>> {
    let mut out = Vec::new();
    // Ahmia result links typically look like <a href="http://....onion">Title</a>
    let re = regex::Regex::new(
        r#"<a[^>]+href=["'](https?://[a-z2-7]{16,56}\.onion[^"']*)["'][^>]*>([^<]{1,200})</a>"#,
    )?;
    for caps in re.captures_iter(body) {
        let url = html_escape::decode_html_entities(caps.get(1).unwrap().as_str()).to_string();
        let title = html_escape::decode_html_entities(caps.get(2).unwrap().as_str())
            .trim()
            .to_string();
        if title.is_empty() {
            continue;
        }
        out.push(SearchResult {
            title,
            url,
            description: "Indexed via Ahmia onion directory (Tor).".into(),
            source: NetworkType::Tor,
            verified: true,
            badge: ResultBadge::Onion,
        });
        if out.len() >= 15 {
            break;
        }
    }
    // Alternate: result__url style
    if out.is_empty() {
        let re2 = regex::Regex::new(r#"(https?://[a-z2-7]{16,56}\.onion[^\s<"']*)"#)?;
        for caps in re2.captures_iter(body) {
            let url = caps.get(1).unwrap().as_str().to_string();
            out.push(SearchResult {
                title: url.clone(),
                url,
                description: "Onion service discovered via Ahmia.".into(),
                source: NetworkType::Tor,
                verified: true,
                badge: ResultBadge::Onion,
            });
            if out.len() >= 10 {
                break;
            }
        }
    }
    Ok(out)
}
