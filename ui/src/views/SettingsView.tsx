import { save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState, type FormEvent } from "react";
import { api, errorMessage, type ImportSummary, type Settings } from "../api";
import { Download, Upload, ShieldCheck } from "lucide-react";
import { Button, PageHeader, PasswordField } from "../components/ui";
import { useStore } from "../store";

function ImportSection() {
  const refresh = useStore((s) => s.refresh);
  const [busy, setBusy] = useState(false);
  const [summary, setSummary] = useState<ImportSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function run() {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      setSummary(await api.importSshConfig());
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel stack">
      <h3>Import</h3>
      <p className="muted small">
        Adds hosts from your OpenSSH config, following Include and ProxyJump,
        and imports unencrypted IdentityFile keys.
      </p>
      <div className="row">
        <Button icon={Upload} busy={busy} onClick={run}>
          {busy ? "Importing…" : "Import ~/.ssh/config"}
        </Button>
      </div>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {summary && (
        <div role="status" className="ok">
          {summary.created} hosts added, {summary.updated} updated,{" "}
          {summary.keys_imported} keys imported.
          {summary.warnings.length > 0 && (
            <ul className="warnings">
              {summary.warnings.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

function ExportSection() {
  const [busy, setBusy] = useState(false);
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setStatus(null);
    if (busy) return;
    setBusy(true);
    try {
      const path = await save({
        defaultPath: "wormhole-backup.whx",
        filters: [{ name: "Wormhole export", extensions: ["whx"] }],
      });
      if (!path) return;
      await api.vaultExport(path, password);
      setStatus(`Exported to ${path}`);
      setPassword("");
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form aria-label="Export vault" className="panel stack" onSubmit={submit}>
      <h3>Export vault</h3>
      <p className="muted small">
        Writes every host, key and password to one file encrypted with the
        password below. Use it to move to another machine.
      </p>
      <PasswordField
        disabled={busy}
        label="Export password"
        autoComplete="new-password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
      />
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {status && (
        <p role="status" className="ok">
          {status}
        </p>
      )}
      <div className="row">
        <Button icon={Download} busy={busy} disabled={!password}>
          {busy ? "Exporting…" : "Export…"}
        </Button>
      </div>
    </form>
  );
}

function PasswordSection() {
  const [busy, setBusy] = useState(false);
  const [old, setOld] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setDone(false);
    if (busy) return;
    if (next !== confirm) return setError("New passwords do not match");
    setBusy(true);
    try {
      await api.vaultChangePassword(old, next);
      setOld("");
      setNext("");
      setConfirm("");
      setDone(true);
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      aria-label="Change master password"
      className="panel stack"
      onSubmit={submit}
    >
      <h3>Master password</h3>
      <PasswordField
        disabled={busy}
        label="Current password"
        autoComplete="current-password"
        value={old}
        onChange={(e) => setOld(e.target.value)}
      />
      <PasswordField
        disabled={busy}
        label="New password"
        autoComplete="new-password"
        value={next}
        onChange={(e) => setNext(e.target.value)}
      />
      <PasswordField
        disabled={busy}
        label="Confirm new password"
        autoComplete="new-password"
        value={confirm}
        onChange={(e) => setConfirm(e.target.value)}
      />
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {done && (
        <p role="status" className="ok">
          Password changed.
        </p>
      )}
      <div className="row">
        <Button icon={ShieldCheck} busy={busy} disabled={!old || !next}>
          {busy ? "Changing…" : "Change password"}
        </Button>
      </div>
    </form>
  );
}

export default function SettingsView() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    void api
      .getSettings()
      .then(setSettings)
      .catch((e) => setError(errorMessage(e)));
  }, []);

  async function saveSettings() {
    if (!settings || busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.saveSettings(settings);
      setSaved(true);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="content settings">
      <PageHeader
        title="Settings"
        description="Make this workspace yours. Keep your vault secure."
      />
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {settings && (
        <div className="panel stack">
          <h3>Security & logging</h3>
          <label>
            Auto-lock after (minutes)
            <input
              disabled={busy}
              type="number"
              min={0}
              value={settings.auto_lock_minutes}
              onChange={(e) => {
                setSaved(false);
                setSettings({
                  ...settings,
                  auto_lock_minutes: Math.max(0, Number(e.target.value) || 0),
                });
              }}
            />
          </label>
          <p className="muted small">
            0 disables auto-lock. Applies after the next unlock.
          </p>
          <label className="check">
            <input
              disabled={busy}
              type="checkbox"
              checked={settings.session_logs}
              onChange={(e) => {
                setSaved(false);
                setSettings({ ...settings, session_logs: e.target.checked });
              }}
            />
            Record session logs (terminal output is saved unencrypted in the app
            data folder)
          </label>
          <div className="row">
            <Button busy={busy} className="primary" onClick={saveSettings}>
              {busy ? "Saving…" : "Save settings"}
            </Button>
            {saved && <span className="ok small">Saved</span>}
          </div>
        </div>
      )}
      <ImportSection />
      <ExportSection />
      <PasswordSection />
    </section>
  );
}
