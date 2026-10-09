import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, type VaultStatus } from "./api";
import {
  FolderKey,
  SquareTerminal,
  KeyRound,
  Braces,
  Cable,
  Files,
  ShieldCheck,
  Settings2,
  LockKeyhole,
  HardDrive,
  X,
  type LucideIcon,
} from "lucide-react";
import { BrandMark, Feedback, IconButton, Tip } from "./components/ui";
import HostKeyDialog from "./components/HostKeyDialog";
import { startEvents } from "./events";
import { useStore, type View } from "./store";
import ForwardsView from "./views/ForwardsView";
import HostsView from "./views/HostsView";
import KeychainView from "./views/KeychainView";
import KnownHostsView from "./views/KnownHostsView";
import LockScreen from "./views/LockScreen";
import SettingsView from "./views/SettingsView";
import SftpView from "./views/SftpView";
import SnippetsView from "./views/SnippetsView";
import TerminalsView from "./views/TerminalsView";

const NAV: {
  name: string;
  items: { view: View; label: string; icon: LucideIcon }[];
}[] = [
  {
    name: "Connections",
    items: [
      { view: "hosts", label: "Hosts", icon: FolderKey },
      { view: "terminals", label: "Terminals", icon: SquareTerminal },
      { view: "forwards", label: "Port forwarding", icon: Cable },
      { view: "sftp", label: "SFTP", icon: Files },
    ],
  },
  {
    name: "Resources",
    items: [
      { view: "keychain", label: "Keychain", icon: KeyRound },
      { view: "snippets", label: "Snippets", icon: Braces },
    ],
  },
  {
    name: "Security",
    items: [
      { view: "known_hosts", label: "Known hosts", icon: ShieldCheck },
      { view: "settings", label: "Settings", icon: Settings2 },
    ],
  },
];

/** Locks the vault after `minutes` without keyboard or pointer activity. */
function useIdleLock(minutes: number, enabled: boolean, onLock: () => void) {
  useEffect(() => {
    if (!enabled || minutes <= 0) return;
    let timer = setTimeout(onLock, minutes * 60_000);
    const reset = () => {
      clearTimeout(timer);
      timer = setTimeout(onLock, minutes * 60_000);
    };
    const events = ["keydown", "mousedown", "mousemove", "wheel"] as const;
    events.forEach((e) => window.addEventListener(e, reset, { passive: true }));
    return () => {
      clearTimeout(timer);
      events.forEach((e) => window.removeEventListener(e, reset));
    };
  }, [minutes, enabled, onLock]);
}

export default function App() {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [autoLock, setAutoLock] = useState(0);
  const { view, setView, refresh, tabs } = useStore();

  useEffect(() => {
    // startEvents is async: if we're cleaned up before it resolves (StrictMode
    // remounts do exactly that), stop the listener as soon as it arrives.
    let stop: (() => void) | undefined;
    let cancelled = false;
    void startEvents().then((s) => (cancelled ? s() : (stop = s)));
    void api
      .vaultStatus()
      .then(setStatus)
      .catch((e) => setError(errorMessage(e)));
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  const unlocked = status?.unlocked ?? false;

  useEffect(() => {
    if (!unlocked) return;
    let cancelled = false;
    setLoading(true);
    void Promise.all([
      refresh(),
      api.getSettings().then((s) => {
        if (!cancelled) setAutoLock(s.auto_lock_minutes);
      }),
    ])
      .then(() => {
        if (!cancelled) setError(null);
      })
      .catch((e) => {
        if (!cancelled) setError(errorMessage(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [unlocked, refresh]);

  const lock = useCallback(() => {
    void api
      .vaultLock()
      .then(() => {
        setError(null);
        setStatus((s) => (s ? { ...s, unlocked: false } : s));
      })
      .catch((e) => setError(errorMessage(e)));
  }, []);
  useIdleLock(autoLock, unlocked, lock);

  if (!status)
    return (
      <div className="app-load">
        <BrandMark />
        {error ? (
          <>
            <Feedback>{error}</Feedback>
            <button onClick={() => window.location.reload()}>Retry</button>
          </>
        ) : (
          <p role="status" className="muted">
            Opening your vault…
          </p>
        )}
      </div>
    );
  if (!status.unlocked) {
    return (
      <LockScreen
        initialized={status.initialized}
        onUnlocked={() => setStatus({ initialized: true, unlocked: true })}
      />
    );
  }

  return (
    <div className="app">
      <nav aria-label="Sidebar" className="sidebar">
        <div className="brand">
          <BrandMark />
          <span>Wormhole</span>
        </div>
        {NAV.map((group) => (
          <div className="nav-section" key={group.name}>
            <div className="nav-group-label">{group.name}</div>
            {group.items.map(({ view: target, label, icon: Icon }) => (
              <Tip key={target} label={label}>
                <button
                  className={view === target ? "nav-btn active" : "nav-btn"}
                  aria-label={label}
                  aria-current={view === target ? "page" : undefined}
                  onClick={() => setView(target)}
                >
                  <Icon aria-hidden="true" />
                  <span className="nav-label">{label}</span>
                  {target === "terminals" && tabs.length > 0 && (
                    <span className="count" aria-hidden="true">
                      {tabs.length}
                    </span>
                  )}
                </button>
              </Tip>
            ))}
          </div>
        ))}
        <span className="grow" />
        <div className="sidebar-foot">
          <Tip label="Lock vault">
            <button className="nav-btn" aria-label="Lock" onClick={lock}>
              <LockKeyhole aria-hidden="true" />
              <span className="nav-label">Lock</span>
            </button>
          </Tip>
          <p>
            <HardDrive size={12} aria-hidden="true" />
            Local vault · Unlocked
          </p>
        </div>
      </nav>
      <main className="main">
        {error && (
          <div className="workspace-notice">
            <Feedback>{error}</Feedback>
            <IconButton
              label="Dismiss notification"
              icon={X}
              onClick={() => setError(null)}
            />
          </div>
        )}
        <div className="workspace">
          {view === "hosts" &&
            (loading ? (
              <div
                className="loading-page"
                role="status"
                aria-label="Loading hosts"
              >
                <div className="skeleton" />
                <div className="skeleton" />
                <div className="skeleton" />
              </div>
            ) : (
              <HostsView />
            ))}
          {/* Terminals stay mounted so sessions keep their scrollback across views. */}
          <div hidden={view !== "terminals"} className="fill">
            <TerminalsView />
          </div>
          {view === "keychain" && <KeychainView />}
          {view === "snippets" && <SnippetsView />}
          {view === "forwards" && <ForwardsView />}
          {view === "sftp" && <SftpView />}
          {view === "known_hosts" && <KnownHostsView />}
          {view === "settings" && <SettingsView />}
        </div>
      </main>
      <HostKeyDialog />
    </div>
  );
}
