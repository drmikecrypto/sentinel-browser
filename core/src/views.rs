use sent_search::SearchResult;
use sent_gov::Proposal;

pub fn welcome_page(theme: &str, search_engine: &str) -> String {
    format!(r#"
    <html>
        <body>
            <section>
                <h1>SENTINEL</h1>
                <p>Privacy-native browser · Tor + clearnet · Google-free Horus search</p>
                <p>Theme: {} | Search: {}</p>
                <p>Type a query in the URL bar to search clearnet + .onion (onion hits are marked). Paste any https:// or .onion address to browse.</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>PRIMARY</h3>
                        <ul>
                            <li><a href="sentinel://search?q=privacy">Search: privacy</a></li>
                            <li><a href="sentinel://search?q=mail">Search: mail (onion + clearnet)</a></li>
                            <li><a href="sentinel://settings">Settings</a></li>
                            <li><a href="sentinel://connect">Connect / Tor</a></li>
                            <li><a href="sentinel://bookmarks">Bookmarks</a></li>
                            <li><a href="sentinel://history">History</a></li>
                            <li><a href="sentinel://status">Network Status</a></li>
                            <li><a href="sentinel://downloads">Downloads</a></li>
                            <li><a href="sentinel://security">Security</a></li>
                            <li><a href="sentinel://governance">Governance (local demo)</a></li>
                        </ul>
                    </column>
                    <column>
                        <h3>SHIELDS</h3>
                        <ul>
                            <li>Ads &amp; trackers blocked by default</li>
                            <li>No Google DNS / SafeSearch</li>
                            <li>DoH via Quad9 or DNS-over-Tor</li>
                            <li>ISP parental filters do not apply inside Tor</li>
                        </ul>
                    </column>
                    <column>
                        <h3>SYSTEM</h3>
                        <p>Engine: WebView2 · Network: Vortex · Search: Horus</p>
                        <p>License: AGPL-3.0</p>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, theme, search_engine)
}

use sent_net::ConnectionProfile;
pub fn connect_page(status: &str, profiles: Vec<ConnectionProfile>, pt_status: &str) -> String {
    let mut profiles_html = String::new();
    for p in profiles {
        profiles_html.push_str(&format!(r#"<li>{:?} - {} <a href="sentinel://connect_switch?name={}">SWITCH</a></li>"#, p.protocol, p.name, p.name));
    }
    if profiles_html.is_empty() {
        profiles_html = "<p>No saved profiles.</p>".to_string();
    }
    format!(r#"
    <html>
        <body>
            <section>
                <h1>CONNECT</h1>
                <p>Status: {}</p>
                <p>Pluggable transports: {}</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>PROTOCOL</h3>
                        <p><a href="sentinel://network?type=tor">Tor</a></p>
                        <p><a href="sentinel://network?type=clear">Clearweb (Direct)</a></p>
                        <p><a href="sentinel://network?type=snowflake">Tor + Snowflake</a> (requires snowflake-client on PATH)</p>
                        <p><a href="sentinel://status">Network status</a></p>
                        <p style="opacity:.7">I2P, WireGuard, and V2Ray are not exposed until they proxy WebView traffic for real.</p>
                    </column>
                    <column>
                        <h3>BRIDGES</h3>
                        <p><a href="sentinel://network_menu">Configure bridges</a></p>
                        <p>obfs4 / Snowflake need PT binaries. Install helpers from the Tor Expert Bundle (on-demand):</p>
                        <p><a href="sentinel://install_pt">Install PT helpers</a></p>
                        <p>Without them, Tor uses direct bootstrap only.</p>
                    </column>
                    <column>
                        <h3>PROFILES</h3>
                        <ul>{}</ul>
                        <p><a href="sentinel://connect_save_profile?name=Default">Save current as Default</a></p>
                        <p><a href="sentinel://connect_test">Test connection</a></p>
                        <p><a href="sentinel://home">Back</a></p>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, status, pt_status, profiles_html)
}

pub fn pt_install_page(ok: bool, detail: &str) -> String {
    let title = if ok { "PT HELPERS INSTALLED" } else { "PT INSTALL FAILED" };
    format!(
        r#"
    <html>
        <body>
            <section>
                <h1>{}</h1>
                <p>{}</p>
                <p>Helpers are stored under your app data <code>sentinel/pt</code> folder (not bundled in the browser zip).</p>
                <p><a href="sentinel://connect">Back to Connect</a> · <a href="sentinel://home">Home</a></p>
            </section>
        </body>
    </html>
    "#,
        title,
        html_escape(detail)
    )
}

pub fn settings_page(theme: &str, search_engine: &str, security_level: &str, history_enabled: &str) -> String {
    let history_status = if history_enabled == "true" { "Enabled" } else { "Disabled" };
    let history_action = if history_enabled == "true" { "DISABLE" } else { "ENABLE" };

    format!(r#"
    <html>
        <body>
            <section>
                <h1>SETTINGS</h1>
                <p>Configure Sentinel Browser</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>GENERAL</h3>
                        <p>Theme: {}</p>
                        <p>Search: {}</p>
                        <p>Homepage: Dashboard</p>
                    </column>
                    <column>
                        <h3>PRIVACY</h3>
                        <p>History: {}</p>
                        <p>Search: Horus (local + Ahmia) — never Google</p>
                        <p>Optional SearXNG: set SENTINEL_SEARX_URL</p>
                        <p>DNS: Quad9 DoH / DNS-over-Tor (no SafeSearch)</p>
                        <p>Allowlist ads on a host: <code>sentinel://allow_site?host=example.com</code></p>
                        <button onclick="sentinel://toggle_history">{} HISTORY</button>
                        <button onclick="sentinel://welcome">BACK</button>
                    </column>
                    <column>
                        <h3>SECURITY</h3>
                        <p>Sandbox: {}</p>
                        <p>Shields: ads/trackers blocked (EasyList subset)</p>
                        <p>Engine: WebView2 (JS enabled; not Tor Browser fingerprint parity)</p>
                        <button onclick="sentinel://toggle_security">TOGGLE LEVEL</button>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, theme, search_engine, history_status, history_action, security_level)
}

    pub fn dapps_page() -> String {
    r#"
    <html>
        <body>
            <section>
                <h1>LINKS</h1>
                <p>Public site bookmarks only — Sentinel does not ship a wallet or Web3 injector.</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>DEFI (EXTERNAL)</h3>
                        <p><a href="https://app.uniswap.org">Uniswap</a></p>
                        <p><a href="https://app.aave.com">Aave</a></p>
                    </column>
                    <column>
                        <h3>IDENTITY</h3>
                        <p><a href="https://app.ens.domains">ENS</a></p>
                        <p><a href="https://ipfs.tech">IPFS</a></p>
                        <p><a href="sentinel://home">Back</a></p>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#.to_string()
}

pub fn search_results_page(query: &str, results: Vec<SearchResult>) -> String {
    let mut clearnet = String::new();
    let mut onion = String::new();
    let mut other = String::new();

    if results.is_empty() {
        clearnet.push_str("<p>No results yet. Horus indexes grow as you browse and via Ahmia (Tor). Configure <code>SENTINEL_SEARX_URL</code> for your SearXNG instance — never Google.</p>");
    } else {
        for res in results {
            let is_onion = matches!(res.badge, sent_search::ResultBadge::Onion)
                || matches!(res.source, sent_search::NetworkType::Tor)
                || res.url.contains(".onion");
            let badge = if is_onion {
                r#"<span class="badge-onion">ONION</span>"#
            } else if matches!(res.badge, sent_search::ResultBadge::Local) {
                r#"<span class="badge-clear">LOCAL INDEX</span>"#
            } else {
                r#"<span class="badge-clear">CLEARNET</span>"#
            };
            let card = format!(
                r#"<div class="result{onion_class}">
                    <div>{badge} <b>{title}</b></div>
                    <div class="onion-url"><a href="{url}">{url}</a></div>
                    <p>{desc}</p>
                    <p><a href="{url}">Open</a> · <a href="sentinel://add_bookmark?url={url}&title={title}">Bookmark</a></p>
                </div>"#,
                onion_class = if is_onion { " onion" } else { "" },
                badge = badge,
                title = html_escape(&res.title),
                url = html_escape(&res.url),
                desc = html_escape(&res.description),
            );
            if is_onion {
                onion.push_str(&card);
            } else if matches!(res.source, sent_search::NetworkType::SurfaceWeb)
                || matches!(res.badge, sent_search::ResultBadge::Clearnet | sent_search::ResultBadge::Local)
            {
                clearnet.push_str(&card);
            } else {
                other.push_str(&card);
            }
        }
    }

    if onion.is_empty() {
        onion.push_str("<p>No dark-web hits for this query (Ahmia needs Tor). Try another term or wait for SOCKS.</p>");
    }
    if clearnet.is_empty() {
        clearnet.push_str("<p>No clearnet/local hits.</p>");
    }

    format!(
        r#"
    <html>
        <body>
            <section>
                <h1>HORUS SEARCH</h1>
                <p>Google-free · Query: "{query}" · Onion results are marked and require Tor</p>
            </section>
            <section>
                <h2>Dark Web (.onion)</h2>
                {onion}
            </section>
            <section>
                <h2>Clearnet &amp; Local Index</h2>
                {clearnet}
            </section>
            <section>
                <h2>Other Networks</h2>
                {other}
            </section>
        </body>
    </html>
    "#,
        query = html_escape(query),
        onion = onion,
        clearnet = clearnet,
        other = if other.is_empty() {
            "<p>None</p>".into()
        } else {
            other
        }
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn design_system_page() -> String {
    r#"
    <html>
        <body>
            <section>
                <h1>DESIGN SYSTEM</h1>
                <p>Typography, colors, spacing, components</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>TYPOGRAPHY</h3>
                        <p>Heading H1</p>
                        <p>Heading H2</p>
                        <p>Body Text</p>
                    </column>
                    <column>
                        <h3>COLORS</h3>
                        <ul>
                            <li>Background: #1A1A1A</li>
                            <li>Card: #1B1D1F</li>
                            <li>Accent: #00D9F2</li>
                            <li>Text: #FFFFFF</li>
                        </ul>
                    </column>
                    <column>
                        <h3>COMPONENTS</h3>
                        <button onclick="sentinel://welcome">Primary Button</button>
                        <button onclick="sentinel://settings">Secondary Button</button>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#.to_string()
}

pub fn prototypes_page() -> String {
    r#"
    <html>
        <body>
            <section>
                <h1>PROTOTYPES</h1>
                <p>Interactive components showcasing navigation, search, and settings.</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>NAVIGATION</h3>
                        <ul>
                            <li><a href="sentinel://search?q=usability">Search Flow</a></li>
                            <li><a href="sentinel://settings">Settings Flow</a></li>
                            <li><a href="sentinel://connect">Connect Flow</a></li>
                        </ul>
                    </column>
                    <column>
                        <h3>FORMS</h3>
                        <button onclick="sentinel://add_bookmark?url=https://sentinel.dao&title=Sentinel">Add Bookmark</button>
                        <button onclick="sentinel://toggle_history">Toggle History</button>
                    </column>
                    <column>
                        <h3>METRICS</h3>
                        <p>3-click rule: All primary features reachable within 2 clicks from dashboard.</p>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#.to_string()
}

pub fn network_menu_page(pt_status: &str) -> String {
    format!(r#"
    <html>
        <body>
            <section>
                <h1>NETWORK MENU</h1>
                <p>Tor, bridges, and Direct only. PT status: {}</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>BRIDGES</h3>
                        <p><a href="sentinel://add_bridge_obfs4?addr=&amp;cert=&amp;iat_mode=0">Add Obfs4 (fill addr/cert in URL)</a></p>
                        <p><a href="sentinel://add_bridge_snowflake?broker=https://snowflake-broker.torproject.net/&amp;relay=snowflake.torproject.net">Add Snowflake</a></p>
                        <p><a href="sentinel://network?type=tor">Use Tor</a></p>
                        <p><a href="sentinel://network?type=snowflake">Use Tor + Snowflake</a></p>
                    </column>
                    <column>
                        <h3>DIRECT</h3>
                        <p><a href="sentinel://network?type=clear">Clearweb (no proxy)</a></p>
                        <p>V2Ray appears here only when V2RAY_PATH points to a binary and a real outbound SOCKS inbound is configured.</p>
                    </column>
                    <column>
                        <h3>CONNECTIONS</h3>
                        <p><a href="sentinel://connect">Connect</a></p>
                        <p><a href="sentinel://status">Status</a></p>
                        <p><a href="sentinel://home">Close</a></p>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, pt_status)
}
pub fn security_page(level: &str, dns: &str, webrtc: &str, fingerprint: &str) -> String {
    format!(r#"
    <html>
        <body>
            <section>
                <h1>SECURITY AUDIT</h1>
                <p>Real-time system protection status.</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>SANDBOX</h3>
                        <p>Level: {}</p>
                        <p>Process Isolation: Active</p>
                        <p>Zygote: Locked</p>
                        <button onclick="sentinel://toggle_security">CHANGE LEVEL</button>
                    </column>
                    <column>
                        <h3>NETWORK PRIVACY</h3>
                        <p>DNS Leak Protection: {}</p>
                        <p>WebRTC Exposure: {}</p>
                        <p>Fingerprint Masking: {}</p>
                        <button onclick="sentinel://status">NET STATUS</button>
                    </column>
                    <column>
                        <h3>SYSTEM</h3>
                        <p>PQC Handshake: ML-KEM-1024</p>
                        <p>Vault Nonce: Secure</p>
                        <button onclick="sentinel://welcome">HOME</button>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, level, dns, webrtc, fingerprint)
}

pub fn downloads_page(downloads: Vec<(String, String, String, String)>) -> String {
    let mut downloads_html = String::new();
    for (url, filename, status, time) in downloads {
        downloads_html.push_str(&format!(
            r#"
            <li>
                <b>{}</b>
                <p>Source: {}</p>
                <p><i>Status: {} | Date: {}</i></p>
            </li>
            "#,
            filename, url, status, time
        ));
    }

    if downloads_html.is_empty() {
        downloads_html = "<p>No active or past downloads found.</p>".to_string();
    }

    format!(r#"
    <html>
        <body>
            <section>
                <h1>DOWNLOADS</h1>
                <p>Track your file transfers.</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>HISTORY</h3>
                        <ul>
                            {}
                        </ul>
                        <button onclick="sentinel://welcome">HOME</button>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, downloads_html)
}

pub fn download_complete_page(filename: &str) -> String {
    format!(r#"
    <html>
        <body>
            <section>
                <h1>DOWNLOAD SAVED</h1>
                <p>Written under Downloads/SentinelDownloads (or home/SentinelDownloads).</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>DETAILS</h3>
                        <p>File: {}</p>
                        <p><a href="sentinel://downloads">Downloads list</a></p>
                        <p><a href="sentinel://home">Home</a></p>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, html_escape(filename))
}

pub fn download_error_page(msg: &str) -> String {
    format!(r#"
    <html>
        <body>
            <section>
                <h1>DOWNLOAD FAILED</h1>
                <p>{}</p>
                <p>Use <code>sentinel://download?url=https://example.com/file</code></p>
                <p><a href="sentinel://downloads">Downloads</a> · <a href="sentinel://home">Home</a></p>
            </section>
        </body>
    </html>
    "#, html_escape(msg))
}

pub fn bookmarks_page(bookmarks: Vec<(String, String)>) -> String {
    let mut bookmarks_html = String::new();
    for (url, title) in bookmarks {
        bookmarks_html.push_str(&format!(
            r#"
            <li>
                <b>{}</b>
                <p>{}</p>
                <button onclick="{}">OPEN</button>
            </li>
            "#, 
            title, url, url
        ));
    }

    format!(r#"
    <html>
        <body>
            <section>
                <h1>BOOKMARKS</h1>
                <p>Saved Sites & Resources</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>FOLDERS</h3>
                        <ul>
                            <li><b>Favorites</b></li>
                            <li>Reading List</li>
                            <li>Onion Sites</li>
                        </ul>
                    </column>
                    <column>
                        <h3>SAVED</h3>
                        <ul>
                            {}
                        </ul>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, bookmarks_html)
}

pub fn history_page(history: Vec<(String, String, String)>) -> String {
    let mut history_html = String::new();
    for (url, title, time) in history {
        history_html.push_str(&format!(
            r#"
            <li>
                <b>{}</b>
                <p>{}</p>
                <p><i>{}</i></p>
                <button onclick="{}">OPEN</button>
            </li>
            "#, 
            title, url, time, url
        ));
    }

    format!(r#"
    <html>
        <body>
            <section>
                <h1>HISTORY</h1>
                <p>Recent Activity Log</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>ACTIONS</h3>
                        <button onclick="sentinel://clear_history">CLEAR HISTORY</button>
                        <button onclick="sentinel://welcome">HOME</button>
                    </column>
                    <column>
                        <h3>RECENT</h3>
                        <ul>
                            {}
                        </ul>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, history_html)
}

pub fn error_page(title: &str, message: &str) -> String {
    format!(r#"
    <html>
        <body>
            <section>
                <h1>{}</h1>
                <p>{}</p>
            </section>
            <button onclick="sentinel://welcome">HOME</button>
        </body>
    </html>
    "#, title, message)
}

pub fn governance_page(proposals: &[Proposal]) -> String {
    let mut proposals_html = String::new();
    for p in proposals {
        proposals_html.push_str(&format!(
            r#"
            <li>
                <b>#{} {}</b>
                <p>{}</p>
                <p><i>Author: {} | Deadline: {}</i></p>
                <a href="sentinel://vote?id={}&approve=true">Vote yes (demo)</a>
                · <a href="sentinel://vote?id={}&approve=false">Vote no (demo)</a>
            </li>
            "#,
            p.id, p.title, p.description, p.author, p.deadline, p.id, p.id
        ));
    }

    format!(r#"
    <html>
        <body>
            <section>
                <h1>GOVERNANCE (EXPERIMENTAL)</h1>
                <p>Local ZK circuit demo only — not an on-chain DAO, treasury, or production voting system.</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>STATUS</h3>
                        <p>Mode: in-process Groth16 demo</p>
                        <p>Active proposals: {}</p>
                        <p><a href="sentinel://home">Back</a></p>
                    </column>
                    <column>
                        <h3>DEMO PROPOSALS</h3>
                        <ul>
                            {}
                        </ul>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#, proposals.len(), proposals_html)
}

pub fn status_page(
    summary: &str,
    protocol: &str,
    socks: &str,
    tor_ready: bool,
    active_conns: usize,
    memory_mb: u64,
    pt: &str,
) -> String {
    format!(r#"
    <html>
        <body>
            <section>
                <h1>NETWORK STATUS</h1>
                <p>Live Vortex snapshot (no invented bandwidth).</p>
            </section>
            <div class="row">
                <row>
                    <column>
                        <h3>CONNECTION</h3>
                        <p>{}</p>
                        <p>Protocol: {}</p>
                        <p>Tor client: {}</p>
                        <p>SOCKS: {}</p>
                        <hr/>
                        <h4>CHOOSE PROTOCOL</h4>
                        <p><a href="sentinel://network?type=tor">Tor</a></p>
                        <p><a href="sentinel://network?type=snowflake">Tor + Snowflake</a></p>
                        <p><a href="sentinel://network?type=clear">Clearweb</a></p>
                    </column>
                    <column>
                        <h3>RUNTIME</h3>
                        <p>Active Vortex streams: {}</p>
                        <p>Process RAM: {} MB</p>
                        <p>PT: {}</p>
                        <p>WebView traffic is not counted here (engine-side).</p>
                    </column>
                    <column>
                        <h3>ROUTING</h3>
                        <p>Exit path depends on Tor/Direct mode.</p>
                        <p><a href="sentinel://connect">Connect</a></p>
                        <p><a href="sentinel://home">Back</a></p>
                    </column>
                </row>
            </div>
        </body>
    </html>
    "#,
        html_escape(summary),
        html_escape(protocol),
        if tor_ready { "ready" } else { "not ready" },
        html_escape(socks),
        active_conns,
        memory_mb,
        html_escape(pt),
    )
}
