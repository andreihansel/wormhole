//! SFTP file manager operations.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use russh_sftp::client::error::Error as RawError;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, StatusCode};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::client::{ConnectError, Connection};

const CHUNK: usize = 32 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SftpError {
    #[error("no such file: {0}")]
    NotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("local file error: {0}")]
    Local(String),
    #[error("sftp error: {0}")]
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub kind: EntryKind,
    pub size: u64,
    /// Permission bits (`mode & 0o7777`).
    pub mode: u32,
    /// Seconds since the Unix epoch.
    pub mtime: Option<u32>,
}

pub struct Sftp {
    session: SftpSession,
}

impl Connection {
    pub async fn sftp(&self) -> Result<Sftp, ConnectError> {
        let ssh = |e: russh::Error| ConnectError::Ssh(e.to_string());
        let channel = self.handle.channel_open_session().await.map_err(ssh)?;
        channel.request_subsystem(true, "sftp").await.map_err(ssh)?;
        let session = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| ConnectError::Ssh(e.to_string()))?;
        Ok(Sftp { session })
    }
}

fn map_err(path: &str) -> impl Fn(RawError) -> SftpError + '_ {
    move |e| match &e {
        RawError::Status(st) if st.status_code == StatusCode::NoSuchFile => SftpError::NotFound(path.into()),
        RawError::Status(st) if st.status_code == StatusCode::PermissionDenied => {
            SftpError::PermissionDenied(path.into())
        }
        _ => SftpError::Other(e.to_string()),
    }
}

fn local_err(e: std::io::Error) -> SftpError {
    SftpError::Local(e.to_string())
}

fn kind_of(attrs: &FileAttributes) -> EntryKind {
    let t = attrs.file_type();
    if t.is_dir() {
        EntryKind::Dir
    } else if t.is_symlink() {
        EntryKind::Symlink
    } else if t.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

impl Sftp {
    /// Absolute path of the login directory.
    pub async fn home(&self) -> Result<String, SftpError> {
        self.session.canonicalize(".").await.map_err(map_err("."))
    }

    /// Directories first, then files; each alphabetical. `.` and `..` are omitted.
    pub async fn list_dir(&self, path: &str) -> Result<Vec<RemoteEntry>, SftpError> {
        let mut entries: Vec<RemoteEntry> = self
            .session
            .read_dir(path)
            .await
            .map_err(map_err(path))?
            .filter(|e| e.file_name() != "." && e.file_name() != "..")
            .map(|e| {
                let attrs = e.metadata();
                RemoteEntry {
                    path: join(path, &e.file_name()),
                    name: e.file_name(),
                    kind: kind_of(&attrs),
                    size: attrs.size.unwrap_or(0),
                    mode: attrs.permissions.unwrap_or(0) & 0o7777,
                    mtime: attrs.mtime,
                }
            })
            .collect();
        entries.sort_by(|a, b| (a.kind != EntryKind::Dir, &a.name).cmp(&(b.kind != EntryKind::Dir, &b.name)));
        Ok(entries)
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), SftpError> {
        self.session.create_dir(path).await.map_err(map_err(path))
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), SftpError> {
        self.session.rename(from, to).await.map_err(map_err(from))
    }

    /// Removes a file, or a directory and everything in it.
    pub fn remove<'a>(&'a self, path: &'a str) -> Pin<Box<dyn Future<Output = Result<(), SftpError>> + Send + 'a>> {
        Box::pin(async move {
            let attrs = self.session.symlink_metadata(path).await.map_err(map_err(path))?;
            if kind_of(&attrs) != EntryKind::Dir {
                return self.session.remove_file(path).await.map_err(map_err(path));
            }
            for entry in self.list_dir(path).await? {
                self.remove(&entry.path).await?;
            }
            self.session.remove_dir(path).await.map_err(map_err(path))
        })
    }

    /// Returns bytes written. `progress(done, total)` is called as data is sent.
    pub async fn upload(
        &self,
        local: &Path,
        remote: &str,
        progress: &mut (dyn FnMut(u64, u64) + Send),
    ) -> Result<u64, SftpError> {
        let mut src = tokio::fs::File::open(local).await.map_err(local_err)?;
        let total = src.metadata().await.map_err(local_err)?.len();
        let mut dst = self.session.create(remote).await.map_err(map_err(remote))?;
        let done = copy(&mut src, &mut dst, total, progress, |e| SftpError::Other(e.to_string())).await?;
        dst.shutdown().await.map_err(|e| SftpError::Other(e.to_string()))?;
        Ok(done)
    }

    pub async fn download(
        &self,
        remote: &str,
        local: &Path,
        progress: &mut (dyn FnMut(u64, u64) + Send),
    ) -> Result<u64, SftpError> {
        let total = self.session.metadata(remote).await.map_err(map_err(remote))?.size.unwrap_or(0);
        let mut src = self.session.open(remote).await.map_err(map_err(remote))?;
        // Write beside the target and rename on success, so failures leave nothing behind.
        let dir = local.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let tmp = tempfile::NamedTempFile::new_in(dir).map_err(local_err)?;
        let mut dst = tokio::fs::File::from_std(tmp.reopen().map_err(local_err)?);
        let done = copy(&mut src, &mut dst, total, progress, |e| SftpError::Other(e.to_string())).await?;
        dst.flush().await.map_err(local_err)?;
        tmp.persist(local).map_err(|e| local_err(e.error))?;
        Ok(done)
    }

    /// Uploads a directory tree into `remote` (created if missing). Returns files copied.
    pub fn upload_dir<'a>(
        &'a self,
        local: &'a Path,
        remote: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<usize, SftpError>> + Send + 'a>> {
        Box::pin(async move {
            if !self.session.try_exists(remote).await.map_err(map_err(remote))? {
                self.mkdir(remote).await?;
            }
            let mut copied = 0;
            for entry in std::fs::read_dir(local).map_err(local_err)? {
                let entry = entry.map_err(local_err)?;
                let target = join(remote, &entry.file_name().to_string_lossy());
                if entry.file_type().map_err(local_err)?.is_dir() {
                    copied += self.upload_dir(&entry.path(), &target).await?;
                } else {
                    self.upload(&entry.path(), &target, &mut |_, _| {}).await?;
                    copied += 1;
                }
            }
            Ok(copied)
        })
    }
}

async fn copy<R, W>(
    src: &mut R,
    dst: &mut W,
    total: u64,
    progress: &mut (dyn FnMut(u64, u64) + Send),
    err: impl Fn(std::io::Error) -> SftpError,
) -> Result<u64, SftpError>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; CHUNK];
    let mut done = 0u64;
    loop {
        let n = src.read(&mut buf).await.map_err(&err)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).await.map_err(&err)?;
        done += n as u64;
        progress(done, total.max(done));
    }
    Ok(done)
}
