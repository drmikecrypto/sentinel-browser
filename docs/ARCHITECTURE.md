# Sentinel Browser Architecture (Path 1)

Honest architecture for the shipping AGPL desktop browser.

## Stack

| Layer | Implementation |
|-------|----------------|
| Content engine | **WebView2** via [wry](https://github.com/tauri-apps/wry) (Windows) |
| Chrome UI | **winit** + **wgpu/glyphon** toolbar (tabs, URL, shields, network badge) |
| Core | **Aegis** (`sent-core`) — navigate, tabs, `sentinel://` pages, encrypted SQLite |
| Network | **Vortex** — arti Tor + local SOCKS5, Quad9 DoH / DNS-over-Tor, adblock-rust |
| Search | **Horus** — Tantivy local index + Ahmia over Tor + optional SearXNG |
| Security | Software vault (random `vault.key`), Job Object limits, WebView privacy prefs |

## Request path

```
URL bar → Aegis::navigate
  ├─ sentinel://* → HTML string → WebView load_html
  ├─ http(s) / .onion → WebView load_url (SOCKS if Tor/.onion)
  └─ query text → Horus search → results HTML
```

Subresources hit WebView2 `WebResourceRequested` → AdBlocker.

## Protocols (user-facing)

- **Clearweb** — direct WebView (no SOCKS)
- **Tor** — arti bootstrap + SOCKS; WebView rebuilt with SOCKS5
- **Tor + Snowflake/obfs4** — `bridges.txt` parsed into arti `TorClientConfig` with `pt-client` transports when binaries are on PATH; otherwise direct bootstrap (no fake PT success)

Not user-facing until they proxy WebView for real: I2P, WireGuard, demo V2Ray.

## What this is not

- Not a Chromium/Servo fork
- Not Tor Browser fingerprint parity
- Not an on-chain DAO (governance page is an experimental local ZK demo)
- Not Google search/DNS/SafeSearch

See [ROADMAP.md](ROADMAP.md) for shipped vs next.
