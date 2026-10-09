//! SFTP browser commands. Progress is reported as `TransferProgress` events.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use wh_core::model::Id;
use wh_ssh::client::Connection;
use wh_ssh::sftp::{RemoteEntry, Sftp, SftpError};

use crate::error::ApiError;
use crate::state::{ApiResult, App, AppEvent};

impl From<SftpError> for ApiError {
    fn from(e: SftpError) -> Self {
        let kind = match &e {
            SftpError::NotFound(_) => "not_found",
            SftpError::PermissionDenied(_) => "permission_denied",
            SftpError::Local(_) => "io",
            SftpError::Other(_) => "sftp",
        };
        ApiError::new(kind, e.to_string())
    }
}

type SftpEntry = Arc<(Sftp, Connection)>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

impl App {
    async fn sftp(&self, id: uuid::Uuid) -> ApiResult<SftpEntry> {
        self.inner.sftps.lock().await.get(&id).cloned().ok_or_else(|| ApiError::new("not_found", "sftp session closed"))
    }

    /// Opens an SFTP session to `host`; returns its id.
    pub async fn sftp_open(&self, host: Id) -> ApiResult<uuid::Uuid> {
        let (conn, _) = self.connect_host(host).await?;
        let sftp = conn.sftp().await?;
        let id = uuid::Uuid::new_v4();
        self.inner.sftps.lock().await.insert(id, Arc::new((sftp, conn)));
        Ok(id)
    }

    pub async fn sftp_close(&self, sftp: uuid::Uuid) -> ApiResult<()> {
        let entry = self.inner.sftps.lock().await.remove(&sftp).ok_or_else(|| ApiError::new("not_found", "sftp session closed"))?;
        entry.1.close().await;
        Ok(())
    }

    pub async fn sftp_home(&self, sftp: uuid::Uuid) -> ApiResult<String> {
        Ok(self.sftp(sftp).await?.0.home().await?)
    }

    pub async fn sftp_list(&self, sftp: uuid::Uuid, path: &str) -> ApiResult<Vec<RemoteEntry>> {
        Ok(self.sftp(sftp).await?.0.list_dir(path).await?)
    }

    pub async fn sftp_mkdir(&self, sftp: uuid::Uuid, path: &str) -> ApiResult<()> {
        Ok(self.sftp(sftp).await?.0.mkdir(path).await?)
    }

    pub async fn sftp_rename(&self, sftp: uuid::Uuid, from: &str, to: &str) -> ApiResult<()> {
        Ok(self.sftp(sftp).await?.0.rename(from, to).await?)
    }

    pub async fn sftp_remove(&self, sftp: uuid::Uuid, path: &str) -> ApiResult<()> {
        Ok(self.sftp(sftp).await?.0.remove(path).await?)
    }

    /// `transfer` is chosen by the caller and tags the progress events.
    pub async fn sftp_upload(&self, sftp: uuid::Uuid, transfer: uuid::Uuid, local: &str, remote: &str) -> ApiResult<u64> {
        let entry = self.sftp(sftp).await?;
        let mut progress = self.progress(transfer);
        Ok(entry.0.upload(Path::new(local), remote, &mut progress).await?)
    }

    pub async fn sftp_download(&self, sftp: uuid::Uuid, transfer: uuid::Uuid, remote: &str, local: &str) -> ApiResult<u64> {
        let entry = self.sftp(sftp).await?;
        let mut progress = self.progress(transfer);
        Ok(entry.0.download(remote, Path::new(local), &mut progress).await?)
    }

    fn progress(&self, transfer: uuid::Uuid) -> impl FnMut(u64, u64) + Send {
        let events = self.inner.events.clone();
        move |done, total| events(AppEvent::TransferProgress { transfer, done, total })
    }

    /// Lists a local directory for the left-hand pane (dirs first, then by name).
    pub fn local_list(&self, path: &str) -> ApiResult<Vec<LocalEntry>> {
        let read = std::fs::read_dir(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ApiError::new("not_found", format!("{path} not found")),
            std::io::ErrorKind::PermissionDenied => ApiError::new("permission_denied", e.to_string()),
            _ => ApiError::new("io", e.to_string()),
        })?;
        let mut entries: Vec<LocalEntry> = read
            .filter_map(Result::ok)
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                Some(LocalEntry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    path: e.path().to_string_lossy().into_owned(),
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                })
            })
            .collect();
        entries.sort_by(|a, b| (!a.is_dir, &a.name).cmp(&(!b.is_dir, &b.name)));
        Ok(entries)
    }
}
