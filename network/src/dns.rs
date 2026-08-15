use anyhow::Result;
use arti_client::TorClient;
use hickory_resolver::Resolver;
use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::name_server::TokioConnectionProvider;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tor_rtcompat::PreferredRuntime;
use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DnsProvider {
    Cloudflare,
    Google,
    Quad9,
    Tor, // DNS over Tor
}

#[derive(Debug, Clone)]
struct CacheEntry {
    ips: Vec<IpAddr>,
    expires: Instant,
}

#[derive(Clone)]
pub struct SecureDns {
    provider: DnsProvider,
    resolver: Option<Resolver<TokioConnectionProvider>>,
    tor_client: Option<TorClient<PreferredRuntime>>,
    cache: Arc<Mutex<HashMap<String, CacheEntry>>>,
}

impl std::fmt::Debug for SecureDns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureDns")
            .field("provider", &self.provider)
            .field("resolver", &"Option<Resolver>")
            .field("tor_client", &"Option<TorClient>")
            .field("cache", &self.cache)
            .finish()
    }
}

impl SecureDns {
    pub async fn new(
        provider: DnsProvider,
        tor_client: Option<TorClient<PreferredRuntime>>,
    ) -> Result<Self> {
        info!("Initializing Secure DNS Resolver: {:?}...", provider);

        let resolver = match provider {
            DnsProvider::Cloudflare => {
                let config = ResolverConfig::cloudflare_https();
                let opts = Self::default_opts();
                Some(Resolver::builder_with_config(config, TokioConnectionProvider::default())
                    .with_options(opts)
                    .build())
            }
            DnsProvider::Google => {
                let config = ResolverConfig::google();
                let opts = Self::default_opts();
                Some(Resolver::builder_with_config(config, TokioConnectionProvider::default())
                    .with_options(opts)
                    .build())
            }
            DnsProvider::Quad9 => {
                let config = ResolverConfig::quad9_https();
                let opts = Self::default_opts();
                Some(Resolver::builder_with_config(config, TokioConnectionProvider::default())
                    .with_options(opts)
                    .build())
            }
            DnsProvider::Tor => None,
        };

        Ok(Self {
            provider,
            resolver,
            tor_client,
            cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn default_opts() -> ResolverOpts {
        let mut opts = ResolverOpts::default();
        opts.timeout = Duration::from_secs(5);
        opts.attempts = 3;
        opts
    }

    pub async fn resolve(&self, domain: &str) -> Result<Vec<IpAddr>> {
        // 1. Check Cache
        {
            let mut cache = self.cache.lock().await;
            if let Some(entry) = cache.get(domain) {
                if entry.expires > Instant::now() {
                    return Ok(entry.ips.clone());
                } else {
                    cache.remove(domain);
                }
            }
        }

        // 2. Handle ENS (.eth) domains
        if domain.ends_with(".eth") {
            info!("ENS domain detected: {}. Resolving via eth.limo gateway...", domain);
            // ENS resolution in production:
            // We append the .limo suffix to the .eth domain. The eth.limo infrastructure
            // handles the ENS resolution and serves the content.
            let eth_limo_domain = format!("{}.limo", domain);
            let ips = self.resolve_standard(&eth_limo_domain).await?;
            return Ok(ips);
        }

        self.resolve_standard(domain).await
    }

    async fn resolve_standard(&self, domain: &str) -> Result<Vec<IpAddr>> {
        info!("Resolving {} via {:?}...", domain, self.provider);

        let ips = match (&self.resolver, &self.tor_client, self.provider) {
            (Some(resolver), _, _) => {
                let response = resolver.lookup_ip(domain).await?;
                response.iter().collect::<Vec<_>>()
            }
            (_, Some(tor), DnsProvider::Tor) => {
                info!("Performing DNS resolution over Tor for {}...", domain);
                // Arti's resolve returns IPs for a hostname over the Tor network
                tor.resolve(domain).await?
            }
            _ => return Err(anyhow::anyhow!("Resolver not available for {:?}", self.provider)),
        };

        // 2. Update Cache (TTL: 1 hour for standard resolution results)
        if !ips.is_empty() {
            let mut cache = self.cache.lock().await;
            cache.insert(
                domain.to_string(),
                CacheEntry {
                    ips: ips.clone(),
                    expires: Instant::now() + Duration::from_secs(3600),
                },
            );
        }

        Ok(ips)
    }
}
