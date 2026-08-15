# Sentinel Roadmap

Honest status after productionize + v0.0.2.

## Shipped

- [x] WebView2 content engine + Direct↔Tor rebuild with URL restore
- [x] arti Tor + SOCKS5; PT status honesty (Snowflake/obfs4 on PATH or app-data install)
- [x] V2Ray only when V2RAY_PATH + real outbound env (SOCKS inbound)
- [x] DoH without Google; DNS-over-Tor in Tor mode
- [x] adblock-rust + WebResourceRequested; shield allowlist table
- [x] Horus: Tantivy + Ahmia + ethical seed crawl
- [x] WebRTC blocked via init script; honest fingerprint status
- [x] Software vault (random key); HSM theater removed from default
- [x] Job Object process limits (Windows)
- [x] Quarantined I2P/WireGuard/PQC-TLS theater from user claims
- [x] ARCHITECTURE.md matches Path 1
- [x] Multi-tab strip + WebView pool (≤3 cached views)
- [x] DHT keyword shards on disk; SOCKS seed crawl when Tor ready
- [x] arti `pt-client` / `bridge-client` + lyrebird/obfs4/snowflake on PATH
- [x] Honest status/downloads/history; CI check+test
- [x] In-app update check — chrome **Update** only when GitHub `releases/latest` is newer
- [x] On-demand PT helpers — Connect → Install PT helpers downloads Tor Expert Bundle into `%AppData%/sentinel/pt` (not vendored in the zip)
- [x] Optional Windows Authenticode in Release CI when `WINDOWS_CERT_PFX` + `WINDOWS_CERT_PASSWORD` secrets exist (otherwise unsigned)
- [x] **v0.0.1 release** — Windows / Linux / macOS ARM binaries  
  https://github.com/drmikecrypto/sentinel-browser/releases/tag/v0.0.1
- [x] **v0.0.2 release** — updates UI, PT on-demand, signing hooks  
  https://github.com/drmikecrypto/sentinel-browser/releases/tag/v0.0.2

## Later (not blocking Path 1)

- [ ] Apple notarization (needs Apple Developer certs — documented only until secrets exist)
- [ ] Full installer packages (MSI / .deb / .dmg) when distribution story expands

## Non-goals

- Tor Browser fingerprint parity on stock WebView2
- On-chain DAO as product feature
- Full I2P / WireGuard until they proxy WebView
