//! Integration tests against the Docker SSH targets.
//! `docker compose -f tests/docker/docker-compose.yml up -d --build`
//! then `cargo test -p wh-ssh -- --include-ignored`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use wh_ssh::client::{connect, ConnectError, ConnectOptions, ShellEvent, ShellRequest};

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn connect_with_password() {
    let policy = accept();
    let conn = connect(&[], &password("127.0.0.1", PW_HOST, "whpass"), policy.clone(), &opts()).await.unwrap();
    let out = conn.exec("echo hi").await.unwrap();
    assert_eq!(out.stdout, b"hi\n");
    assert_eq!(out.exit_status, Some(0));
    let seen = policy.0.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].0.as_str(), seen[0].1), ("127.0.0.1", PW_HOST));
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn connect_with_ed25519_key() {
    let conn = connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap();
    assert_eq!(conn.exec("whoami").await.unwrap().stdout, b"wh\n");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn connect_with_encrypted_key_after_import() {
    let k = wh_ssh::keys::import_private(&fixture("id_ed25519_enc"), Some("fixture-pass")).unwrap();
    let mut ep = key("127.0.0.1", KEY_HOST);
    ep.credentials.key = Some(Arc::new(wh_ssh::keys::load_stored(&k.private_openssh).unwrap()));
    let conn = connect(&[], &ep, accept(), &opts()).await.unwrap();
    assert_eq!(conn.exec("echo ok").await.unwrap().stdout, b"ok\n");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn falls_back_to_password_when_key_rejected() {
    let k = wh_ssh::keys::generate(wh_ssh::keys::KeyType::Ed25519, "unknown").unwrap();
    let mut ep = password("127.0.0.1", PW_HOST, "whpass");
    ep.credentials.key = Some(Arc::new(wh_ssh::keys::load_stored(&k.private_openssh).unwrap()));
    let conn = connect(&[], &ep, accept(), &opts()).await.unwrap();
    assert_eq!(conn.exec("echo ok").await.unwrap().stdout, b"ok\n");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn auth_failure_reports_auth_error() {
    let err = connect(&[], &password("127.0.0.1", PW_HOST, "wrong"), accept(), &opts()).await.err().unwrap();
    assert!(matches!(err, ConnectError::AuthFailed(ref h) if h == "127.0.0.1"), "{err:?}");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn host_key_rejected_by_policy() {
    let err = connect(&[], &password("127.0.0.1", PW_HOST, "whpass"), Arc::new(RejectAll), &opts()).await.err().unwrap();
    assert!(matches!(err, ConnectError::HostKeyRejected(_)), "{err:?}");
}

#[tokio::test]
async fn timeout_when_server_never_speaks_ssh() {
    // Accepts TCP but never sends an SSH banner.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _held = listener.accept().await;
        tokio::time::sleep(Duration::from_secs(30)).await;
    });
    let opts = ConnectOptions { timeout: Duration::from_millis(300), keepalive: None };
    let err = connect(&[], &password("127.0.0.1", port, "x"), accept(), &opts).await.err().unwrap();
    assert!(matches!(err, ConnectError::Timeout(_)), "{err:?}");
}

#[tokio::test]
async fn connection_refused_is_network_error() {
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let err = connect(&[], &password("127.0.0.1", port, "x"), accept(), &opts()).await.err().unwrap();
    assert!(matches!(err, ConnectError::Network { .. }), "{err:?}");
}

async fn read_until(shell: &mut wh_ssh::client::Shell, needle: &str) -> String {
    let mut seen = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !seen.contains(needle) {
        match tokio::time::timeout_at(deadline, shell.read()).await {
            Ok(Some(ShellEvent::Output(b))) => seen.push_str(&String::from_utf8_lossy(&b)),
            other => panic!("no {needle:?} in {seen:?}; got {other:?}"),
        }
    }
    seen
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn shell_echoes_command_output() {
    let conn = connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap();
    let mut shell = conn.open_shell(ShellRequest { cols: 80, rows: 24, env: vec![] }).await.unwrap();
    shell.write(b"echo $((40+2))\n").await.unwrap();
    read_until(&mut shell, "42").await;
    shell.write(b"exit 3\n").await.unwrap();
    loop {
        match shell.read().await {
            Some(ShellEvent::Exit(code)) => break assert_eq!(code, 3),
            Some(_) => continue,
            None => panic!("closed without exit status"),
        }
    }
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn resize_pty_changes_stty_size() {
    let conn = connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap();
    let mut shell = conn.open_shell(ShellRequest { cols: 80, rows: 24, env: vec![] }).await.unwrap();
    shell.write(b"stty size\n").await.unwrap();
    read_until(&mut shell, "24 80").await;
    shell.resize(132, 50).await.unwrap();
    shell.write(b"stty size\n").await.unwrap();
    read_until(&mut shell, "50 132").await;
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn env_vars_sent_when_server_accepts() {
    let conn = connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap();
    let env = vec![("WH_GREETING".into(), "hello-env".into())];
    let mut shell = conn.open_shell(ShellRequest { cols: 80, rows: 24, env }).await.unwrap();
    shell.write(b"echo \"[$WH_GREETING]\"\n").await.unwrap();
    read_until(&mut shell, "[hello-env]").await;
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn jump_chain_reaches_internal_via_bastion() {
    let policy = accept();
    let jumps = [key("127.0.0.1", BASTION)];
    let conn = connect(&jumps, &key("internal", 22), policy.clone(), &opts()).await.unwrap();
    let host = String::from_utf8(conn.exec("hostname").await.unwrap().stdout).unwrap();
    assert!(!host.trim().is_empty());
    // Host keys are verified for every hop.
    let hosts: Vec<_> = policy.0.lock().unwrap().iter().map(|(h, p, _)| format!("{h}:{p}")).collect();
    assert_eq!(hosts, ["127.0.0.1:2203", "internal:22"]);
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn two_hop_chain() {
    let jumps = [key("127.0.0.1", BASTION), key("internal", 22)];
    let conn = connect(&jumps, &password("internal2", 22, "whpass"), accept(), &opts()).await.unwrap();
    assert_eq!(conn.exec("echo deep").await.unwrap().stdout, b"deep\n");
}
