//! Shared harness for wh-app integration tests (tests/docker).
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use uuid::Uuid;
use wh_app::{App, AppEvent};
use wh_core::model::{Host, Identity};
use wh_vault::crypto::KdfParams;

const FAST: KdfParams = KdfParams { mem_kib: 256, iterations: 1, parallelism: 1 };

pub struct Harness {
    pub app: App,
    pub events: mpsc::UnboundedReceiver<AppEvent>,
    pub seen: Vec<AppEvent>,
    /// Terminal output per session, accumulated across calls.
    pub output: std::collections::HashMap<Uuid, String>,
    _dir: tempfile::TempDir,
}

pub fn harness() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let (tx, rx) = mpsc::unbounded_channel();
    let app = App::new(dir.path().to_path_buf(), FAST, Arc::new(move |e| { let _ = tx.send(e); })).unwrap();
    app.vault_create("master").unwrap();
    Harness { app, events: rx, seen: vec![], output: Default::default(), _dir: dir }
}

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../tests/fixtures/keys/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

impl Harness {
    pub fn password_host(&self, label: &str, port: u16) -> Host {
        let ident = self.app.save_identity(Identity { id: Uuid::new_v4(), label: "wh".into(), username: Some("wh".into()), key: None }).unwrap();
        self.app.set_password(ident.id, Some("whpass")).unwrap();
        let mut h = Host::new(label, "127.0.0.1").unwrap();
        h.settings.port = Some(port);
        h.settings.identity = Some(ident.id);
        self.app.save_host(h).unwrap()
    }

    pub fn key_identity(&self) -> Identity {
        let key = self.app.key_import("fixture", &fixture("id_ed25519"), None).unwrap();
        self.app.save_identity(Identity { id: Uuid::new_v4(), label: "key".into(), username: Some("wh".into()), key: Some(key.id) }).unwrap()
    }

    /// Answers every host key prompt with `accept` until `fut` completes.
    pub async fn with_prompts<T>(&mut self, accept: bool, fut: impl std::future::Future<Output = T>) -> T {
        tokio::pin!(fut);
        loop {
            tokio::select! {
                out = &mut fut => {
                    // Keep events emitted just before completion.
                    while let Ok(ev) = self.events.try_recv() {
                        self.seen.push(ev);
                    }
                    return out;
                }
                Some(ev) = self.events.recv() => {
                    if let AppEvent::HostKeyPrompt { prompt, .. } = &ev {
                        self.app.host_key_respond(*prompt, accept).unwrap();
                    }
                    self.seen.push(ev);
                }
            }
        }
    }

    pub async fn output_until(&mut self, session: Uuid, needle: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !self.output.get(&session).is_some_and(|t| t.contains(needle)) {
            match tokio::time::timeout_at(deadline, self.events.recv()).await {
                Ok(Some(AppEvent::SessionOutput { session: s, data })) => {
                    self.output.entry(s).or_default().push_str(&String::from_utf8_lossy(&data));
                }
                Ok(Some(other)) => self.seen.push(other),
                _ => panic!("no {needle:?}; output so far {:?}", self.output.get(&session)),
            }
        }
    }

    pub fn prompts(&self) -> usize {
        self.seen.iter().filter(|e| matches!(e, AppEvent::HostKeyPrompt { .. })).count()
    }
}

