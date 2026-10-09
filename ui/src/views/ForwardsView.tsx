import { useEffect, useState, type FormEvent } from "react";
import {
  api,
  errorMessage,
  type ForwardKind,
  type Id,
  type PortForward,
} from "../api";
import { Cable, Plus, Play, Square } from "lucide-react";
import {
  Button,
  EmptyState,
  Overlay,
  PageHeader,
  StatusBadge,
} from "../components/ui";
import Confirm from "../components/Confirm";
import { useStore } from "../store";

function RuleForm({ onDone }: { onDone: () => void }) {
  const { inventory, refresh } = useStore();
  const [draftId] = useState(() => crypto.randomUUID());
  const [label, setLabel] = useState("");
  const [kind, setKind] = useState<ForwardKind>("local");
  const [host, setHost] = useState("");
  const [bindAddress, setBindAddress] = useState("127.0.0.1");
  const [bindPort, setBindPort] = useState("");
  const [destHost, setDestHost] = useState("localhost");
  const [destPort, setDestPort] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const port = (s: string) => {
    const n = Number(s);
    return Number.isInteger(n) && n >= 0 && n <= 65535 && s.trim() !== ""
      ? n
      : null;
  };

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (saving) return;
    const bp = port(bindPort);
    const dp = kind === "dynamic" ? 0 : port(destPort);
    if (!host) return setError("Choose a host");
    if (bp === null || dp === null)
      return setError("Ports must be between 0 and 65535");
    setSaving(true);
    setError(null);
    try {
      await api.saveForward({
        id: draftId,
        label: label.trim() || `${kind} ${bp}`,
        host,
        kind,
        bind_address: bindAddress.trim() || "127.0.0.1",
        bind_port: bp,
        dest_host: kind === "dynamic" ? "" : destHost.trim(),
        dest_port: dp,
      });
      await refresh();
      onDone();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form aria-label="Port forward" className="panel stack" onSubmit={submit}>
      <div className="row">
        <label className="grow">
          Name
          <input
            disabled={saving}
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
        </label>
        <label>
          Type
          <select
            disabled={saving}
            value={kind}
            onChange={(e) => setKind(e.target.value as ForwardKind)}
          >
            <option value="local">Local (-L)</option>
            <option value="remote">Remote (-R)</option>
            <option value="dynamic">Dynamic SOCKS (-D)</option>
          </select>
        </label>
      </div>
      <label>
        Host
        <select
          disabled={saving}
          value={host}
          onChange={(e) => setHost(e.target.value)}
        >
          <option value="">Choose…</option>
          {inventory.hosts.map((h) => (
            <option key={h.id} value={h.id}>
              {h.label}
            </option>
          ))}
        </select>
      </label>
      <div className="row">
        <label className="grow">
          Bind address
          <input
            disabled={saving}
            value={bindAddress}
            onChange={(e) => setBindAddress(e.target.value)}
          />
        </label>
        <label className="narrow">
          Bind port
          <input
            disabled={saving}
            inputMode="numeric"
            value={bindPort}
            onChange={(e) => setBindPort(e.target.value)}
          />
        </label>
      </div>
      {kind !== "dynamic" && (
        <div className="row">
          <label className="grow">
            Destination host
            <input
              disabled={saving}
              value={destHost}
              onChange={(e) => setDestHost(e.target.value)}
            />
          </label>
          <label className="narrow">
            Destination port
            <input
              disabled={saving}
              inputMode="numeric"
              value={destPort}
              onChange={(e) => setDestPort(e.target.value)}
            />
          </label>
        </div>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <div className="row end">
        <button type="button" disabled={saving} onClick={onDone}>
          Cancel
        </button>
        <Button busy={saving} className="primary">
          {saving ? "Saving…" : "Save"}
        </Button>
      </div>
    </form>
  );
}

function describe(f: PortForward, hostLabel: string): string {
  const bind = `${f.bind_address}:${f.bind_port}`;
  if (f.kind === "dynamic") return `SOCKS5 on ${bind} via ${hostLabel}`;
  if (f.kind === "remote")
    return `${hostLabel} ${bind} → ${f.dest_host}:${f.dest_port} (this machine)`;
  return `${bind} → ${f.dest_host}:${f.dest_port} via ${hostLabel}`;
}

export default function ForwardsView() {
  const { inventory, refresh } = useStore();
  const [active, setActive] = useState<Record<Id, string>>({});
  const [busy, setBusy] = useState<Id | null>(null);
  const [errors, setErrors] = useState<Record<Id, string>>({});
  const [creating, setCreating] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<PortForward | null>(null);

  useEffect(() => {
    void api
      .forwardsActive()
      .then((list) =>
        setActive(Object.fromEntries(list.map((a) => [a.rule, a.listen]))),
      )
      .catch((e) => setLoadError(errorMessage(e)));
  }, []);

  async function toggle(f: PortForward) {
    if (busy) return;
    setBusy(f.id);
    setErrors(({ [f.id]: _, ...rest }) => rest);
    try {
      if (active[f.id]) {
        await api.forwardStop(f.id);
        setActive(({ [f.id]: _, ...rest }) => rest);
      } else {
        const a = await api.forwardStart(f.id);
        setActive((prev) => ({ ...prev, [f.id]: a.listen }));
      }
    } catch (e) {
      setErrors((prev) => ({ ...prev, [f.id]: errorMessage(e) }));
    } finally {
      setBusy(null);
    }
  }

  async function remove() {
    if (!deleting) return;
    if (active[deleting.id]) await api.forwardStop(deleting.id).catch(() => {});
    await api.deleteForward(deleting.id);
    setDeleting(null);
    await refresh();
  }

  const hostLabel = (id: Id) =>
    inventory.hosts.find((h) => h.id === id)?.label ?? "missing host";

  return (
    <section className="content">
      <PageHeader
        title="Port forwarding"
        description="Secure routes between your machine and your servers."
        count={inventory.forwards.length}
        actions={
          <Button
            icon={Plus}
            className="primary"
            onClick={() => setCreating(true)}
          >
            New rule
          </Button>
        }
      />
      {loadError && (
        <p role="alert" className="error">
          {loadError}
        </p>
      )}
      {creating && (
        <Overlay
          title="New forwarding rule"
          onDismiss={() => setCreating(false)}
          wide
        >
          <RuleForm onDone={() => setCreating(false)} />
        </Overlay>
      )}
      {inventory.forwards.length === 0 && (
        <EmptyState icon={Cable} title="Find a new route.">
          Tunnel local ports to your servers, expose local services remotely, or
          run a SOCKS proxy.
        </EmptyState>
      )}
      <div className="resource-list">
        {inventory.forwards.map((f) => (
          <article key={f.id} aria-label={f.label} className="card">
            <h3>{f.label}</h3>
            <p className="muted small">{describe(f, hostLabel(f.host))}</p>
            {active[f.id] && (
              <StatusBadge state="live">{`Listening on ${active[f.id]}`}</StatusBadge>
            )}
            {errors[f.id] && (
              <p role="alert" className="error small">
                {errors[f.id]}
              </p>
            )}
            <div className="row end">
              <button className="ghost danger" onClick={() => setDeleting(f)}>
                Delete
              </button>
              <Button
                icon={active[f.id] ? Square : Play}
                className={active[f.id] ? "" : "primary"}
                busy={busy === f.id}
                disabled={!!busy}
                onClick={() => toggle(f)}
              >
                {busy === f.id
                  ? active[f.id]
                    ? "Stopping…"
                    : "Starting…"
                  : active[f.id]
                    ? "Stop"
                    : "Start"}
              </Button>
            </div>
          </article>
        ))}
      </div>
      {deleting && (
        <Confirm
          title={`Delete ${deleting.label}?`}
          onConfirm={remove}
          onCancel={() => setDeleting(null)}
        />
      )}
    </section>
  );
}
