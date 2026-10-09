//! SSH connections: authentication, host key policy, jump chains, shells.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, ChannelOpenHandle, Handle, Msg};
use russh::keys::{Certificate, PrivateKey, PrivateKeyWithHashAlg, PublicKey};
use russh::keys::PublicKeyOrCertificate;
use russh::{ChannelMsg, ChannelReadHalf, ChannelWriteHalf};
use thiserror::Error;
use zeroize::Zeroizing;

#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("timed out connecting to {0}")]
    Timeout(String),
    #[error("network error reaching {host}: {message}")]
    Network { host: String, message: String },
    #[error("host key for {0} was rejected")]
    HostKeyRejected(String),
    #[error("authentication failed for {0}")]
    AuthFailed(String),
    #[error("ssh error: {0}")]
    Ssh(String),
}

#[derive(Clone, Default)]
pub struct Credentials {
    pub username: String,
    pub password: Option<Zeroizing<String>>,
    pub key: Option<Arc<PrivateKey>>,
    pub certificate: Option<Certificate>,
}

#[derive(Clone)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub credentials: Credentials,
}

#[derive(Debug, Clone)]
pub struct ConnectOptions {
    pub timeout: Duration,
    pub keepalive: Option<Duration>,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self { timeout: Duration::from_secs(15), keepalive: Some(Duration::from_secs(30)) }
    }
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Decides whether to trust a server's host key. The app implements this
/// with known_hosts plus a user prompt.
pub trait HostKeyPolicy: Send + Sync + 'static {
    fn verify(&self, host: String, port: u16, key: PublicKey) -> BoxFuture<bool>;
}

/// An authenticated session. Jump-host sessions are kept alive alongside it.
pub struct Connection {
    pub(crate) handle: Arc<Handle<ClientHandler>>,
    pub(crate) routes: RemoteRoutes,
    _jumps: Vec<Handle<ClientHandler>>,
}

/// Remote (-R) forwards: server-side port -> local destination.
pub(crate) type RemoteRoutes = Arc<Mutex<HashMap<u32, (String, u16)>>>;

pub(crate) struct ClientHandler {
    host: String,
    port: u16,
    policy: Arc<dyn HostKeyPolicy>,
    routes: RemoteRoutes,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let key = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            // Host certificates are checked against the certified key for now.
            PublicKeyOrCertificate::Certificate(cert) => PublicKey::new(cert.public_key().clone(), ""),
        };
        Ok(self.policy.verify(self.host.clone(), self.port, key).await)
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<Msg>,
        _connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let dest = self.routes.lock().expect("routes lock").get(&connected_port).cloned();
        let Some((host, port)) = dest else {
            return Ok(()); // dropping `reply` rejects the channel
        };
        reply.accept().await;
        tokio::spawn(async move {
            if let Ok(mut tcp) = tokio::net::TcpStream::connect((host.as_str(), port)).await {
                let mut stream = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut tcp, &mut stream).await;
            }
        });
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub exit_status: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct ShellRequest {
    pub cols: u32,
    pub rows: u32,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEvent {
    Output(Vec<u8>),
    Exit(u32),
}

pub struct Shell {
    reader: ShellReader,
    writer: ShellWriter,
}

/// Read half of a [`Shell`], for driving input and output from separate tasks.
pub struct ShellReader(ChannelReadHalf);

/// Write half of a [`Shell`].
pub struct ShellWriter(ChannelWriteHalf<Msg>);

/// Connect to `target`, tunnelling through each of `jumps` in order.
pub async fn connect(
    jumps: &[Endpoint],
    target: &Endpoint,
    policy: Arc<dyn HostKeyPolicy>,
    opts: &ConnectOptions,
) -> Result<Connection, ConnectError> {
    let mut hops: Vec<Handle<ClientHandler>> = Vec::with_capacity(jumps.len());
    for ep in jumps {
        let handle = connect_hop(hops.last(), ep, policy.clone(), RemoteRoutes::default(), opts).await?;
        hops.push(handle);
    }
    let routes = RemoteRoutes::default();
    let handle = connect_hop(hops.last(), target, policy, routes.clone(), opts).await?;
    Ok(Connection { handle: Arc::new(handle), routes, _jumps: hops })
}

async fn connect_hop(
    via: Option<&Handle<ClientHandler>>,
    ep: &Endpoint,
    policy: Arc<dyn HostKeyPolicy>,
    routes: RemoteRoutes,
    opts: &ConnectOptions,
) -> Result<Handle<ClientHandler>, ConnectError> {
    let config = Arc::new(client::Config {
        keepalive_interval: opts.keepalive,
        inactivity_timeout: None,
        ..Default::default()
    });
    let handler = ClientHandler { host: ep.host.clone(), port: ep.port, policy, routes };
    let attempt = async {
        let mut handle = match via {
            None => {
                let tcp = tokio::net::TcpStream::connect((ep.host.as_str(), ep.port))
                    .await
                    .map_err(|e| ConnectError::Network { host: ep.host.clone(), message: e.to_string() })?;
                tcp.set_nodelay(true).ok();
                client::connect_stream(config, tcp, handler).await
            }
            Some(prev) => {
                let channel = prev
                    .channel_open_direct_tcpip(ep.host.clone(), ep.port.into(), "127.0.0.1", 0)
                    .await
                    .map_err(|e| ConnectError::Network { host: ep.host.clone(), message: e.to_string() })?;
                client::connect_stream(config, channel.into_stream(), handler).await
            }
        }
        .map_err(|e| map_err(&ep.host, e))?;
        authenticate(&mut handle, ep).await?;
        Ok(handle)
    };
    tokio::time::timeout(opts.timeout, attempt)
        .await
        .map_err(|_| ConnectError::Timeout(ep.host.clone()))?
}

/// Tries certificate, then key, then password; succeeds on the first accepted.
async fn authenticate(handle: &mut Handle<ClientHandler>, ep: &Endpoint) -> Result<(), ConnectError> {
    let c = &ep.credentials;
    let user = c.username.clone();
    let ssh = |e| map_err(&ep.host, e);
    if let (Some(key), Some(cert)) = (&c.key, &c.certificate) {
        if handle.authenticate_openssh_cert(&user, key.clone(), cert.clone()).await.map_err(ssh)?.success() {
            return Ok(());
        }
    }
    if let Some(key) = &c.key {
        let hash = handle.best_supported_rsa_hash().await.ok().flatten().flatten();
        let key = PrivateKeyWithHashAlg::new(key.clone(), hash);
        if handle.authenticate_publickey(&user, key).await.map_err(ssh)?.success() {
            return Ok(());
        }
    }
    if let Some(pw) = &c.password {
        if handle.authenticate_password(&user, pw.as_str()).await.map_err(ssh)?.success() {
            return Ok(());
        }
    }
    Err(ConnectError::AuthFailed(ep.host.clone()))
}

fn map_err(host: &str, e: russh::Error) -> ConnectError {
    match e {
        russh::Error::UnknownKey => ConnectError::HostKeyRejected(host.to_string()),
        russh::Error::IO(io) => ConnectError::Network { host: host.to_string(), message: io.to_string() },
        other => ConnectError::Ssh(other.to_string()),
    }
}

impl Connection {
    /// Run a command without a PTY and collect its output.
    pub async fn exec(&self, command: &str) -> Result<ExecOutput, ConnectError> {
        let mut channel = self.handle.channel_open_session().await.map_err(ssh_err)?;
        channel.exec(true, command).await.map_err(ssh_err)?;
        let mut out = ExecOutput { stdout: Vec::new(), stderr: Vec::new(), exit_status: None };
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => out.stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, ext: 1 } => out.stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => out.exit_status = Some(exit_status),
                _ => {}
            }
        }
        Ok(out)
    }

    pub async fn open_shell(&self, req: ShellRequest) -> Result<Shell, ConnectError> {
        let channel = self.handle.channel_open_session().await.map_err(ssh_err)?;
        for (k, v) in &req.env {
            // Servers silently ignore variables not listed in AcceptEnv.
            channel.set_env(false, k.as_str(), v.as_str()).await.map_err(ssh_err)?;
        }
        channel
            .request_pty(false, "xterm-256color", req.cols, req.rows, 0, 0, &[])
            .await
            .map_err(ssh_err)?;
        channel.request_shell(false).await.map_err(ssh_err)?;
        let (reader, writer) = channel.split();
        Ok(Shell { reader: ShellReader(reader), writer: ShellWriter(writer) })
    }

    pub async fn close(&self) {
        let _ = self.handle.disconnect(russh::Disconnect::ByApplication, "", "en").await;
    }
}

fn ssh_err(e: russh::Error) -> ConnectError {
    ConnectError::Ssh(e.to_string())
}

impl Shell {
    pub async fn write(&self, data: &[u8]) -> Result<(), ConnectError> {
        self.writer.write(data).await
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), ConnectError> {
        self.writer.resize(cols, rows).await
    }

    /// Next output chunk or exit; `None` once the channel is closed.
    pub async fn read(&mut self) -> Option<ShellEvent> {
        self.reader.read().await
    }

    pub fn split(self) -> (ShellReader, ShellWriter) {
        (self.reader, self.writer)
    }
}

impl ShellWriter {
    pub async fn write(&self, data: &[u8]) -> Result<(), ConnectError> {
        self.0.data(data).await.map_err(ssh_err)
    }

    pub async fn resize(&self, cols: u32, rows: u32) -> Result<(), ConnectError> {
        self.0.window_change(cols, rows, 0, 0).await.map_err(ssh_err)
    }

    pub async fn close(&self) {
        let _ = self.0.eof().await;
        let _ = self.0.close().await;
    }
}

impl ShellReader {
    pub async fn read(&mut self) -> Option<ShellEvent> {
        loop {
            match self.0.wait().await? {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    return Some(ShellEvent::Output(data.to_vec()))
                }
                ChannelMsg::ExitStatus { exit_status } => return Some(ShellEvent::Exit(exit_status)),
                _ => continue,
            }
        }
    }
}
