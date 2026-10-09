import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, type KnownHost } from "../api";
import { ShieldCheck, Upload } from "lucide-react";
import { Button, EmptyState, PageHeader } from "../components/ui";
import Confirm from "../components/Confirm";

export default function KnownHostsView() {
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [known, setKnown] = useState<KnownHost[]>([]);
  const [removing, setRemoving] = useState<KnownHost | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => setKnown(await api.knownHosts()), []);
  useEffect(() => {
    void load()
      .catch((e) => setError(errorMessage(e)))
      .finally(() => setLoading(false));
  }, [load]);

  async function remove() {
    if (!removing) return;
    await api.deleteKnownHost(removing.id);
    setRemoving(null);
    await load();
  }

  async function importFile() {
    if (busy) return;
    setBusy(true);
    setStatus(null);
    setError(null);
    try {
      const n = await api.importKnownHosts();
      setStatus(`Imported ${n} new ${n === 1 ? "entry" : "entries"}.`);
      await load();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="content">
      <PageHeader
        title="Known hosts"
        description="The servers you have chosen to trust."
        count={known.length}
        actions={
          <Button icon={Upload} busy={busy} onClick={importFile}>
            {busy ? "Importing…" : "Import ~/.ssh/known_hosts"}
          </Button>
        }
      />
      {loading && (
        <p role="status" className="muted">
          Loading trust records…
        </p>
      )}
      {status && (
        <p role="status" className="ok">
          {status}
        </p>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {!loading && known.length === 0 && (
        <EmptyState icon={ShieldCheck} title="Trust starts with verification.">
          Host keys you trust appear here. You'll be asked the first time you
          connect to a server.
        </EmptyState>
      )}
      <div role="table" aria-label="Known hosts" className="table">
        {known.map((k) => {
          const [algo, blob] = k.key.split(" ");
          return (
            <div
              key={k.id}
              role="row"
              aria-label={k.pattern}
              className="table-row"
            >
              <span role="cell" className="mono grow">
                {k.pattern.startsWith("|1|") ? "(hashed hostname)" : k.pattern}
              </span>
              <span role="cell" className="muted small">
                {algo}
              </span>
              <span role="cell" className="mono small trust-key" title={blob}>
                {blob}
              </span>
              <button className="ghost danger" onClick={() => setRemoving(k)}>
                Remove
              </button>
            </div>
          );
        })}
      </div>
      {removing && (
        <Confirm
          title={`Remove ${removing.pattern.startsWith("|1|") ? "this host" : removing.pattern}?`}
          body="You'll be asked to verify its key again on the next connection."
          confirmLabel="Remove"
          onConfirm={remove}
          onCancel={() => setRemoving(null)}
        />
      )}
    </section>
  );
}
