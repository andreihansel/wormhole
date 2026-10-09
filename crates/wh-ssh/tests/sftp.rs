//! SFTP against tests/docker.

mod common;

use std::sync::atomic::{AtomicU32, Ordering};

use common::*;
use wh_ssh::client::{connect, Connection};
use wh_ssh::sftp::{EntryKind, SftpError};

/// A fresh, empty remote directory per test.
async fn scratch(conn: &Connection) -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = format!("/tmp/wh-sftp-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst));
    conn.exec(&format!("rm -rf {dir} && mkdir -p {dir}")).await.unwrap();
    dir
}

async fn conn() -> Connection {
    connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap()
}

fn noop() -> impl FnMut(u64, u64) + Send {
    |_, _| {}
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn home_is_login_directory() {
    let c = conn().await;
    assert_eq!(c.sftp().await.unwrap().home().await.unwrap(), "/home/wh");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn list_dir_returns_entries_with_size_mode_mtime() {
    let c = conn().await;
    let dir = scratch(&c).await;
    c.exec(&format!("cd {dir} && printf 12345 > b.txt && chmod 640 b.txt && mkdir zdir && ln -s b.txt a-link")).await.unwrap();
    let entries = c.sftp().await.unwrap().list_dir(&dir).await.unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["zdir", "a-link", "b.txt"], "dirs first, then alphabetical");
    let file = &entries[2];
    assert_eq!((file.kind, file.size, file.mode), (EntryKind::File, 5, 0o640));
    assert_eq!(file.path, format!("{dir}/b.txt"));
    assert!(file.mtime.unwrap() > 1_600_000_000);
    assert_eq!(entries[0].kind, EntryKind::Dir);
    assert_eq!(entries[1].kind, EntryKind::Symlink);
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn upload_then_download_roundtrip_bytes_equal() {
    let c = conn().await;
    let dir = scratch(&c).await;
    let sftp = c.sftp().await.unwrap();
    let local_dir = tempfile::tempdir().unwrap();
    let src = local_dir.path().join("src.bin");
    let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    std::fs::write(&src, &data).unwrap();
    let remote = format!("{dir}/up.bin");
    assert_eq!(sftp.upload(&src, &remote, &mut noop()).await.unwrap(), data.len() as u64);
    let dst = local_dir.path().join("dst.bin");
    assert_eq!(sftp.download(&remote, &dst, &mut noop()).await.unwrap(), data.len() as u64);
    assert_eq!(std::fs::read(&dst).unwrap(), data);
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn progress_callback_reports_monotonic_bytes() {
    let c = conn().await;
    let dir = scratch(&c).await;
    let sftp = c.sftp().await.unwrap();
    let local = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(local.path(), vec![1u8; 200_000]).unwrap();
    let mut seen = Vec::new();
    sftp.upload(local.path(), &format!("{dir}/p.bin"), &mut |done, total| seen.push((done, total))).await.unwrap();
    assert!(seen.len() > 1, "expected several progress updates, got {seen:?}");
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0));
    assert_eq!(*seen.last().unwrap(), (200_000, 200_000));
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn mkdir_rename_delete() {
    let c = conn().await;
    let dir = scratch(&c).await;
    let sftp = c.sftp().await.unwrap();
    sftp.mkdir(&format!("{dir}/a")).await.unwrap();
    c.exec(&format!("touch {dir}/a/inner && mkdir {dir}/a/sub && touch {dir}/a/sub/deep")).await.unwrap();
    sftp.rename(&format!("{dir}/a"), &format!("{dir}/b")).await.unwrap();
    let names: Vec<_> = sftp.list_dir(&dir).await.unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["b"]);
    sftp.remove(&format!("{dir}/b")).await.unwrap();
    assert!(sftp.list_dir(&dir).await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn errors_map_to_kinds() {
    let c = conn().await;
    let sftp = c.sftp().await.unwrap();
    assert!(matches!(sftp.list_dir("/root").await, Err(SftpError::PermissionDenied(_))));
    assert!(matches!(sftp.list_dir("/does/not/exist").await, Err(SftpError::NotFound(_))));
    let local = tempfile::tempdir().unwrap();
    let err = sftp.download("/nope.bin", &local.path().join("x"), &mut noop()).await.unwrap_err();
    assert!(matches!(err, SftpError::NotFound(_)), "{err:?}");
    assert!(!local.path().join("x").exists(), "failed download must not leave a file");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn recursive_upload_directory() {
    let c = conn().await;
    let dir = scratch(&c).await;
    let sftp = c.sftp().await.unwrap();
    let local = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(local.path().join("a/b")).unwrap();
    std::fs::write(local.path().join("top.txt"), "t").unwrap();
    std::fs::write(local.path().join("a/b/deep.txt"), "deep").unwrap();
    let remote = format!("{dir}/tree");
    assert_eq!(sftp.upload_dir(local.path(), &remote).await.unwrap(), 2);
    let out = c.exec(&format!("cat {remote}/a/b/deep.txt {remote}/top.txt")).await.unwrap();
    assert_eq!(out.stdout, b"deept");
}
