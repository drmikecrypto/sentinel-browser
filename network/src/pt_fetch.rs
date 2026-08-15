//! On-demand install of Tor pluggable-transport helpers into app data.
//!
//! Downloads the official Tor Expert Bundle for this OS/arch from archive.torproject.org,
//! extracts `lyrebird` / `snowflake-client` into `%AppData%/sentinel/pt` (or XDG data).
//! Does not vendor binaries in the git repo.

use anyhow::{bail, Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::info;

/// Pinned Tor Browser / expert-bundle train used for PT extraction.
pub const TOR_BUNDLE_VERSION: &str = "14.5.5";

pub fn pt_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sentinel")
        .join("pt")
}

fn expert_bundle_url() -> String {
    if let Ok(u) = std::env::var("SENTINEL_TOR_BUNDLE_URL") {
        if !u.is_empty() {
            return u;
        }
    }
    let ver = std::env::var("SENTINEL_TOR_BUNDLE_VERSION")
        .unwrap_or_else(|_| TOR_BUNDLE_VERSION.to_string());
    let arch = if cfg!(target_os = "windows") {
        format!("tor-expert-bundle-windows-x86_64-{ver}.tar.gz")
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            format!("tor-expert-bundle-macos-aarch64-{ver}.tar.gz")
        } else {
            format!("tor-expert-bundle-macos-x86_64-{ver}.tar.gz")
        }
    } else {
        // linux
        format!("tor-expert-bundle-linux-x86_64-{ver}.tar.gz")
    };
    format!(
        "https://archive.torproject.org/tor-package-archive/torbrowser/{ver}/{arch}"
    )
}

fn want_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["lyrebird.exe", "snowflake-client.exe", "obfs4proxy.exe"]
    }
    #[cfg(not(windows))]
    {
        &["lyrebird", "snowflake-client", "obfs4proxy"]
    }
}

/// Install PT helpers from Tor Expert Bundle into `pt_dir()`.
pub async fn install_pt_helpers() -> Result<String> {
    let dir = pt_dir();
    fs::create_dir_all(&dir).context("create pt dir")?;

    let url = expert_bundle_url();
    info!("Downloading Tor Expert Bundle for PTs: {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .user_agent("SentinelBrowser/pt-fetch")
        .build()?;

    let bytes = client
        .get(&url)
        .send()
        .await
        .context("download expert bundle")?
        .error_for_status()
        .context("expert bundle HTTP error")?
        .bytes()
        .await
        .context("read expert bundle body")?;

    if bytes.len() < 10_000 {
        bail!("Expert bundle too small ({} bytes) — URL may be wrong", bytes.len());
    }

    let tmp = dir.join("bundle.tar.gz");
    {
        let mut f = File::create(&tmp).context("create bundle temp")?;
        f.write_all(&bytes)?;
    }

    let extracted = extract_pts_from_targz(&tmp, &dir)?;
    let _ = fs::remove_file(&tmp);

    if extracted.is_empty() {
        bail!("No lyrebird/snowflake-client found inside expert bundle");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in &extracted {
            let p = dir.join(name);
            if p.exists() {
                let mut perms = fs::metadata(&p)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&p, perms)?;
            }
        }
    }

    // Prefer installed PTs for this process
    if let Some(ly) = extracted.iter().find(|n| n.contains("lyrebird") || n.contains("obfs4")) {
        std::env::set_var("LYREBIRD_PATH", dir.join(ly));
        std::env::set_var("OBFS4PROXY_PATH", dir.join(ly));
    }
    if let Some(sf) = extracted.iter().find(|n| n.contains("snowflake")) {
        std::env::set_var("SNOWFLAKE_CLIENT_PATH", dir.join(sf));
    }

    Ok(format!(
        "Installed into {}: {}",
        dir.display(),
        extracted.join(", ")
    ))
}

fn extract_pts_from_targz(archive: &Path, dest: &Path) -> Result<Vec<String>> {
    let file = File::open(archive).context("open tar.gz")?;
    let dec = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(dec);
    let mut found = Vec::new();
    let wants = want_names();

    for entry in archive.entries().context("tar entries")? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if !wants.iter().any(|w| *w == name) {
            continue;
        }
        let out = dest.join(&name);
        entry.unpack(&out).with_context(|| format!("unpack {}", name))?;
        info!("Extracted PT helper: {}", out.display());
        found.push(name);
    }
    Ok(found)
}

/// Prefer locally installed PT dir before PATH.
pub fn installed_bin(name: &str) -> Option<PathBuf> {
    let dir = pt_dir();
    let candidates = [
        dir.join(name),
        #[cfg(windows)]
        dir.join(format!("{}.exe", name)),
    ];
    for c in candidates {
        if c.exists() {
            return Some(c);
        }
    }
    None
}
