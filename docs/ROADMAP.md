# Sentinel Roadmap

Honest status after productionize pass.

## Shipped

- [x] WebView2 content engine + Direct↔Tor rebuild with URL restore
- [x] arti Tor + SOCKS5; PT status honesty (Snowflake/obfs4 on PATH)
- [x] V2Ray only when V2RAY_PATH + real outbound env (SOCKS inbound)
- [x] DoH without Google; DNS-over-Tor in Tor mode
- [x] adblock-rust + WebResourceRequested; shield allowlist table
- [x] Horus: Tantivy + Ahmia + ethical seed crawl
- [x] WebRTC blocked via init script; honest fingerprint status
- [x] Software vault (random key); HSM theater removed from default
- [x] Job Object process limits (Windows)
- [x] Quarantined I2P/WireGuard/PQC-TLS theater from user claims
- [x] ARCHITECTURE.md matches Path 1

## Next

- [x] Multi-tab strip (one live WebView; URL restore on switch)
- [x] Persist DHT keyword shards to disk (Kad memory + local fallback)
- [x] Seed crawl re-runs over SOCKS once Tor is ready
- [x] arti `pt-client` / `bridge-client`: bridges.txt + PT binaries attached to TorClientConfig
- [x] CI: Windows check/test + release artifact (optional Authenticode via secrets)
- [x] Tab WebView pool (up to 3 cached views; hide/show on switch)

## Later

- [x] lyrebird accepted as obfs4/meek_lite PT binary (alongside obfs4proxy)
- [x] Honest status page (no fake KB/s); real downloads; clear_history
- [ ] Code-signing cert in CI secrets for production releases
- [ ] Package PT binaries with installers (when distribution story exists)

## Non-goals

- Tor Browser fingerprint parity on stock WebView2
- On-chain DAO as product feature
- Full I2P / WireGuard until they proxy WebView
