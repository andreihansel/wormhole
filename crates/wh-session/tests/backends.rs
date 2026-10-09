use std::sync::Arc;
use std::time::Duration;

use wh_session::{backends, TermInput, TermOutput, TerminalIo};

async fn read_until(io: &mut TerminalIo, needle: &str) -> String {
    let mut seen = String::new();
    while !seen.contains(needle) {
        match tokio::time::timeout(Duration::from_secs(10), io.output.recv()).await {
            Ok(Some(TermOutput::Data(d))) => seen.push_str(&String::from_utf8_lossy(&d)),
            other => panic!("no {needle:?} in {seen:?}; got {other:?}"),
        }
    }
    seen
}

async fn exit_code(io: &mut TerminalIo) -> Option<u32> {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), io.output.recv()).await.expect("timed out") {
            Some(TermOutput::Exit(c)) => return Some(c),
            Some(TermOutput::Data(_)) => continue,
            None => return None,
        }
    }
}

#[tokio::test]
async fn local_terminal_spawns_shell_and_reports_exit() {
    let mut io = backends::local(Some("/bin/sh"), 80, 24).unwrap();
    io.input.send(TermInput::Data(b"echo $((6*7))\n".to_vec())).unwrap();
    read_until(&mut io, "42").await;
    io.input.send(TermInput::Resize { cols: 120, rows: 33 }).unwrap();
    io.input.send(TermInput::Data(b"stty size\n".to_vec())).unwrap();
    read_until(&mut io, "33 120").await;
    io.input.send(TermInput::Data(b"exit 5\n".to_vec())).unwrap();
    assert_eq!(exit_code(&mut io).await, Some(5));
}

#[tokio::test]
async fn local_terminal_close_ends_session() {
    let mut io = backends::local(Some("/bin/sh"), 80, 24).unwrap();
    io.input.send(TermInput::Close).unwrap();
    // The shell is gone: either an exit status or a closed channel, promptly.
    let _ = exit_code(&mut io).await;
}

mod ssh {
    use super::*;
    use wh_ssh::client::{connect, BoxFuture, ConnectOptions, Credentials, Endpoint, HostKeyPolicy, ShellRequest};

    struct AcceptAll;
    impl HostKeyPolicy for AcceptAll {
        fn verify(&self, _: String, _: u16, _: russh::keys::PublicKey) -> BoxFuture<bool> {
            Box::pin(async { true })
        }
    }

    #[tokio::test]
    #[ignore = "needs tests/docker running"]
    async fn ssh_backend_streams_output_and_exit() {
        let ep = Endpoint {
            host: "127.0.0.1".into(),
            port: 2201,
            credentials: Credentials {
                username: "wh".into(),
                password: Some(zeroize::Zeroizing::new("whpass".into())),
                ..Default::default()
            },
        };
        let conn = connect(&[], &ep, Arc::new(AcceptAll), &ConnectOptions::default()).await.unwrap();
        let shell = conn.open_shell(ShellRequest { cols: 80, rows: 24, env: vec![] }).await.unwrap();
        let mut io = backends::ssh(shell);
        io.input.send(TermInput::Data(b"echo ssh-$((1+1))\n".to_vec())).unwrap();
        read_until(&mut io, "ssh-2").await;
        io.input.send(TermInput::Resize { cols: 90, rows: 30 }).unwrap();
        io.input.send(TermInput::Data(b"stty size; exit 4\n".to_vec())).unwrap();
        read_until(&mut io, "30 90").await;
        assert_eq!(exit_code(&mut io).await, Some(4));
    }
}
