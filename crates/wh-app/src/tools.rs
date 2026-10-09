//! Port forwards and running snippets across hosts.

use std::collections::BTreeMap;

use serde::Serialize;
use wh_core::model::{ForwardKind, Id, PortForward};

use crate::error::ApiError;
use crate::state::{ApiResult, App};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActiveForward {
    pub rule: Id,
    pub listen: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnippetRun {
    pub host: Id,
    pub stdout: String,
    pub stderr: String,
    pub exit_status: Option<u32>,
    pub error: Option<ApiError>,
}

impl App {
    /// Starts a saved port-forward rule on its host.
    pub async fn forward_start(&self, rule: Id) -> ApiResult<ActiveForward> {
        if self.inner.forwards.lock().await.contains_key(&rule) {
            return Err(ApiError::new("already_running", "this forward is already running"));
        }
        let f = self
            .vault()
            .get::<PortForward>(rule)?
            .ok_or_else(|| ApiError::new("not_found", "forward rule not found"))?;
        let (conn, _) = self.connect_host(f.host).await?;
        let bind = if f.bind_address.is_empty() { "127.0.0.1" } else { f.bind_address.as_str() };
        let handle = match f.kind {
            ForwardKind::Local => conn.forward_local(bind, f.bind_port, &f.dest_host, f.dest_port).await,
            ForwardKind::Remote => conn.forward_remote(bind, f.bind_port, &f.dest_host, f.dest_port).await,
            ForwardKind::Dynamic => conn.forward_dynamic(bind, f.bind_port).await,
        }
        .map_err(|e| ApiError::new("forward_failed", e.to_string()))?;
        let active = ActiveForward { rule, listen: handle.listen.clone() };
        self.inner.forwards.lock().await.insert(rule, (handle, conn));
        Ok(active)
    }

    pub async fn forward_stop(&self, rule: Id) -> ApiResult<()> {
        let (handle, conn) = self
            .inner
            .forwards
            .lock()
            .await
            .remove(&rule)
            .ok_or_else(|| ApiError::new("not_found", "forward is not running"))?;
        handle.stop().await;
        conn.close().await;
        Ok(())
    }

    pub async fn forwards_active(&self) -> Vec<ActiveForward> {
        let running = self.inner.forwards.lock().await;
        running.iter().map(|(rule, (h, _))| ActiveForward { rule: *rule, listen: h.listen.clone() }).collect()
    }

    /// Renders `script` with `vars` and runs it on every host concurrently.
    /// Results are in the order of `hosts`; per-host failures are reported, not raised.
    pub async fn run_snippet(
        &self,
        script: &str,
        vars: &BTreeMap<String, String>,
        hosts: &[Id],
    ) -> ApiResult<Vec<SnippetRun>> {
        let command = wh_core::snippet::render(script, vars).map_err(|e| ApiError::new("validation", e.to_string()))?;
        let tasks: Vec<_> = hosts
            .iter()
            .map(|&host| {
                let (app, command) = (self.clone(), command.clone());
                tokio::spawn(async move { app.run_on(host, &command).await })
            })
            .collect();
        let mut runs = Vec::with_capacity(tasks.len());
        for (task, &host) in tasks.into_iter().zip(hosts) {
            let run = task.await.unwrap_or_else(|e| SnippetRun::failed(host, ApiError::new("internal", e.to_string())));
            runs.push(run);
        }
        Ok(runs)
    }

    async fn run_on(&self, host: Id, command: &str) -> SnippetRun {
        let result = async {
            let (conn, _) = self.connect_host(host).await?;
            let out = conn.exec(command).await?;
            conn.close().await;
            Ok::<_, ApiError>(out)
        }
        .await;
        match result {
            Ok(out) => SnippetRun {
                host,
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                exit_status: out.exit_status,
                error: None,
            },
            Err(e) => SnippetRun::failed(host, e),
        }
    }
}

impl SnippetRun {
    fn failed(host: Id, error: ApiError) -> Self {
        Self { host, stdout: String::new(), stderr: String::new(), exit_status: None, error: Some(error) }
    }
}
