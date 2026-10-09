use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use uuid::Uuid;

pub type SessionId = Uuid;

/// Input to a terminal backend (SSH shell or local PTY).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermInput {
    Data(Vec<u8>),
    Resize { cols: u32, rows: u32 },
    Close,
}

/// Output from a terminal backend. A closed channel means the backend went away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermOutput {
    Data(Vec<u8>),
    Exit(u32),
}

/// Channel pair connecting the manager to any terminal backend.
pub struct TerminalIo {
    pub input: mpsc::UnboundedSender<TermInput>,
    pub output: mpsc::UnboundedReceiver<TermOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloseReason {
    Exit { code: u32 },
    Disconnected,
    ClosedByUser,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    Output { id: SessionId, data: Vec<u8> },
    Closed { id: SessionId, reason: CloseReason },
}

#[derive(Debug, Clone, Default)]
pub struct SessionOptions {
    /// Raw terminal output is appended here. Input is never logged.
    pub log_path: Option<PathBuf>,
    /// Sent as input (followed by a newline) right after the session starts.
    pub startup_command: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("no such session")]
    NotFound,
}

type Sink = Arc<dyn Fn(SessionEvent) + Send + Sync>;
type Inputs = Arc<Mutex<HashMap<SessionId, mpsc::UnboundedSender<TermInput>>>>;

pub struct SessionManager {
    inputs: Inputs,
    sink: Sink,
}

impl SessionManager {
    pub fn new(sink: Sink) -> Self {
        Self { inputs: Inputs::default(), sink }
    }

    pub fn spawn(&self, io: TerminalIo, opts: SessionOptions) -> SessionId {
        let id = Uuid::new_v4();
        if let Some(cmd) = &opts.startup_command {
            let _ = io.input.send(TermInput::Data(format!("{cmd}\n").into_bytes()));
        }
        self.inputs.lock().expect("inputs lock").insert(id, io.input);
        tokio::spawn(pump(id, io.output, opts.log_path, self.inputs.clone(), self.sink.clone()));
        id
    }

    pub fn write(&self, id: SessionId, data: &[u8]) -> Result<(), SessionError> {
        self.send(id, TermInput::Data(data.to_vec()))
    }

    pub fn resize(&self, id: SessionId, cols: u32, rows: u32) -> Result<(), SessionError> {
        self.send(id, TermInput::Resize { cols, rows })
    }

    pub fn close(&self, id: SessionId) -> Result<(), SessionError> {
        let input = self.inputs.lock().expect("inputs lock").remove(&id).ok_or(SessionError::NotFound)?;
        let _ = input.send(TermInput::Close);
        (self.sink)(SessionEvent::Closed { id, reason: CloseReason::ClosedByUser });
        Ok(())
    }

    /// Sends `data` to every listed live session; returns how many received it.
    pub fn broadcast(&self, ids: &[SessionId], data: &[u8]) -> usize {
        ids.iter().filter(|&&id| self.write(id, data).is_ok()).count()
    }

    pub fn ids(&self) -> Vec<SessionId> {
        self.inputs.lock().expect("inputs lock").keys().copied().collect()
    }

    fn send(&self, id: SessionId, msg: TermInput) -> Result<(), SessionError> {
        let inputs = self.inputs.lock().expect("inputs lock");
        let tx = inputs.get(&id).ok_or(SessionError::NotFound)?;
        tx.send(msg).map_err(|_| SessionError::NotFound)
    }
}

/// Forwards backend output to the sink (and log) until the backend ends.
async fn pump(
    id: SessionId,
    mut output: mpsc::UnboundedReceiver<TermOutput>,
    log_path: Option<PathBuf>,
    inputs: Inputs,
    sink: Sink,
) {
    let mut log = match log_path {
        Some(p) => tokio::fs::OpenOptions::new().create(true).append(true).open(p).await.ok(),
        None => None,
    };
    let live = |inputs: &Inputs| inputs.lock().expect("inputs lock").contains_key(&id);
    let reason = loop {
        match output.recv().await {
            Some(TermOutput::Data(data)) => {
                if let Some(f) = log.as_mut() {
                    let _ = f.write_all(&data).await;
                }
                if live(&inputs) {
                    sink(SessionEvent::Output { id, data });
                }
            }
            Some(TermOutput::Exit(code)) => break CloseReason::Exit { code },
            None => break CloseReason::Disconnected,
        }
    };
    if let Some(f) = log.as_mut() {
        let _ = f.flush().await;
    }
    // Whoever removes the entry (here or `close`) emits the single Closed event.
    if inputs.lock().expect("inputs lock").remove(&id).is_some() {
        sink(SessionEvent::Closed { id, reason });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    /// A scripted terminal: the test plays the remote side.
    struct Fake {
        input: mpsc::UnboundedReceiver<TermInput>,
        output: mpsc::UnboundedSender<TermOutput>,
    }

    fn fake() -> (TerminalIo, Fake) {
        let (in_tx, in_rx) = mpsc::unbounded_channel();
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        (TerminalIo { input: in_tx, output: out_rx }, Fake { input: in_rx, output: out_tx })
    }

    type Events = Arc<Mutex<Vec<SessionEvent>>>;

    fn manager() -> (SessionManager, Events) {
        let events: Events = Arc::default();
        let sink = events.clone();
        (SessionManager::new(Arc::new(move |e| sink.lock().unwrap().push(e))), events)
    }

    async fn until(events: &Events, pred: impl Fn(&[SessionEvent]) -> bool) {
        for _ in 0..200 {
            if pred(&events.lock().unwrap()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("condition not met; events: {:?}", events.lock().unwrap());
    }

    async fn recv(f: &mut Fake) -> TermInput {
        tokio::time::timeout(Duration::from_secs(1), f.input.recv()).await.expect("input expected").expect("open")
    }

    #[tokio::test]
    async fn open_session_returns_unique_id() {
        let (m, _) = manager();
        let (a, _fa) = fake();
        let (b, _fb) = fake();
        let (ia, ib) = (m.spawn(a, Default::default()), m.spawn(b, Default::default()));
        assert_ne!(ia, ib);
        let mut ids = m.ids();
        ids.sort();
        let mut want = vec![ia, ib];
        want.sort();
        assert_eq!(ids, want);
    }

    #[tokio::test]
    async fn output_is_streamed_in_order() {
        let (m, events) = manager();
        let (io, f) = fake();
        let id = m.spawn(io, Default::default());
        for chunk in ["a", "b", "c"] {
            f.output.send(TermOutput::Data(chunk.into())).unwrap();
        }
        until(&events, |e| e.len() == 3).await;
        let data: Vec<_> = events
            .lock()
            .unwrap()
            .iter()
            .map(|e| match e {
                SessionEvent::Output { id: got, data } if *got == id => String::from_utf8(data.clone()).unwrap(),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(data, ["a", "b", "c"]);
    }

    #[tokio::test]
    async fn write_and_resize_reach_terminal() {
        let (m, _) = manager();
        let (io, mut f) = fake();
        let id = m.spawn(io, Default::default());
        m.write(id, b"ls\n").unwrap();
        m.resize(id, 100, 40).unwrap();
        assert_eq!(recv(&mut f).await, TermInput::Data(b"ls\n".to_vec()));
        assert_eq!(recv(&mut f).await, TermInput::Resize { cols: 100, rows: 40 });
        assert_eq!(m.write(Uuid::new_v4(), b"x"), Err(SessionError::NotFound));
    }

    #[tokio::test]
    async fn exit_emits_closed_event_and_removes_session() {
        let (m, events) = manager();
        let (io, f) = fake();
        let id = m.spawn(io, Default::default());
        f.output.send(TermOutput::Exit(7)).unwrap();
        until(&events, |e| e.iter().any(|e| matches!(e, SessionEvent::Closed { .. }))).await;
        assert!(events.lock().unwrap().contains(&SessionEvent::Closed { id, reason: CloseReason::Exit { code: 7 } }));
        until(&events, |_| m.ids().is_empty()).await;
    }

    #[tokio::test]
    async fn disconnect_emits_closed_event_with_reason() {
        let (m, events) = manager();
        let (io, f) = fake();
        let id = m.spawn(io, Default::default());
        drop(f);
        until(&events, |e| e.contains(&SessionEvent::Closed { id, reason: CloseReason::Disconnected })).await;
    }

    #[tokio::test]
    async fn close_session_frees_resources() {
        let (m, events) = manager();
        let (io, mut f) = fake();
        let id = m.spawn(io, Default::default());
        m.close(id).unwrap();
        assert_eq!(recv(&mut f).await, TermInput::Close);
        until(&events, |e| e.contains(&SessionEvent::Closed { id, reason: CloseReason::ClosedByUser })).await;
        assert!(m.ids().is_empty());
        assert_eq!(m.close(id), Err(SessionError::NotFound));
        // Exactly one Closed event even if the backend also goes away.
        drop(f);
        tokio::time::sleep(Duration::from_millis(30)).await;
        let closed = events.lock().unwrap().iter().filter(|e| matches!(e, SessionEvent::Closed { .. })).count();
        assert_eq!(closed, 1);
    }

    #[tokio::test]
    async fn broadcast_writes_to_all_members_only() {
        let (m, _) = manager();
        let (a, mut fa) = fake();
        let (b, mut fb) = fake();
        let (c, mut fc) = fake();
        let (ia, ib, _ic) = (m.spawn(a, Default::default()), m.spawn(b, Default::default()), m.spawn(c, Default::default()));
        assert_eq!(m.broadcast(&[ia, ib, Uuid::new_v4()], b"uptime\n"), 2);
        assert_eq!(recv(&mut fa).await, TermInput::Data(b"uptime\n".to_vec()));
        assert_eq!(recv(&mut fb).await, TermInput::Data(b"uptime\n".to_vec()));
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(fc.input.try_recv().is_err(), "non-member must not receive input");
    }

    #[tokio::test]
    async fn session_log_records_output_but_not_input() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.log");
        let (m, events) = manager();
        let (io, mut f) = fake();
        let id = m.spawn(io, SessionOptions { log_path: Some(log.clone()), ..Default::default() });
        m.write(id, b"hunter2\n").unwrap();
        recv(&mut f).await;
        f.output.send(TermOutput::Data(b"Password accepted\r\n".to_vec())).unwrap();
        f.output.send(TermOutput::Exit(0)).unwrap();
        until(&events, |e| e.iter().any(|e| matches!(e, SessionEvent::Closed { .. }))).await;
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("Password accepted"));
        assert!(!text.contains("hunter2"));
    }

    #[tokio::test]
    async fn startup_command_runs_after_connect() {
        let (m, _) = manager();
        let (io, mut f) = fake();
        let id = m.spawn(io, SessionOptions { startup_command: Some("cd /srv".into()), ..Default::default() });
        assert_eq!(recv(&mut f).await, TermInput::Data(b"cd /srv\n".to_vec()));
        m.write(id, b"ls\n").unwrap();
        assert_eq!(recv(&mut f).await, TermInput::Data(b"ls\n".to_vec()));
    }
}
