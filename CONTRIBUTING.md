# Contributing to Sentinel

## Development Environment

Sentinel is built with Rust (2021 edition). Windows is the primary target (WebView2).

### Prerequisites

- Rust stable + Cargo
- **WebView2 Runtime** (usually preinstalled on modern Windows)
- Network access for first Tor bootstrap and crate download
- Optional: C++ build tools if you enable V2Ray process integration

### Build

```bash
git clone https://github.com/drmikecrypto/sentinel-browser.git
cd sentinel-browser
cargo build --workspace
cargo run
```

Optional env:

- `SENTINEL_SEARX_URL` — your SearXNG instance (never Google)
- `RUST_LOG=info` — verbose logs

### Module map

| Crate | Role |
|-------|------|
| `core` | Aegis — navigation, tabs, `sentinel://`, vault |
| `ui` | Native chrome (wgpu) + **WebView2** content (wry) |
| `network` | Vortex — Tor/arti, SOCKS5, DoH, adblock |
| `search` | Horus — Tantivy + Ahmia + DHT |
| `security` | Shield policy hooks |
| `governance` | Proposal scaffolding |

### Standards

- `cargo fmt` before committing
- `cargo check --workspace` must pass
- Do not mark roadmap items done unless they ship in code
- Keep AGPL-3.0 headers on new source files
- Do not add Google search/DNS/SafeSearch integrations

### Trademark

Forks under AGPL are welcome; do not brand closed products as “Sentinel”. See [docs/GOVERNANCE.md](docs/GOVERNANCE.md).
