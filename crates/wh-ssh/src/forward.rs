//! Port forwarding: local (-L), remote (-R) and dynamic SOCKS5 (-D).

use std::net::SocketAddr;
use std::sync::Arc;

use russh::client::{Handle, Msg};
use russh::ChannelStream;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::client::{ClientHandler, Connection, RemoteRoutes};

#[derive(Debug, Error)]
pub enum ForwardError {
    #[error("cannot listen on {addr}: {message}")]
    Bind { addr: String, message: String },
    #[error("server refused the forward: {0}")]
    Refused(String),
}

/// A running forward. Dropping it (or calling `stop`) closes the listener.
pub struct ForwardHandle {
    /// Where the forward listens (local side for -L/-D, server side for -R).
    pub listen: String,
    local: Option<SocketAddr>,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
    remote: Option<RemoteCancel>,
}

struct RemoteCancel {
    handle: Arc<Handle<ClientHandler>>,
    routes: RemoteRoutes,
    bind: String,
    port: u32,
}

impl RemoteCancel {
    async fn cancel(self) {
        self.routes.lock().expect("routes lock").remove(&self.port);
        let _ = self.handle.cancel_tcpip_forward(self.bind, self.port).await;
    }
}

impl ForwardHandle {
    /// Stops accepting new connections. Connections already open keep running.
    pub async fn stop(mut self) {
        self.stop.take();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        if let Some(remote) = self.remote.take() {
            remote.cancel().await;
        }
    }

    /// The local socket address for -L/-D forwards.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local
    }
}

impl Drop for ForwardHandle {
    fn drop(&mut self) {
        if let Some(remote) = self.remote.take() {
            if let Ok(rt) = tokio::runtime::Handle::try_current() {
                rt.spawn(remote.cancel());
            }
        }
    }
}

/// Accepts on `listener` until stopped, handing each socket to `serve`.
fn spawn_listener<F, Fut>(listener: TcpListener, serve: F) -> (oneshot::Sender<()>, JoinHandle<()>)
where
    F: Fn(TcpStream) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stop_rx => break,
                accepted = listener.accept() => match accepted {
                    Ok((sock, _)) => { tokio::spawn(serve(sock)); }
                    Err(_) => break,
                },
            }
        }
    });
    (stop_tx, task)
}

async fn bind(addr: &str, port: u16) -> Result<(TcpListener, SocketAddr), ForwardError> {
    let listener = TcpListener::bind((addr, port))
        .await
        .map_err(|e| ForwardError::Bind { addr: format!("{addr}:{port}"), message: e.to_string() })?;
    let local = listener.local_addr().map_err(|e| ForwardError::Bind { addr: addr.into(), message: e.to_string() })?;
    Ok((listener, local))
}

/// Opens a `direct-tcpip` channel to `host:port`, reporting `sock`'s peer as the originator.
async fn open_direct(
    handle: &Handle<ClientHandler>,
    sock: &TcpStream,
    host: String,
    port: u16,
) -> Option<ChannelStream<Msg>> {
    let (ip, orig_port) = sock.peer_addr().map(|a| (a.ip().to_string(), a.port())).unwrap_or(("127.0.0.1".into(), 0));
    let channel = handle.channel_open_direct_tcpip(host, port.into(), ip, orig_port.into()).await.ok()?;
    Some(channel.into_stream())
}

async fn tunnel(handle: Arc<Handle<ClientHandler>>, mut sock: TcpStream, host: String, port: u16) {
    if let Some(mut stream) = open_direct(&handle, &sock, host, port).await {
        let _ = tokio::io::copy_bidirectional(&mut sock, &mut stream).await;
    }
}

async fn serve_socks(handle: Arc<Handle<ClientHandler>>, mut sock: TcpStream) {
    use socks5::{Greeting, Request};
    let mut buf = Vec::with_capacity(64);
    let greeting = loop {
        if let Some((g, used)) = socks5::parse_greeting(&buf) {
            buf.drain(..used);
            break g;
        }
        if read_more(&mut sock, &mut buf).await.is_none() {
            return;
        }
    };
    if greeting != Greeting::NoAuth {
        let _ = sock.write_all(&[5, 0xFF]).await;
        return;
    }
    if sock.write_all(&[5, 0]).await.is_err() {
        return;
    }
    let request = loop {
        if let Some((r, used)) = socks5::parse_request(&buf) {
            buf.drain(..used);
            break r;
        }
        if read_more(&mut sock, &mut buf).await.is_none() {
            return;
        }
    };
    match request {
        Request::Reject(code) => {
            let _ = sock.write_all(&socks5::reply(code)).await;
        }
        Request::Connect { host, port } => match open_direct(&handle, &sock, host, port).await {
            Some(mut stream) => {
                if sock.write_all(&socks5::reply(0)).await.is_ok() {
                    let _ = tokio::io::copy_bidirectional(&mut sock, &mut stream).await;
                }
            }
            // 0x05: connection refused
            None => {
                let _ = sock.write_all(&socks5::reply(5)).await;
            }
        },
    }
}

async fn read_more(sock: &mut TcpStream, buf: &mut Vec<u8>) -> Option<()> {
    const MAX_HANDSHAKE: usize = 600;
    let mut chunk = [0u8; 256];
    let n = sock.read(&mut chunk).await.ok().filter(|&n| n > 0)?;
    buf.extend_from_slice(&chunk[..n]);
    (buf.len() <= MAX_HANDSHAKE).then_some(())
}

impl Connection {
    pub async fn forward_local(
        &self,
        bind_addr: &str,
        port: u16,
        dest_host: &str,
        dest_port: u16,
    ) -> Result<ForwardHandle, ForwardError> {
        let (listener, local) = bind(bind_addr, port).await?;
        let handle = self.handle.clone();
        let dest = dest_host.to_string();
        let (stop, task) = spawn_listener(listener, move |sock| {
            let (handle, dest) = (handle.clone(), dest.clone());
            tunnel(handle, sock, dest, dest_port)
        });
        Ok(ForwardHandle { listen: local.to_string(), local: Some(local), stop: Some(stop), task: Some(task), remote: None })
    }

    pub async fn forward_dynamic(&self, bind_addr: &str, port: u16) -> Result<ForwardHandle, ForwardError> {
        let (listener, local) = bind(bind_addr, port).await?;
        let handle = self.handle.clone();
        let (stop, task) = spawn_listener(listener, move |sock| serve_socks(handle.clone(), sock));
        Ok(ForwardHandle { listen: local.to_string(), local: Some(local), stop: Some(stop), task: Some(task), remote: None })
    }

    /// Ask the server to listen on `server_bind:server_port` and send
    /// connections to `dest_host:dest_port` as seen from this machine.
    pub async fn forward_remote(
        &self,
        server_bind: &str,
        server_port: u16,
        dest_host: &str,
        dest_port: u16,
    ) -> Result<ForwardHandle, ForwardError> {
        let port = self
            .handle
            .tcpip_forward(server_bind, server_port.into())
            .await
            .map_err(|e| ForwardError::Refused(e.to_string()))?;
        // The server returns the chosen port only when asked for port 0.
        let port = if port == 0 { u32::from(server_port) } else { port };
        self.routes.lock().expect("routes lock").insert(port, (dest_host.to_string(), dest_port));
        Ok(ForwardHandle {
            listen: format!("{server_bind}:{port}"),
            local: None,
            stop: None,
            task: None,
            remote: Some(RemoteCancel {
                handle: self.handle.clone(),
                routes: self.routes.clone(),
                bind: server_bind.to_string(),
                port,
            }),
        })
    }
}

/// SOCKS5 wire parsing, kept pure for unit testing.
pub mod socks5 {
    #[derive(Debug, PartialEq, Eq)]
    pub enum Greeting {
        /// Client offers "no authentication" (0x00).
        NoAuth,
        Unsupported,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub enum Request {
        Connect { host: String, port: u16 },
        /// Reply code to send before closing (0x07 command not supported, 0x08 address type).
        Reject(u8),
    }

    /// `VER NMETHODS METHODS...`; returns `None` if more bytes are needed.
    pub fn parse_greeting(buf: &[u8]) -> Option<(Greeting, usize)> {
        let (&ver, &n) = (buf.first()?, buf.get(1)?);
        let methods = buf.get(2..2 + n as usize)?;
        let g = if ver == 5 && methods.contains(&0) { Greeting::NoAuth } else { Greeting::Unsupported };
        Some((g, 2 + n as usize))
    }

    /// `VER CMD RSV ATYP DST.ADDR DST.PORT`; returns `None` if more bytes are needed.
    pub fn parse_request(buf: &[u8]) -> Option<(Request, usize)> {
        let (&cmd, &atyp) = (buf.get(1)?, buf.get(3)?);
        let (host, addr_end) = match atyp {
            1 => {
                let a: [u8; 4] = buf.get(4..8)?.try_into().ok()?;
                (std::net::Ipv4Addr::from(a).to_string(), 8)
            }
            3 => {
                let len = *buf.get(4)? as usize;
                (String::from_utf8_lossy(buf.get(5..5 + len)?).into_owned(), 5 + len)
            }
            4 => {
                let a: [u8; 16] = buf.get(4..20)?.try_into().ok()?;
                (std::net::Ipv6Addr::from(a).to_string(), 20)
            }
            // Unknown address type: we cannot know the length, so consume what we have.
            _ => return Some((Request::Reject(0x08), buf.len())),
        };
        let port = u16::from_be_bytes(buf.get(addr_end..addr_end + 2)?.try_into().ok()?);
        let used = addr_end + 2;
        if cmd != 1 {
            return Some((Request::Reject(0x07), used));
        }
        Some((Request::Connect { host, port }, used))
    }

    pub fn reply(code: u8) -> [u8; 10] {
        [5, code, 0, 1, 0, 0, 0, 0, 0, 0]
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn greeting_no_auth() {
            assert_eq!(parse_greeting(&[5, 2, 0x02, 0x00]), Some((Greeting::NoAuth, 4)));
            assert_eq!(parse_greeting(&[5, 1, 0x02]), Some((Greeting::Unsupported, 3)));
            assert_eq!(parse_greeting(&[4, 1, 0]), Some((Greeting::Unsupported, 3)));
            assert_eq!(parse_greeting(&[5, 2, 0]), None, "incomplete");
        }

        #[test]
        fn connect_ipv4_domain_ipv6() {
            assert_eq!(
                parse_request(&[5, 1, 0, 1, 10, 0, 0, 7, 0x1f, 0x90]),
                Some((Request::Connect { host: "10.0.0.7".into(), port: 8080 }, 10))
            );
            let mut dom = vec![5, 1, 0, 3, 9];
            dom.extend_from_slice(b"localhost");
            dom.extend_from_slice(&[0, 80]);
            assert_eq!(parse_request(&dom), Some((Request::Connect { host: "localhost".into(), port: 80 }, dom.len())));
            let mut v6 = vec![5, 1, 0, 4];
            v6.extend_from_slice(&[0; 15]);
            v6.push(1);
            v6.extend_from_slice(&[0, 22]);
            assert_eq!(parse_request(&v6), Some((Request::Connect { host: "::1".into(), port: 22 }, 22)));
        }

        #[test]
        fn rejects_bind_and_unknown_address_type() {
            assert_eq!(parse_request(&[5, 2, 0, 1, 1, 2, 3, 4, 0, 80]), Some((Request::Reject(0x07), 10)));
            assert_eq!(parse_request(&[5, 1, 0, 9, 0, 0]), Some((Request::Reject(0x08), 6)));
        }

        #[test]
        fn incomplete_request_needs_more() {
            assert_eq!(parse_request(&[5, 1, 0, 3, 9, b'l']), None);
            assert_eq!(parse_request(&[5, 1]), None);
        }
    }
}
