//! Local persistence for Horus DHT index shards.
//!
//! libp2p Kad still uses an in-memory store for routing; Put also writes here so
//! keyword shards survive restart. Get falls back to disk when the network has
//! nothing — never invents empty "success" theater.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

pub struct ShardStore {
    dir: PathBuf,
}

impl ShardStore {
    pub fn open(dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&dir).context("create dht shard dir")?;
        Ok(Self { dir })
    }

    pub fn default_path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("sentinel")
            .join("dht-shards")
    }

    fn path_for(&self, key: &[u8]) -> PathBuf {
        self.dir.join(hex::encode(key))
    }

    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let path = self.path_for(key);
        fs::write(&path, value).with_context(|| format!("write shard {:?}", path))?;
        Ok(())
    }

    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let path = self.path_for(key);
        fs::read(&path).ok()
    }

    /// Load all shards for seeding the in-memory Kad store on boot.
    pub fn load_all(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();
        let entries = match fs::read_dir(&self.dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("dht shard read_dir failed: {}", e);
                return out;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Ok(key) = hex::decode(name) else {
                continue;
            };
            match fs::read(&path) {
                Ok(value) if !value.is_empty() => out.push((key, value)),
                Ok(_) => {}
                Err(e) => warn!("skip shard {}: {}", name, e),
            }
        }
        if !out.is_empty() {
            info!("Loaded {} persisted DHT shards from disk", out.len());
        }
        out
    }
}
