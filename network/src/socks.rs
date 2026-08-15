/*
 * Local SOCKS5 proxy that tunnels CONNECT through arti TorClient.
 * Used by WebView2 (proxy config) and reqwest for .onion / Tor mode.
 */

use anyhow::{anyhow, Context, Result};
use arti_client::{TorAddr, TorClient};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tor_rtcompat::PreferredRuntime;
use tracing::{info, warn};

pub struct SocksProxy {
    pub port: u16,
}

impl SocksProxy {
    /// Bind 127.0.0.1:0 (or preferred) and spawn accept loop.
    pub async fn start(
        tor: Arc<Mutex<Option<TorClient<PreferredRuntime>>>>,
        preferred_port: u16,
    ) -> Result<Self> {
        let addr = SocketAddr::from(([127, 0, 0, 1], preferred_port));
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(_) => TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
                .await
                .context("bind SOCKS listener")?,
        };
        let port = listener.local_addr()?.port();
        info!("Vortex SOCKS5 listening on 127.0.0.1:{}", port);

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, _)) => {
                        let tor = tor.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_socks_client(socket, tor).await {
                                warn!("SOCKS client error: {}", e);
                            }
                        });
                    }
                    Err(e) => warn!("SOCKS accept error: {}", e),
                }
            }
        });

        Ok(Self { port })
    }
}

async fn handle_socks_client(
    mut client: TcpStream,
    tor: Arc<Mutex<Option<TorClient<PreferredRuntime>>>>,
) -> Result<()> {
    // SOCKS5 greeting
    let mut buf = [0u8; 2];
    client.read_exact(&mut buf).await?;
    if buf[0] != 0x05 {
        return Err(anyhow!("not SOCKS5"));
    }
    let nmethods = buf[1] as usize;
    let mut methods = vec![0u8; nmethods];
    client.read_exact(&mut methods).await?;
    // No auth
    client.write_all(&[0x05, 0x00]).await?;

    // Request
    let mut hdr = [0u8; 4];
    client.read_exact(&mut hdr).await?;
    if hdr[0] != 0x05 || hdr[1] != 0x01 {
        // Only CONNECT
        client.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
        return Err(anyhow!("unsupported SOCKS command"));
    }
    let atyp = hdr[3];
    let host = match atyp {
        0x01 => {
            let mut ip = [0u8; 4];
            client.read_exact(&mut ip).await?;
            format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
        }
        0x03 => {
            let mut len = [0u8; 1];
            client.read_exact(&mut len).await?;
            let mut name = vec![0u8; len[0] as usize];
            client.read_exact(&mut name).await?;
            String::from_utf8(name).context("hostname utf8")?
        }
        0x04 => {
            let mut ip = [0u8; 16];
            client.read_exact(&mut ip).await?;
            std::net::Ipv6Addr::from(ip).to_string()
        }
        _ => return Err(anyhow!("bad atyp")),
    };
    let mut port_buf = [0u8; 2];
    client.read_exact(&mut port_buf).await?;
    let port = u16::from_be_bytes(port_buf);

    let guard = tor.lock().await;
    let Some(client_tor) = guard.as_ref() else {
        client
            .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await?;
        return Err(anyhow!("Tor not ready"));
    };

    let target = TorAddr::from((&*host, port)).map_err(|e| anyhow!("{}", e))?;
    let remote = match client_tor.connect(target).await {
        Ok(s) => s,
        Err(e) => {
            client
                .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await?;
            return Err(anyhow!("tor connect: {}", e));
        }
    };

    // Success
    client
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;

    let (mut cr, mut cw) = client.into_split();
    let (mut rr, mut rw) = tokio::io::split(remote);
    let c2r = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut cr, &mut rw).await;
    });
    let r2c = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut rr, &mut cw).await;
    });
    let _ = tokio::try_join!(c2r, r2c);
    Ok(())
}
