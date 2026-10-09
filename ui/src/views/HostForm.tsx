import { useState, type FormEvent } from "react";
import { api, emptySettings, errorMessage, type Host } from "../api";
import { Button, Overlay, PasswordField } from "../components/ui";
import Confirm from "../components/Confirm";
import { inheritedFrom } from "../inherit";
import { useStore } from "../store";

interface Props {
  host: Host | null;
  defaultGroup: string | null;
  onClose: () => void;
}

function hint(value: unknown, from: string | undefined): string {
  return value !== undefined && value !== null && from
    ? `${String(value)} (from ${from})`
    : "";
}

export default function HostForm({ host, defaultGroup, onClose }: Props) {
  const { inventory, refresh } = useStore();
  const [draftId] = useState(() => host?.id ?? crypto.randomUUID());
  const [label, setLabel] = useState(host?.label ?? "");
  const [address, setAddress] = useState(host?.address ?? "");
  const [port, setPort] = useState(host?.settings.port?.toString() ?? "");
  const [username, setUsername] = useState(host?.settings.username ?? "");
  const [group, setGroup] = useState(host?.group ?? defaultGroup ?? "");
  const [identity, setIdentity] = useState(host?.settings.identity ?? "");
  const [password, setPassword] = useState("");
  const [tags, setTags] = useState(host?.tags.join(", ") ?? "");
  const [jumps, setJumps] = useState<string[]>(host?.settings.jump_chain ?? []);
  const [startup, setStartup] = useState(host?.settings.startup_snippet ?? "");
  const [env, setEnv] = useState(
    Object.entries(host?.settings.env ?? {})
      .map(([k, v]) => `${k}=${v}`)
      .join("\n"),
  );
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [discard, setDiscard] = useState(false);
  const [busy, setBusy] = useState(false);
  const initial = JSON.stringify([
    host?.label ?? "",
    host?.address ?? "",
    host?.settings.port?.toString() ?? "",
    host?.settings.username ?? "",
    host?.group ?? defaultGroup ?? "",
    host?.settings.identity ?? "",
    "",
    host?.tags.join(", ") ?? "",
    host?.settings.jump_chain ?? [],
    host?.settings.startup_snippet ?? "",
    Object.entries(host?.settings.env ?? {})
      .map(([k, v]) => `${k}=${v}`)
      .join("\n"),
  ]);
  const dirty =
    initial !==
    JSON.stringify([
      label,
      address,
      port,
      username,
      group,
      identity,
      password,
      tags,
      jumps,
      startup,
      env,
    ]);
  function dismiss() {
    if (busy) return;
    if (dirty) setDiscard(true);
    else onClose();
  }

  const inherited = inheritedFrom(inventory.groups, group || null);
  const otherHosts = inventory.hosts.filter((h) => h.id !== host?.id);

  async function save(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    setError(null);
    const portNum = port.trim() ? Number(port) : null;
    if (
      portNum !== null &&
      !(Number.isInteger(portNum) && portNum >= 1 && portNum <= 65535)
    ) {
      setError("Port must be between 1 and 65535");
      return;
    }
    if (!address.trim()) {
      setError("Address is required");
      return;
    }
    const envMap: Record<string, string> = {};
    for (const line of env.split("\n")) {
      const i = line.indexOf("=");
      if (i > 0) envMap[line.slice(0, i).trim()] = line.slice(i + 1);
    }
    const next: Host = {
      id: draftId,
      label: label.trim() || address.trim(),
      address: address.trim(),
      group: group || null,
      tags: tags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean),
      settings: {
        ...(host?.settings ?? emptySettings()),
        port: portNum,
        username: username.trim() || null,
        identity: identity || null,
        jump_chain: jumps.length ? jumps : null,
        startup_snippet: startup || null,
        env: envMap,
      },
    };
    setBusy(true);
    try {
      await api.saveHost(next);
      if (password) await api.setPassword(next.id, password);
      await refresh();
      onClose();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!host || busy) return;
    await api.deleteHost(host.id);
    await refresh();
    onClose();
  }

  return (
    <Overlay
      title={host ? `Edit ${host.label}` : "New host"}
      sheet
      onDismiss={dismiss}
    >
      <form aria-label="Host" onSubmit={save} className="stack">
        <p className="muted">Configure a connection in your local vault.</p>
        <h3 className="form-section">Connection</h3>
        <label>
          Label
          <input
            disabled={busy}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            placeholder="web-1"
          />
        </label>
        <label>
          Address
          <input
            disabled={busy}
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder="203.0.113.10 or host.example.com"
          />
        </label>
        <div className="row">
          <label className="grow">
            Username
            <input
              disabled={busy}
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder={hint(inherited.username, inherited.from?.username)}
            />
          </label>
          <label className="narrow">
            Port
            <input
              disabled={busy}
              inputMode="numeric"
              value={port}
              onChange={(e) => setPort(e.target.value)}
              placeholder={hint(inherited.port, inherited.from?.port) || "22"}
            />
          </label>
        </div>
        <label>
          Group
          <select
            disabled={busy}
            value={group}
            onChange={(e) => setGroup(e.target.value)}
          >
            <option value="">No group</option>
            {inventory.groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </label>
        <h3 className="form-section">Credentials</h3>
        <label>
          Identity
          <select
            disabled={busy}
            value={identity}
            onChange={(e) => setIdentity(e.target.value)}
          >
            <option value="">
              {inherited.identity
                ? `Inherited (from ${inherited.from?.identity})`
                : "None"}
            </option>
            {inventory.identities.map((i) => (
              <option key={i.id} value={i.id}>
                {i.label}
              </option>
            ))}
          </select>
        </label>
        <PasswordField
          disabled={busy}
          label="Password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder={
            host && inventory.has_password.includes(host.id)
              ? "•••••• (stored)"
              : "Optional"
          }
        />
        <label>
          Tags
          <input
            disabled={busy}
            value={tags}
            onChange={(e) => setTags(e.target.value)}
            placeholder="prod, nginx"
          />
        </label>
        <h3 className="form-section">Advanced settings</h3>
        <fieldset>
          <legend>Jump hosts</legend>
          {otherHosts.length === 0 && (
            <p className="muted small">No other hosts yet.</p>
          )}
          {otherHosts.map((h) => (
            <label key={h.id} className="check">
              <input
                disabled={busy}
                type="checkbox"
                checked={jumps.includes(h.id)}
                onChange={(e) =>
                  setJumps(
                    e.target.checked
                      ? [...jumps, h.id]
                      : jumps.filter((j) => j !== h.id),
                  )
                }
              />
              {h.label}
            </label>
          ))}
        </fieldset>
        <label>
          Startup snippet
          <select
            disabled={busy}
            value={startup}
            onChange={(e) => setStartup(e.target.value)}
          >
            <option value="">None</option>
            {inventory.snippets.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Environment variables
          <textarea
            disabled={busy}
            rows={3}
            value={env}
            onChange={(e) => setEnv(e.target.value)}
            placeholder={"LANG=en_US.UTF-8"}
          />
        </label>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <div className="row end sheet-footer">
          {host && (
            <button
              type="button"
              className="danger ghost"
              disabled={busy}
              onClick={() => setConfirmDelete(true)}
            >
              Delete
            </button>
          )}
          <span className="grow" />
          <button type="button" onClick={dismiss} disabled={busy}>
            Cancel
          </button>
          <Button type="submit" className="primary" busy={busy}>
            {busy ? "Saving…" : "Save"}
          </Button>
        </div>
      </form>
      {discard && (
        <Confirm
          title="Discard changes?"
          body="Your unsaved host changes will be lost."
          confirmLabel="Discard"
          cancelLabel="Keep editing"
          onConfirm={onClose}
          onCancel={() => setDiscard(false)}
        />
      )}
      {confirmDelete && host && (
        <Confirm
          title={`Delete ${host.label}?`}
          body="Its stored password is deleted too."
          onConfirm={remove}
          onCancel={() => setConfirmDelete(false)}
        />
      )}
    </Overlay>
  );
}
