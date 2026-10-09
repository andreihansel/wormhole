//! Port forwarding against tests/docker (each target runs a TCP echo server on 7777).

mod common;

use std::time::Duration;

use common::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use wh_ssh::client::connect;
use wh_ssh::forward::ForwardError;

async fn echo_roundtrip(stream: &mut TcpStream, msg: &[u8]) {
    stream.write_all(msg).await.unwrap();
    let mut buf = vec![0; msg.len()];
    tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut buf)).await.unwrap().unwrap();
    assert_eq!(buf, msg);
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn local_forward_proxies_tcp() {
    let conn = connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap();
    let fwd = conn.forward_local("127.0.0.1", 0, "localhost", 7777).await.unwrap();
    let addr = fwd.local_addr().unwrap();
    // Two concurrent clients through one forward.
    let mut a = TcpStream::connect(addr).await.unwrap();
    let mut b = TcpStream::connect(addr).await.unwrap();
    echo_roundtrip(&mut a, b"ping-a").await;
    echo_roundtrip(&mut b, b"ping-b").await;
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn dynamic_forward_speaks_socks5() {
    let conn = connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap();
    let fwd = conn.forward_dynamic("127.0.0.1", 0).await.unwrap();
    let mut s = TcpStream::connect(fwd.local_addr().unwrap()).await.unwrap();
    s.write_all(&[5, 1, 0]).await.unwrap();
    let mut method = [0; 2];
    s.read_exact(&mut method).await.unwrap();
    assert_eq!(method, [5, 0]);
    let mut req = vec![5, 1, 0, 3, 9];
    req.extend_from_slice(b"localhost");
    req.extend_from_slice(&7777u16.to_be_bytes());
    s.write_all(&req).await.unwrap();
    let mut reply = [0; 10];
    s.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply[1], 0, "SOCKS reply should be success");
    echo_roundtrip(&mut s, b"through-socks").await;
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn remote_forward_exposes_local_port_on_server() {
    // Local echo server the remote side should reach.
    let local = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_port = local.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = local.accept().await.unwrap();
            tokio::spawn(async move {
                let (mut r, mut w) = s.split();
                let _ = tokio::io::copy(&mut r, &mut w).await;
            });
        }
    });
    let conn = connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap();
    let fwd = conn.forward_remote("127.0.0.1", 9911, "127.0.0.1", local_port).await.unwrap();
    assert_eq!(fwd.listen, "127.0.0.1:9911");
    let out = conn.exec("printf hello-remote | socat -t2 - TCP:127.0.0.1:9911").await.unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hello-remote");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn forward_bind_conflict_reports_error() {
    let taken = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = taken.local_addr().unwrap().port();
    let conn = connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap();
    let err = conn.forward_local("127.0.0.1", port, "localhost", 7777).await.err().unwrap();
    assert!(matches!(err, ForwardError::Bind { .. }), "{err:?}");
}

#[tokio::test]
#[ignore = "needs tests/docker running"]
async fn stopping_forward_closes_listener() {
    let conn = connect(&[], &key("127.0.0.1", KEY_HOST), accept(), &opts()).await.unwrap();
    let fwd = conn.forward_local("127.0.0.1", 0, "localhost", 7777).await.unwrap();
    let addr = fwd.local_addr().unwrap();
    fwd.stop().await;
    assert!(TcpStream::connect(addr).await.is_err());
}
