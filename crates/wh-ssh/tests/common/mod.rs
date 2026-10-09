//! Helpers for integration tests against tests/docker (run with WH_DOCKER=1).
#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::PublicKey;
use wh_ssh::client::{BoxFuture, ConnectOptions, Credentials, Endpoint, HostKeyPolicy};
use wh_ssh::keys;
use zeroize::Zeroizing;

pub const PW_HOST: u16 = 2201;
pub const KEY_HOST: u16 = 2202;
pub const BASTION: u16 = 2203;

pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../tests/fixtures/keys/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

pub fn password(host: &str, port: u16, pw: &str) -> Endpoint {
    Endpoint {
        host: host.into(),
        port,
        credentials: Credentials {
            username: "wh".into(),
            password: Some(Zeroizing::new(pw.into())),
            ..Default::default()
        },
    }
}

pub fn key(host: &str, port: u16) -> Endpoint {
    let k = keys::import_private(&fixture("id_ed25519"), None).unwrap();
    Endpoint {
        host: host.into(),
        port,
        credentials: Credentials {
            username: "wh".into(),
            key: Some(Arc::new(keys::load_stored(&k.private_openssh).unwrap())),
            ..Default::default()
        },
    }
}

pub fn opts() -> ConnectOptions {
    ConnectOptions { timeout: Duration::from_secs(10), keepalive: None }
}

/// Accepts every key and records what it was asked about.
#[derive(Default)]
pub struct Recorder(pub Mutex<Vec<(String, u16, String)>>);

impl HostKeyPolicy for Recorder {
    fn verify(&self, host: String, port: u16, key: PublicKey) -> BoxFuture<bool> {
        self.0.lock().unwrap().push((host, port, key.algorithm().to_string()));
        Box::pin(async { true })
    }
}

pub struct RejectAll;

impl HostKeyPolicy for RejectAll {
    fn verify(&self, _: String, _: u16, _: PublicKey) -> BoxFuture<bool> {
        Box::pin(async { false })
    }
}

pub fn accept() -> Arc<Recorder> {
    Arc::new(Recorder::default())
}
