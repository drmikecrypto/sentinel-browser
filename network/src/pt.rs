//! Pluggable-transport detection + arti TorClientConfig bridge wiring.

use std::path::{Path, PathBuf};
use tracing::{info, warn};

use arti_client::config::{BridgeConfigBuilder, CfgPath, TorClientConfig};
use arti_client::config::pt::TransportConfigBuilder;

pub fn which_bin(name: &str) -> Option<PathBuf> {
    if let Some(p) = crate::pt_fetch::installed_bin(name) {
        return Some(p);
    }
    // Also map snowflake-client / lyrebird aliases from install dir
    if name == "obfs4proxy" {
        if let Some(p) = crate::pt_fetch::installed_bin("lyrebird") {
            return Some(p);
        }
    }
    let env_key = format!("{}_PATH", name.replace('-', "_").to_ascii_uppercase());
    if let Ok(p) = std::env::var(&env_key) {
        let path = PathBuf::from(&p);
        if path.exists() {
            return Some(path);
        }
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
            #[cfg(windows)]
            {
                let exe = dir.join(format!("{}.exe", name));
                if exe.exists() {
                    return Some(exe);
                }
            }
        }
    }
    None
}

pub fn snowflake_available() -> bool {
    which_bin("snowflake-client").is_some()
}

/// obfs4proxy or Tor Browser's lyrebird (same PT protocols).
pub fn obfs4_bin() -> Option<PathBuf> {
    which_bin("obfs4proxy").or_else(|| which_bin("lyrebird"))
}

pub fn obfs4_available() -> bool {
    obfs4_bin().is_some()
}

pub fn pt_status_summary() -> String {
    let sf = if snowflake_available() {
        "snowflake-client: found"
    } else {
        "snowflake-client: missing"
    };
    let ob = match obfs4_bin() {
        Some(p) => format!("obfs4/lyrebird: {}", p.display()),
        None => "obfs4/lyrebird: missing".into(),
    };
    let summary = format!("{} · {}", sf, ob);
    info!("PT status: {}", summary);
    summary
}

pub fn persist_bridge_line(config_dir: &Path, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(config_dir)?;
    let path = config_dir.join("bridges.txt");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{}", line)?;
    Ok(())
}

pub fn read_bridge_lines(config_dir: &Path) -> Vec<String> {
    let path = config_dir.join("bridges.txt");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            if l.to_ascii_lowercase().starts_with("bridge ") {
                l.to_string()
            } else {
                format!("Bridge {}", l)
            }
        })
        .collect()
}

fn push_transport(
    builder: &mut arti_client::config::TorClientConfigBuilder,
    protocol: &str,
    binary: &Path,
) -> anyhow::Result<()> {
    let mut transport = TransportConfigBuilder::default();
    transport
        .protocols(vec![protocol.parse()?])
        .path(CfgPath::new(binary.to_string_lossy().into_owned()))
        .run_on_startup(true);
    builder.bridges().transports().push(transport);
    info!("Attached PT transport '{}' → {}", protocol, binary.display());
    Ok(())
}

/// Build TorClientConfig with bridges.txt + PT binaries when present.
/// Returns default config (no bridges) when nothing usable is configured.
pub fn build_tor_client_config(config_dir: &Path) -> TorClientConfig {
    match try_build_tor_client_config(config_dir) {
        Ok(cfg) => cfg,
        Err(e) => {
            warn!("Bridge/PT config failed ({}); using direct Tor bootstrap", e);
            TorClientConfig::default()
        }
    }
}

fn try_build_tor_client_config(config_dir: &Path) -> anyhow::Result<TorClientConfig> {
    let lines = read_bridge_lines(config_dir);
    if lines.is_empty() {
        info!("No bridges.txt entries — direct Tor bootstrap");
        return Ok(TorClientConfig::default());
    }

    let needs_obfs4 = lines.iter().any(|l| {
        let t = l.to_ascii_lowercase();
        t.contains(" obfs4 ") || t.starts_with("bridge obfs4")
    });
    let needs_snowflake = lines.iter().any(|l| {
        let t = l.to_ascii_lowercase();
        t.contains(" snowflake ") || t.starts_with("bridge snowflake")
    });
    let needs_meek = lines.iter().any(|l| {
        let t = l.to_ascii_lowercase();
        t.contains(" meek") || t.contains("meek_lite")
    });

    let obfs4 = obfs4_bin();
    let snowflake = which_bin("snowflake-client");

    if needs_obfs4 && obfs4.is_none() {
        anyhow::bail!("obfs4 bridges configured but obfs4proxy/lyrebird not on PATH");
    }
    if needs_snowflake && snowflake.is_none() {
        anyhow::bail!("snowflake bridges configured but snowflake-client not on PATH");
    }
    if needs_meek && obfs4.is_none() {
        anyhow::bail!("meek bridges configured but obfs4proxy/lyrebird (meek_lite) not on PATH");
    }

    let mut builder = TorClientConfig::builder();
    let mut parsed = 0usize;
    for line in &lines {
        match line.parse::<BridgeConfigBuilder>() {
            Ok(bridge) => {
                builder.bridges().bridges().push(bridge);
                parsed += 1;
            }
            Err(e) => warn!("Skipping invalid bridge line '{}': {}", line, e),
        }
    }
    if parsed == 0 {
        anyhow::bail!("bridges.txt had no parseable bridge lines");
    }

    if let Some(ref bin) = obfs4 {
        if needs_obfs4 {
            push_transport(&mut builder, "obfs4", bin)?;
        }
        if needs_meek {
            push_transport(&mut builder, "meek_lite", bin)?;
        }
    }
    if let Some(ref bin) = snowflake {
        if needs_snowflake {
            push_transport(&mut builder, "snowflake", bin)?;
        }
    }

    info!(
        "Tor config: {} bridge(s) + PT attached (obfs4={} snowflake={})",
        parsed,
        obfs4.is_some() && needs_obfs4,
        snowflake.is_some() && needs_snowflake
    );
    Ok(builder.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn empty_bridges_gives_default_config() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = build_tor_client_config(dir.path());
        let _ = cfg; // builds without panic
    }

    #[test]
    fn read_and_normalize_bridge_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridges.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "# comment").unwrap();
        writeln!(
            f,
            "obfs4 192.0.2.1:443 aabbccddeeff00112233445566778899aabbccdd cert=abc iat-mode=0"
        )
        .unwrap();
        writeln!(
            f,
            "Bridge snowflake 192.0.2.3:1 2B280B23E1107BB62ABFC40DDCC8824814F80A72 url=https://example/"
        )
        .unwrap();
        let lines = read_bridge_lines(dir.path());
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("Bridge "));
        assert!(lines[1].to_ascii_lowercase().contains("snowflake"));
    }

    #[test]
    fn missing_pt_binary_falls_back_to_direct() {
        let dir = tempfile::tempdir().unwrap();
        persist_bridge_line(
            dir.path(),
            "Bridge obfs4 192.0.2.1:443 aabbccddeeff00112233445566778899aabbccdd cert=abc iat-mode=0",
        )
        .unwrap();
        // Without obfs4 on PATH, try_build fails → build_tor_client_config returns default
        let _cfg = build_tor_client_config(dir.path());
    }
}
