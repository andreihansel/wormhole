//! Terminal backends adapted to `TerminalIo`.

use std::io::{Read, Write};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::mpsc;
use wh_ssh::client::ShellEvent;

use crate::manager::{TermInput, TermOutput, TerminalIo};

/// Bridges an SSH PTY shell to the session manager.
pub fn ssh(shell: wh_ssh::client::Shell) -> TerminalIo {
    let (in_tx, mut in_rx) = mpsc::unbounded_channel();
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let (mut reader, writer) = shell.split();
    tokio::spawn(async move {
        while let Some(ev) = reader.read().await {
            let msg = match ev {
                ShellEvent::Output(d) => TermOutput::Data(d),
                ShellEvent::Exit(c) => TermOutput::Exit(c),
            };
            if out_tx.send(msg).is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Some(msg) = in_rx.recv().await {
            let ok = match msg {
                TermInput::Data(d) => writer.write(&d).await.is_ok(),
                TermInput::Resize { cols, rows } => writer.resize(cols, rows).await.is_ok(),
                TermInput::Close => false,
            };
            if !ok {
                break;
            }
        }
        writer.close().await;
    });
    TerminalIo { input: in_tx, output: out_rx }
}

/// Spawns `program` (default: `$SHELL`, else `/bin/sh`) in a local PTY.
pub fn local(program: Option<&str>, cols: u16, rows: u16) -> std::io::Result<TerminalIo> {
    let pair = native_pty_system()
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(std::io::Error::other)?;
    let program = program.map(String::from).or_else(|| std::env::var("SHELL").ok()).unwrap_or("/bin/sh".into());
    let mut cmd = CommandBuilder::new(program);
    cmd.env("TERM", "xterm-256color");
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(home);
    }
    let mut child = pair.slave.spawn_command(cmd).map_err(std::io::Error::other)?;
    drop(pair.slave);
    let mut killer = child.clone_killer();
    let mut reader = pair.master.try_clone_reader().map_err(std::io::Error::other)?;
    let mut writer = pair.master.take_writer().map_err(std::io::Error::other)?;
    let master = pair.master;

    let (in_tx, mut in_rx) = mpsc::unbounded_channel::<TermInput>();
    let (out_tx, out_rx) = mpsc::unbounded_channel();

    // Owns the child so the exit status is sent only after output drains.
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if out_tx.send(TermOutput::Data(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
        if let Ok(status) = child.wait() {
            let _ = out_tx.send(TermOutput::Exit(status.exit_code()));
        }
    });
    std::thread::spawn(move || {
        while let Some(msg) = in_rx.blocking_recv() {
            match msg {
                TermInput::Data(d) => {
                    if writer.write_all(&d).and_then(|_| writer.flush()).is_err() {
                        break;
                    }
                }
                TermInput::Resize { cols, rows } => {
                    let clamp = |v: u32| v.min(u16::MAX.into()) as u16;
                    let size = PtySize { rows: clamp(rows), cols: clamp(cols), pixel_width: 0, pixel_height: 0 };
                    let _ = master.resize(size);
                }
                TermInput::Close => break,
            }
        }
        let _ = killer.kill();
    });
    Ok(TerminalIo { input: in_tx, output: out_rx })
}
