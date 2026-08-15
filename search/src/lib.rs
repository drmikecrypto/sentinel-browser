/*
 * Sentinel Search Engine (Horus) - AGPL-3.0 License
 * Copyright (C) 2026 Sentinel DAO
 *
 * Google-free: local Tantivy index + Ahmia (over Tor) + optional user SearXNG.
 * .onion results are first-class and visually distinct in the UI.
 */

use anyhow::{Context, Result};
use arti_client::TorClient;
use futures::StreamExt;
use libp2p::{
    identity,
    kad::{store::MemoryStore, Behaviour as Kademlia, Event as KademliaEvent, QueryResult, Quorum, Record},
    noise,
    swarm::SwarmEvent,
    tcp, yamux, PeerId, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex};
use tor_rtcompat::PreferredRuntime;
use tracing::{info, warn};

mod index;
mod ahmia;
pub mod seeds;
mod crawl;
mod dht_store;

pub use index::TantivyIndex;
pub use crawl::crawl_seeds_into;

#[derive(Debug)]
pub enum Command {
    Put {
        key: Vec<u8>,
        value: Vec<u8>,
        sender: oneshot::Sender<Result<()>>,
    },
    Get {
        key: Vec<u8>,
        sender: oneshot::Sender<Result<Vec<u8>>>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResultBadge {
    Clearnet,
    Onion,
    Ipfs,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub description: String,
    pub source: NetworkType,
    pub verified: bool,
    #[serde(default = "default_badge")]
    pub badge: ResultBadge,
}

fn default_badge() -> ResultBadge {
    ResultBadge::Clearnet
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkType {
    SurfaceWeb,
    Tor,
    I2P,
    Blockchain(String),
}

pub struct HorusEngine {
    local_index: Arc<Mutex<TantivyIndex>>,
    index_path: PathBuf,
    tor_client: Option<TorClient<PreferredRuntime>>,
    indexer: Arc<dyn Indexer + Send + Sync>,
    /// Optional user-hosted SearXNG base URL (never Google).
    searx_url: Option<String>,
    socks_port: Option<u16>,
}

impl HorusEngine {
    pub fn new(
        indexer: Arc<dyn Indexer + Send + Sync>,
        tor_client: Option<TorClient<PreferredRuntime>>,
    ) -> Self {
        let index_path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("sentinel")
            .join("horus-index");
        let local_index = TantivyIndex::open_or_create(&index_path)
            .unwrap_or_else(|e| {
                warn!("Tantivy open failed ({}), using temp index", e);
                TantivyIndex::open_or_create(&std::env::temp_dir().join("sentinel-horus"))
                    .expect("temp index")
            });

        // Seed curated documents once
        if let Ok(n) = local_index.len() {
            if n < 5 {
                let _ = local_index.bulk_index(&seeds::seed_documents());
            }
        }

        // Background ethical crawl (non-blocking)
        {
            let idx_path = index_path.clone();
            let socks = None; // filled later via set_socks + restart crawl from Aegis if needed
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                if let Ok(idx) = TantivyIndex::open_or_create(&idx_path) {
                    match crawl::crawl_seeds_into(&idx, socks).await {
                        Ok(n) => info!("Seed crawl indexed {} pages", n),
                        Err(e) => warn!("Seed crawl error: {}", e),
                    }
                }
            });
        }

        Self {
            local_index: Arc::new(Mutex::new(local_index)),
            index_path,
            tor_client,
            indexer,
            searx_url: std::env::var("SENTINEL_SEARX_URL").ok(),
            socks_port: None,
        }
    }

    pub fn set_socks_port(&mut self, port: Option<u16>) {
        self.socks_port = port;
    }

    /// Re-crawl seed URLs (including .onion) once Tor SOCKS is up.
    pub fn recrawl_with_socks(&self) {
        let Some(port) = self.socks_port else {
            return;
        };
        let idx_path = self.index_path.clone();
        tokio::spawn(async move {
            info!("Starting SOCKS seed crawl via 127.0.0.1:{}", port);
            match TantivyIndex::open_or_create(&idx_path) {
                Ok(idx) => match crawl::crawl_seeds_into(&idx, Some(port)).await {
                    Ok(n) => info!("SOCKS seed crawl indexed {} pages", n),
                    Err(e) => warn!("SOCKS seed crawl error: {}", e),
                },
                Err(e) => warn!("SOCKS crawl index open failed: {}", e),
            }
        });
    }

    pub fn set_searx_url(&mut self, url: Option<String>) {
        self.searx_url = url;
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        info!("Horus universal search (Google-free): '{}'", query);
        let mut all = Vec::new();

        // 1. Local owned index
        {
            let idx = self.local_index.lock().await;
            match idx.search(query, 20) {
                Ok(mut local) => all.append(&mut local),
                Err(e) => warn!("Local index search failed: {}", e),
            }
        }

        // 2. Ahmia onion directory over Tor
        match ahmia::search_ahmia(query, self.tor_client.as_ref(), self.socks_port).await {
            Ok(mut onions) => all.append(&mut onions),
            Err(e) => warn!("Ahmia search unavailable: {}", e),
        }

        // 3. Optional user SearXNG (never Google)
        if let Some(ref base) = self.searx_url {
            match search_searx(base, query).await {
                Ok(mut r) => all.append(&mut r),
                Err(e) => warn!("SearXNG failed: {}", e),
            }
        }

        // 4. DHT (only merge when non-empty — no empty theater)
        if let Ok(Ok(p2p)) =
            tokio::time::timeout(Duration::from_millis(600), self.indexer.search(query)).await
        {
            if !p2p.is_empty() {
                all.extend(p2p);
            }
        }

        // Rank: onions after a few clearnet hits but interleaved with badge; boost verified onion
        all.sort_by(|a, b| {
            let score = |r: &SearchResult| {
                let mut s = 0;
                if r.verified {
                    s += 2;
                }
                if r.badge == ResultBadge::Onion {
                    s += 1;
                }
                if r.badge == ResultBadge::Local {
                    s += 3;
                }
                s
            };
            score(b).cmp(&score(a)).then_with(|| a.title.cmp(&b.title))
        });
        all.dedup_by(|a, b| a.url == b.url);

        // Cap
        all.truncate(40);
        Ok(all)
    }

    pub async fn index_page(&self, result: SearchResult) -> Result<()> {
        let idx = self.local_index.lock().await;
        idx.add_document(&result)?;
        self.indexer.index(result).await?;
        Ok(())
    }
}

async fn search_searx(base: &str, query: &str) -> Result<Vec<SearchResult>> {
    let url = format!(
        "{}/search?q={}&format=json",
        base.trim_end_matches('/'),
        urlencoding::encode(query)
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("SentinelHorus/0.1")
        .build()?;
    let resp: serde_json::Value = client.get(&url).send().await?.json().await?;
    let mut out = Vec::new();
    if let Some(arr) = resp.get("results").and_then(|v| v.as_array()) {
        for item in arr.iter().take(15) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string();
            let link = item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if link.is_empty() {
                continue;
            }
            let description = item
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_onion = link.contains(".onion");
            out.push(SearchResult {
                title,
                url: link,
                description,
                source: if is_onion {
                    NetworkType::Tor
                } else {
                    NetworkType::SurfaceWeb
                },
                verified: false,
                badge: if is_onion {
                    ResultBadge::Onion
                } else {
                    ResultBadge::Clearnet
                },
            });
        }
    }
    Ok(out)
}

#[async_trait::async_trait]
pub trait Indexer: Send + Sync {
    async fn index(&self, data: SearchResult) -> Result<()>;
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>>;
}

pub struct BlockchainIndexer {
    dht_handle: mpsc::Sender<Command>,
}

impl BlockchainIndexer {
    pub fn new(dht_handle: mpsc::Sender<Command>) -> Self {
        Self { dht_handle }
    }
}

#[async_trait::async_trait]
impl Indexer for BlockchainIndexer {
    async fn index(&self, data: SearchResult) -> Result<()> {
        let key = data.url.as_bytes().to_vec();
        let value = serde_json::to_vec(&data)?;
        let (tx, rx) = oneshot::channel();
        self.dht_handle
            .send(Command::Put {
                key,
                value,
                sender: tx,
            })
            .await?;
        rx.await?
    }

    async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(query.to_lowercase().trim().as_bytes());
        let keyword_hash = hasher.finalize().as_bytes().to_vec();
        let (tx, rx) = oneshot::channel();
        self.dht_handle
            .send(Command::Get {
                key: keyword_hash,
                sender: tx,
            })
            .await?;
        match rx.await? {
            Ok(value) => {
                let results: Vec<SearchResult> = serde_json::from_slice(&value)
                    .context("Failed to deserialize DHT shard")?;
                Ok(results)
            }
            Err(_) => Ok(vec![]),
        }
    }
}

pub async fn start_dht() -> Result<mpsc::Sender<Command>> {
    let (command_tx, mut command_rx) = mpsc::channel(100);
    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());
    let shard_store = dht_store::ShardStore::open(dht_store::ShardStore::default_path())?;
    let persisted = shard_store.load_all();
    let mut swarm = SwarmBuilder::with_existing_identity(id_keys)
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| {
            Kademlia::new(peer_id, MemoryStore::new(key.public().to_peer_id()))
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Seed in-memory Kad from disk (local truth; network may still be empty)
    for (key, value) in persisted {
        let record = Record {
            key: key.into(),
            value,
            publisher: None,
            expires: None,
        };
        if let Err(e) = swarm.behaviour_mut().put_record(record, Quorum::One) {
            warn!("Failed to rehydrate DHT shard: {:?}", e);
        }
    }

    // Real libp2p bootstrap (IPFS) — drop fake sentinel peer IDs
    let bootstrap_nodes = [
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmNnooEu7zsDDpR6UkMnAQTFL1tqE4jjWqfFgm4b7rcwx",
        "/dnsaddr/bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    ];
    for addr_str in bootstrap_nodes {
        if let Ok(addr) = addr_str.parse::<libp2p::Multiaddr>() {
            if let Some(pid) = addr.iter().find_map(|p| {
                if let libp2p::multiaddr::Protocol::P2p(peer_id) = p {
                    Some(peer_id)
                } else {
                    None
                }
            }) {
                swarm.behaviour_mut().add_address(&pid, addr);
            }
        }
    }
    let _ = swarm.behaviour_mut().bootstrap();

    let mut pending_queries: std::collections::HashMap<
        libp2p::kad::QueryId,
        oneshot::Sender<Result<Vec<u8>>>,
    > = std::collections::HashMap::new();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                event = swarm.select_next_some() => {
                    if let SwarmEvent::Behaviour(KademliaEvent::OutboundQueryProgressed { id, result, .. }) = event {
                        if let Some(sender) = pending_queries.remove(&id) {
                            match result {
                                QueryResult::GetRecord(Ok(ok)) => {
                                    if let libp2p::kad::GetRecordOk::FoundRecord(peer_record) = ok {
                                        let _ = sender.send(Ok(peer_record.record.value));
                                    }
                                }
                                QueryResult::GetRecord(Err(e)) => {
                                    let _ = sender.send(Err(anyhow::anyhow!("{:?}", e)));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                cmd = command_rx.recv() => if let Some(cmd) = cmd {
                    match cmd {
                        Command::Put { key, value, sender } => {
                            if let Err(e) = shard_store.put(&key, &value) {
                                warn!("DHT shard disk write failed: {}", e);
                            }
                            let record = Record {
                                key: key.into(),
                                value,
                                publisher: None,
                                expires: None,
                            };
                            match swarm.behaviour_mut().put_record(record, Quorum::One) {
                                Ok(_) => { let _ = sender.send(Ok(())); }
                                Err(e) => { let _ = sender.send(Err(anyhow::anyhow!(e))); }
                            }
                        }
                        Command::Get { key, sender } => {
                            // Prefer local disk if present (honest offline shard), else Kad
                            if let Some(local) = shard_store.get(&key) {
                                let _ = sender.send(Ok(local));
                            } else {
                                let query_id = swarm.behaviour_mut().get_record(key.into());
                                pending_queries.insert(query_id, sender);
                            }
                        }
                    }
                }
            }
        }
    });
    Ok(command_tx)
}

#[cfg(test)]
mod tests;
