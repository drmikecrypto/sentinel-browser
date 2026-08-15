//! Check GitHub Releases for a newer Sentinel Browser version.

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::{info, warn};

pub const REPO_RELEASES_API: &str =
    "https://api.github.com/repos/drmikecrypto/sentinel-browser/releases/latest";

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub html_url: String,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
}

/// Running app version (from the workspace binary crate via build-time env in core).
pub fn current_version() -> &'static str {
    option_env!("SENTINEL_APP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let t = s.trim().trim_start_matches('v');
    let mut parts = t.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    Some((major, minor, patch))
}

pub fn is_newer(remote_tag: &str, local: &str) -> bool {
    match (parse_semver(remote_tag), parse_semver(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
}

/// Query GitHub for latest non-draft release. Returns None if unavailable or not newer.
pub async fn check_for_update() -> Result<Option<UpdateInfo>> {
    let local = current_version();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(format!("SentinelBrowser/{}", local))
        .build()?;

    let resp = client
        .get(REPO_RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("GitHub releases request failed")?;

    if !resp.status().is_success() {
        anyhow::bail!("GitHub releases HTTP {}", resp.status());
    }

    let rel: GhRelease = resp.json().await.context("parse GitHub release JSON")?;
    if rel.draft || rel.prerelease {
        info!("Latest GitHub release is draft/prerelease — ignoring");
        return Ok(None);
    }

    if !is_newer(&rel.tag_name, local) {
        info!(
            "No update (local={}, remote={})",
            local,
            rel.tag_name.trim_start_matches('v')
        );
        return Ok(None);
    }

    let version = rel.tag_name.trim_start_matches('v').to_string();
    info!("Update available: {} → {}", local, version);
    Ok(Some(UpdateInfo {
        version,
        html_url: rel.html_url,
    }))
}

pub fn spawn_periodic_update_check(
    proxy: winit::event_loop::EventLoopProxy<sent_ui::UiEvent>,
) {
    tokio::spawn(async move {
        // Initial delay so boot UI settles
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
        loop {
            match check_for_update().await {
                Ok(Some(info)) => {
                    let _ = proxy.send_event(sent_ui::UiEvent::UpdateAvailable {
                        version: info.version,
                        html_url: info.html_url,
                    });
                }
                Ok(None) => {}
                Err(e) => warn!("Update check failed: {}", e),
            }
            tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_newer() {
        assert!(is_newer("v0.0.2", "0.0.1"));
        assert!(!is_newer("v0.0.1", "0.0.1"));
        assert!(!is_newer("0.0.1", "0.0.2"));
        assert!(is_newer("1.0.0", "0.9.9"));
    }
}
