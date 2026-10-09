import { useEffect, useRef, useState } from "react";
import { Columns2, Radio, SquareTerminal, X } from "lucide-react";
import { EmptyState, Tip } from "../components/ui";
import { api, errorMessage, type CloseReason, type Id } from "../api";
import { attachOutput, subscribe } from "../events";
import { useStore, type Tab } from "../store";
import { createTerm } from "../term";

function describe(reason: CloseReason): string {
  switch (reason.kind) {
    case "exit":
      return `exit ${reason.code}`;
    case "disconnected":
      return "connection lost";
    case "closed_by_user":
      return "closed";
  }
}

interface PaneProps {
  tab: Tab;
  visible: boolean;
  /** Called with typed input; decides between write and broadcast. */
  onInput: (session: Id, data: string) => void;
}

function TerminalPane({ tab, visible, onInput }: PaneProps) {
  const el = useRef<HTMLDivElement>(null);
  const term = useRef<ReturnType<typeof createTerm> | null>(null);
  const currentView = useStore((s) => s.view);
  const input = useRef(onInput);
  input.current = onInput;

  useEffect(() => {
    const t = createTerm(el.current!);
    term.current = t;
    t.onData((d) => input.current(tab.session, d));
    t.onResize(
      (cols, rows) =>
        void api.sessionResize(tab.session, cols, rows).catch(() => {}),
    );
    const detach = attachOutput(tab.session, (d) => t.write(d));
    const onWindowResize = () => t.fit();
    window.addEventListener("resize", onWindowResize);
    const observer = new ResizeObserver(() => t.fit());
    observer.observe(el.current!);
    return () => {
      window.removeEventListener("resize", onWindowResize);
      observer.disconnect();
      detach();
      t.dispose();
    };
  }, [tab.session]);

  useEffect(() => {
    if (visible && currentView === "terminals") {
      term.current?.fit();
      term.current?.focus();
    }
  }, [visible, currentView]);

  return (
    <div
      role="tabpanel"
      aria-label={tab.title}
      hidden={!visible}
      className="pane"
    >
      <div className="pane-title">{tab.title}</div>
      <div className="term" ref={el} />
      {tab.closed && (
        <div role="status" className="ended">
          Session ended ({tab.reason})
        </div>
      )}
    </div>
  );
}

export default function TerminalsView() {
  const { tabs, activeTab, setActiveTab, closeTab, openTab, markClosed } =
    useStore();
  const [split, setSplit] = useState(false);
  const [broadcast, setBroadcast] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(
    () =>
      subscribe((e) => {
        if (e.type === "session_closed")
          markClosed(e.session, describe(e.reason));
      }),
    [markClosed],
  );

  function onInput(session: Id, data: string) {
    const live = useStore.getState().tabs.filter((t) => !t.closed);
    if (!live.some((t) => t.session === session)) return;
    if (broadcast)
      void api.sessionBroadcast(
        live.map((t) => t.session),
        data,
      );
    else void api.sessionWrite(session, data).catch(() => {});
  }

  async function close(tab: Tab) {
    if (!tab.closed) await api.sessionClose(tab.session).catch(() => {});
    closeTab(tab.session);
  }

  async function newLocal() {
    setError(null);
    try {
      const session = await api.openLocalSession(80, 24);
      openTab({ session, title: "Local", host: null });
    } catch (e) {
      setError(errorMessage(e));
    }
  }

  return (
    <div className="terminals">
      <header className="tabbar">
        <div role="tablist" aria-label="Sessions" className="tabs">
          {tabs.map((t) => (
            <div
              key={t.session}
              className={t.session === activeTab ? "tab active" : "tab"}
            >
              <button
                role="tab"
                aria-selected={t.session === activeTab}
                onClick={() => setActiveTab(t.session)}
              >
                <span className={t.closed ? "dot off" : "dot on"} aria-hidden />
                {t.title}
              </button>
              <Tip label={`Close ${t.title}`}>
                <button
                  className="tab-close"
                  aria-label={`Close ${t.title}`}
                  onClick={() => close(t)}
                >
                  <X size={14} aria-hidden="true" />
                </button>
              </Tip>
            </div>
          ))}
        </div>
        <div className="row terminal-tools">
          <button
            role="switch"
            aria-checked={split}
            className={split ? "toggle on" : "toggle"}
            onClick={() => setSplit(!split)}
          >
            <Columns2 size={15} aria-hidden="true" />
            Split view
          </button>
          <button
            role="switch"
            aria-checked={broadcast}
            className={broadcast ? "toggle on warn" : "toggle"}
            onClick={() => setBroadcast(!broadcast)}
            title="Send keystrokes to every open session"
          >
            <Radio size={15} aria-hidden="true" />
            Broadcast input
          </button>
          <button onClick={newLocal}>
            <SquareTerminal size={15} aria-hidden="true" />
            New local terminal
          </button>
        </div>
      </header>
      {broadcast && (
        <div className="broadcast-note" role="status">
          Broadcast input is on. Keystrokes go to every live session.
        </div>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {tabs.length === 0 ? (
        <EmptyState
          icon={SquareTerminal}
          title="Ready when you are."
          action={<button onClick={newLocal}>Open local terminal</button>}
        >
          Connect to a host or open a local terminal. Your sessions will appear
          here.
        </EmptyState>
      ) : (
        <div
          className={
            split ? `panes grid n${Math.min(tabs.length, 4)}` : "panes"
          }
        >
          {tabs.map((t) => (
            <TerminalPane
              key={t.session}
              tab={t}
              visible={split || t.session === activeTab}
              onInput={onInput}
            />
          ))}
        </div>
      )}
    </div>
  );
}
