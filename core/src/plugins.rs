//! Plugin discovery stub.
//!
//! WASM (wasmer) is intentionally not linked in default builds — wasmer/cranelift
//! currently breaks Linux release linking (`__rust_probestack`). Re-enable behind
//! a feature when a stable host API is ready.

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

pub struct PluginManager {
    plugin_dir: PathBuf,
}

impl PluginManager {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self { plugin_dir }
    }

    pub fn discover_plugins(&mut self) -> Result<()> {
        info!(
            "Plugin scan at {:?} — WASM host disabled in this build (no wasmer link)",
            self.plugin_dir
        );
        if !self.plugin_dir.exists() {
            let _ = std::fs::create_dir_all(&self.plugin_dir);
        }
        Ok(())
    }
}
