//! App-level connection tests against tests/docker (run with --include-ignored).

mod common;

use common::*;
use uuid::Uuid;
use wh_app::AppEvent;
use wh_core::inventory::Group;
use wh_core::model::{Host, Snippet};

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn connect_with_identity_password_and_trust_on_first_use() {
    let mut h = harness();
    let host = h.password_host("pw", 2201);
    let app = h.app.clone();
    let s = h.with_prompts(true, app.open_ssh_session(host.id, 80, 24)).await.unwrap();
    assert_eq!(h.prompts(), 1);
    match h.seen.iter().find(|e| matches!(e, AppEvent::HostKeyPrompt { .. })).unwrap() {
        AppEvent::HostKeyPrompt { host, port, fingerprint, .. } => {
            assert_eq!((host.as_str(), *port), ("127.0.0.1", 2201));
            assert!(fingerprint.starts_with("SHA256:"));
        }
        _ => unreachable!(),
    }
    h.app.session_write(s, b"echo app-$((2+3))\n").unwrap();
    h.output_until(s, "app-5").await;

    // The accepted key is remembered: no prompt the second time.
    let app = h.app.clone();
    h.with_prompts(false, app.open_ssh_session(host.id, 80, 24)).await.unwrap();
    assert_eq!(h.prompts(), 1);
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn rejecting_prompt_fails_connection() {
    let mut h = harness();
    let host = h.password_host("pw", 2201);
    let app = h.app.clone();
    let err = h.with_prompts(false, app.open_ssh_session(host.id, 80, 24)).await.unwrap_err();
    assert_eq!(err.kind, "host_key_rejected");
    assert!(h.app.known_hosts().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn changed_host_key_is_refused_without_prompt() {
    let mut h = harness();
    let host = h.password_host("pw", 2201);
    // Trust a different ed25519 key for this host.
    let wrong = fixture("id_ed25519_enc.pub").split_whitespace().take(2).collect::<Vec<_>>().join(" ");
    assert_eq!(h.app.import_known_hosts(&format!("[127.0.0.1]:2201 {wrong}\n")).unwrap(), 1);
    let app = h.app.clone();
    let err = h.with_prompts(true, app.open_ssh_session(host.id, 80, 24)).await.unwrap_err();
    assert_eq!(err.kind, "host_key_rejected");
    assert_eq!(h.prompts(), 0);
    assert!(h.seen.iter().any(|e| matches!(e, AppEvent::HostKeyChanged { port: 2201, .. })));
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn key_auth_through_inherited_jump_host_with_startup_snippet() {
    let mut h = harness();
    let ident = h.key_identity();
    let mut bastion = Host::new("bastion", "127.0.0.1").unwrap();
    bastion.settings.port = Some(2203);
    bastion.settings.identity = Some(ident.id);
    let bastion = h.app.save_host(bastion).unwrap();
    let snippet = h.app.save_snippet(Snippet { id: Uuid::new_v4(), label: "mark".into(), script: "echo started-$((7*6))".into() }).unwrap();
    // The group supplies identity, jump chain and startup snippet.
    let mut g = Group { id: Uuid::new_v4(), name: "private".into(), parent: None, settings: Default::default() };
    g.settings.identity = Some(ident.id);
    g.settings.jump_chain = Some(vec![bastion.id]);
    g.settings.startup_snippet = Some(snippet.id);
    let g = h.app.save_group(g).unwrap();
    let mut internal = Host::new("internal", "internal").unwrap();
    internal.group = Some(g.id);
    let internal = h.app.save_host(internal).unwrap();

    let app = h.app.clone();
    let s = h.with_prompts(true, app.open_ssh_session(internal.id, 80, 24)).await.unwrap();
    assert_eq!(h.prompts(), 2, "bastion and internal are both verified");
    h.output_until(s, "started-42").await;
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn missing_username_is_a_validation_error() {
    let mut h = harness();
    let mut host = Host::new("nouser", "127.0.0.1").unwrap();
    host.settings.port = Some(2201);
    let host = h.app.save_host(host).unwrap();
    let app = h.app.clone();
    assert_eq!(h.with_prompts(true, app.open_ssh_session(host.id, 80, 24)).await.unwrap_err().kind, "validation");
}

#[tokio::test]
async fn local_session_and_close() {
    let mut h = harness();
    let s = h.app.open_local_session(80, 24).unwrap();
    h.app.session_write(s, b"echo local-$((1+1))\n").unwrap();
    h.output_until(s, "local-2").await;
    h.app.session_close(s).unwrap();
    assert_eq!(h.app.session_write(s, b"x").unwrap_err().kind, "not_found");
}

#[tokio::test]
async fn broadcast_reaches_selected_sessions() {
    let mut h = harness();
    let a = h.app.open_local_session(80, 24).unwrap();
    let b = h.app.open_local_session(80, 24).unwrap();
    assert_eq!(h.app.session_broadcast(&[a, b, Uuid::new_v4()], b"echo both-$((5+5))\n"), 2);
    h.output_until(a, "both-10").await;
    h.output_until(b, "both-10").await;
}

#[test]
fn unknown_prompt_response_is_not_found() {
    let h = harness();
    assert_eq!(h.app.host_key_respond(Uuid::new_v4(), true).unwrap_err().kind, "not_found");
}

#[tokio::test]
async fn session_logs_written_when_enabled() {
    let mut h = harness();
    let logs = h.app.data_dir().join("logs");
    let s = h.app.open_local_session(80, 24).unwrap();
    h.app.session_write(s, b"echo unlogged-$((1+1))\n").unwrap();
    h.output_until(s, "unlogged-2").await;
    assert!(!logs.exists());

    h.app.save_settings(wh_app::admin::Settings { session_logs: true, ..Default::default() }).unwrap();
    let s = h.app.open_local_session(80, 24).unwrap();
    h.app.session_write(s, b"echo logged-$((3+4)); exit\n").unwrap();
    h.output_until(s, "logged-7").await;
    h.app.session_close(s).ok();
    let mut text = String::new();
    for _ in 0..100 {
        text = std::fs::read_dir(&logs).unwrap().filter_map(|e| std::fs::read_to_string(e.unwrap().path()).ok()).collect();
        if text.contains("logged-7") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(text.contains("logged-7"), "log was: {text:?}");
}

#[test]
fn known_hosts_can_be_imported_listed_and_deleted() {
    let h = harness();
    let key = fixture("id_ed25519.pub").split_whitespace().take(2).collect::<Vec<_>>().join(" ");
    assert_eq!(h.app.import_known_hosts(&format!("a.example {key}\nb.example {key}\n")).unwrap(), 2);
    let known = h.app.known_hosts().unwrap();
    assert_eq!(known.len(), 2);
    h.app.delete_known_host(known[0].id).unwrap();
    assert_eq!(h.app.known_hosts().unwrap().len(), 1);
    assert_eq!(h.app.delete_known_host(known[0].id).unwrap_err().kind, "not_found");
}
