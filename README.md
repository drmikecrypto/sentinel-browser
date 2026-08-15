# Sentinel Browser

> **AGPL-3.0 community privacy browser** — clearnet + Tor, Google-free search, ads/trackers blocked by default.

**Maintainer:** [drmikecrypto](https://github.com/drmikecrypto)  
**Repo:** https://github.com/drmikecrypto/sentinel-browser  
**Releases:** https://github.com/drmikecrypto/sentinel-browser/releases

## What works (production-oriented Path 1)

| Area | Behavior |
|------|----------|
| **Pages** | WebView under native chrome (WebView2 on Windows; WebKit elsewhere) |
| **Tor / Direct** | arti SOCKS; WebView **rebuilds** when switching modes (URL restored) |
| **`.onion`** | SOCKS path; searchable via Ahmia |
| **Search** | Tantivy + seed crawl + Ahmia; optional `SENTINEL_SEARX_URL` — never Google |
| **Shields** | adblock-rust + EasyList subset; per-host allowlist via `sentinel://allow_site?host=` |
| **DNS** | Quad9 DoH / DNS-over-Tor; Google DNS rejected |
| **Privacy** | WebRTC blocked in-page; fingerprint **not** claimed as Tor Browser |
| **Sandbox** | Windows Job Object (kill-on-close + memory limit) |
| **Circumvention** | Bridges persisted; Snowflake/obfs4/lyrebird only if PT binaries on PATH |

## Download (v0.0.1+)

Grab platform binaries from [Releases](https://github.com/drmikecrypto/sentinel-browser/releases).

## Not user-facing until real

I2P, WireGuard, demo V2Ray (unless `V2RAY_PATH` + `SENTINEL_V2RAY_HOST`/`PORT`), on-chain DAO, HSM/PKCS#11.

## Build from source

```bash
git clone https://github.com/drmikecrypto/sentinel-browser.git
cd sentinel-browser
cargo build --release
cargo run --release
```

Optional env:

| Variable | Purpose |
|----------|---------|
| `SENTINEL_SEARX_URL` | Your SearXNG base URL (never Google) |
| `V2RAY_PATH` + `SENTINEL_V2RAY_HOST` / `PORT` | Real V2Ray SOCKS outbound |
| `SNOWFLAKE_CLIENT_PATH` / `OBFS4PROXY_PATH` / `LYREBIRD_PATH` | Override PT binary paths |

Put bridge lines in the app config dir `bridges.txt`. With `obfs4proxy`/`lyrebird` or `snowflake-client` on `PATH`, arti attaches those transports; without them Tor bootstraps direct only.

Downloads: `sentinel://download?url=https://…` saves under `Downloads/SentinelDownloads`.

## License

AGPL-3.0 — see [LICENSE](LICENSE). Trademark notes: [docs/GOVERNANCE.md](docs/GOVERNANCE.md).
