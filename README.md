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
| **Updates** | Polls GitHub latest release; chrome **Update** appears only when a newer tag exists |
| **Circumvention** | Bridges persisted; Snowflake/obfs4/lyrebird from PATH or on-demand install |

## Download (v0.0.2+)

Grab platform binaries from [Releases](https://github.com/drmikecrypto/sentinel-browser/releases/tag/v0.0.2).

Windows builds are **unsigned** unless repo secrets `WINDOWS_CERT_PFX` (base64 PFX) and `WINDOWS_CERT_PASSWORD` are set for the Release workflow. Apple notarization is not wired (no Apple certs yet).

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
| `SENTINEL_TOR_BUNDLE_URL` / `SENTINEL_TOR_BUNDLE_VERSION` | Override Tor Expert Bundle used by Install PT helpers |

### Pluggable transports (on demand)

PT helpers are **not** shipped inside the browser binary or release zip. From Connect → **Install PT helpers** (`sentinel://install_pt`), Sentinel downloads the official Tor Expert Bundle for your OS/arch from [archive.torproject.org](https://archive.torproject.org/) and extracts `lyrebird` / `snowflake-client` into `%AppData%/sentinel/pt` (or XDG data dir). Those paths are preferred before `PATH`.

Put bridge lines in the app config dir `bridges.txt`. Without PT binaries, Tor bootstraps direct only.

Downloads: `sentinel://download?url=https://…` saves under `Downloads/SentinelDownloads`.

## License

AGPL-3.0 — see [LICENSE](LICENSE). Trademark notes: [docs/GOVERNANCE.md](docs/GOVERNANCE.md).
