//! Curated seed documents for the owned Horus index (privacy / Tor / open web).
//! No Google, no SafeSearch profiles.

use crate::{NetworkType, ResultBadge, SearchResult};

pub fn seed_documents() -> Vec<SearchResult> {
    vec![
        SearchResult {
            title: "Tor Project".into(),
            url: "https://www.torproject.org/".into(),
            description: "Official Tor Project — anonymity network and Tor Browser.".into(),
            source: NetworkType::SurfaceWeb,
            verified: true,
            badge: ResultBadge::Local,
        },
        SearchResult {
            title: "Ahmia — Search Tor Hidden Services".into(),
            url: "https://ahmia.fi/".into(),
            description: "Search engine for Tor .onion sites (also reachable as onion).".into(),
            source: NetworkType::SurfaceWeb,
            verified: true,
            badge: ResultBadge::Local,
        },
        SearchResult {
            title: "DuckDuckGo Onion".into(),
            url: "https://duckduckgogg42xjoc72x3sjasowoarfbgcmvfimaftt6twagswzczad.onion/".into(),
            description: "Privacy search over Tor (.onion). Not Google.".into(),
            source: NetworkType::Tor,
            verified: true,
            badge: ResultBadge::Onion,
        },
        SearchResult {
            title: "EFF — Electronic Frontier Foundation".into(),
            url: "https://www.eff.org/".into(),
            description: "Digital rights, privacy, free speech.".into(),
            source: NetworkType::SurfaceWeb,
            verified: true,
            badge: ResultBadge::Local,
        },
        SearchResult {
            title: "Privacy Guides".into(),
            url: "https://www.privacyguides.org/".into(),
            description: "Threat-model based privacy recommendations.".into(),
            source: NetworkType::SurfaceWeb,
            verified: true,
            badge: ResultBadge::Local,
        },
        SearchResult {
            title: "SecureDrop Directory".into(),
            url: "https://securedrop.org/".into(),
            description: "Whistleblower submission systems for newsrooms.".into(),
            source: NetworkType::SurfaceWeb,
            verified: true,
            badge: ResultBadge::Local,
        },
        SearchResult {
            title: "Debian Onion Mirror".into(),
            url: "http://2s4yqjx5ul6okpp3f2gaunr2syex5jgbfpfvhxxbbjwnrsvbk5v3qbid.onion/".into(),
            description: "Debian package mirror as Tor onion service.".into(),
            source: NetworkType::Tor,
            verified: true,
            badge: ResultBadge::Onion,
        },
        SearchResult {
            title: "ProPublica Onion".into(),
            url: "http://p53lf57qovyuvwsc6xnrppyply3vtqm7l6pcobkmyqsiofyeznfoenad.onion/".into(),
            description: "Investigative journalism over Tor.".into(),
            source: NetworkType::Tor,
            verified: true,
            badge: ResultBadge::Onion,
        },
        SearchResult {
            title: "Quad9 DNS".into(),
            url: "https://quad9.net/".into(),
            description: "Recursive DNS focused on security — Sentinel default DoH (not Google).".into(),
            source: NetworkType::SurfaceWeb,
            verified: true,
            badge: ResultBadge::Local,
        },
        SearchResult {
            title: "SearXNG".into(),
            url: "https://docs.searxng.org/".into(),
            description: "Self-hostable metasearch — configure SENTINEL_SEARX_URL for your instance.".into(),
            source: NetworkType::SurfaceWeb,
            verified: true,
            badge: ResultBadge::Local,
        },
    ]
}
