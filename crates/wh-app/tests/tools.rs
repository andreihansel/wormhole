//! Forwards, snippets across hosts and SFTP through the app, against tests/docker.

mod common;

use std::collections::BTreeMap;
use std::time::Duration;

use common::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;
use wh_app::AppEvent;
use wh_core::model::{ForwardKind, Host, PortForward};

fn local_forward(host: Uuid, dest_port: u16) -> PortForward {
    PortForward {
        id: Uuid::new_v4(),
        label: "echo".into(),
        host,
        kind: ForwardKind::Local,
        bind_address: "127.0.0.1".into(),
        bind_port: 0,
        dest_host: "localhost".into(),
        dest_port,
    }
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn forward_start_stop() {
    let mut h = harness();
    let host = h.password_host("pw", 2201);
    let rule = h.app.save_forward(local_forward(host.id, 7777)).unwrap();
    let app = h.app.clone();
    let active = h.with_prompts(true, app.forward_start(rule.id)).await.unwrap();
    assert_eq!(h.app.forwards_active().await, vec![active.clone()]);
    let mut s = tokio::net::TcpStream::connect(&active.listen).await.unwrap();
    s.write_all(b"fwd").await.unwrap();
    let mut buf = [0; 3];
    tokio::time::timeout(Duration::from_secs(5), s.read_exact(&mut buf)).await.unwrap().unwrap();
    assert_eq!(&buf, b"fwd");
    assert_eq!(h.app.forward_start(rule.id).await.unwrap_err().kind, "already_running");
    h.app.forward_stop(rule.id).await.unwrap();
    assert!(h.app.forwards_active().await.is_empty());
    assert_eq!(h.app.forward_stop(rule.id).await.unwrap_err().kind, "not_found");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn run_snippet_on_n_hosts_collects_per_host_result() {
    let mut h = harness();
    let a = h.password_host("a", 2201);
    let b = h.password_host("b", 2202);
    let mut broken = Host::new("broken", "127.0.0.1").unwrap();
    broken.settings.port = Some(1); // nothing listens here
    broken.settings.username = Some("wh".into());
    let broken = h.app.save_host(broken).unwrap();
    let vars = BTreeMap::from([("word".to_string(), "hi".to_string())]);
    let app = h.app.clone();
    let runs = h.with_prompts(true, app.run_snippet("echo {{word}}-$((1+1)); exit 3", &vars, &[a.id, broken.id, b.id])).await.unwrap();
    assert_eq!(runs.iter().map(|r| r.host).collect::<Vec<_>>(), [a.id, broken.id, b.id]);
    assert_eq!((runs[0].stdout.as_str(), runs[0].exit_status), ("hi-2\n", Some(3)));
    assert_eq!(runs[2].stdout, "hi-2\n");
    assert_eq!(runs[1].error.as_ref().unwrap().kind, "network");
}

#[tokio::test]
async fn run_snippet_with_missing_variable_fails_before_connecting() {
    let h = harness();
    let err = h.app.run_snippet("echo {{x}}", &BTreeMap::new(), &[Uuid::new_v4()]).await.unwrap_err();
    assert_eq!(err.kind, "validation");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn sftp_browse_upload_download_with_progress() {
    let mut h = harness();
    let host = h.password_host("pw", 2201);
    let app = h.app.clone();
    let sftp = h.with_prompts(true, app.sftp_open(host.id)).await.unwrap();
    let home = h.app.sftp_home(sftp).await.unwrap();
    let dir = format!("{home}/app-sftp-{}", Uuid::new_v4());
    h.app.sftp_mkdir(sftp, &dir).await.unwrap();

    let local = tempfile::tempdir().unwrap();
    let src = local.path().join("up.bin");
    std::fs::write(&src, vec![9u8; 100_000]).unwrap();
    let transfer = Uuid::new_v4();
    let n = h.app.sftp_upload(sftp, transfer, src.to_str().unwrap(), &format!("{dir}/up.bin")).await.unwrap();
    assert_eq!(n, 100_000);
    let mut last = None;
    while let Ok(ev) = h.events.try_recv() {
        if let AppEvent::TransferProgress { transfer: t, done, total } = ev {
            assert_eq!(t, transfer);
            last = Some((done, total));
        }
    }
    assert_eq!(last, Some((100_000, 100_000)));

    let names: Vec<_> = h.app.sftp_list(sftp, &dir).await.unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["up.bin"]);
    h.app.sftp_rename(sftp, &format!("{dir}/up.bin"), &format!("{dir}/moved.bin")).await.unwrap();
    let dst = local.path().join("down.bin");
    h.app.sftp_download(sftp, Uuid::new_v4(), &format!("{dir}/moved.bin"), dst.to_str().unwrap()).await.unwrap();
    assert_eq!(std::fs::read(&dst).unwrap().len(), 100_000);
    h.app.sftp_remove(sftp, &dir).await.unwrap();
    assert_eq!(h.app.sftp_list(sftp, &dir).await.unwrap_err().kind, "not_found");
    h.app.sftp_close(sftp).await.unwrap();
    assert_eq!(h.app.sftp_home(sftp).await.unwrap_err().kind, "not_found");
}

#[test]
fn local_list_sorts_dirs_first() {
    let h = harness();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("b.txt"), "12").unwrap();
    std::fs::write(dir.path().join("a.txt"), "").unwrap();
    std::fs::create_dir(dir.path().join("z")).unwrap();
    let entries = h.app.local_list(dir.path().to_str().unwrap()).unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["z", "a.txt", "b.txt"]);
    assert_eq!((entries[2].size, entries[0].is_dir), (2, true));
    assert_eq!(h.app.local_list("/definitely/missing").unwrap_err().kind, "not_found");
}
