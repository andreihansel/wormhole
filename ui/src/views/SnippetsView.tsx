import { useEffect, useState, type FormEvent } from "react";
import {
  api,
  errorMessage,
  type Id,
  type Snippet,
  type SnippetRun,
} from "../api";
import { Braces, Play, Plus } from "lucide-react";
import { Button, EmptyState, Overlay, PageHeader } from "../components/ui";
import Confirm from "../components/Confirm";
import { useStore } from "../store";

function SnippetForm({
  snippet,
  onDone,
}: {
  snippet: Snippet | null;
  onDone: () => void;
}) {
  const refresh = useStore((s) => s.refresh);
  const [draftId] = useState(() => snippet?.id ?? crypto.randomUUID());
  const [label, setLabel] = useState(snippet?.label ?? "");
  const [script, setScript] = useState(snippet?.script ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.saveSnippet({
        id: draftId,
        label: label.trim() || "Snippet",
        script,
      });
      await refresh();
      onDone();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!snippet || busy) return;
    await api.deleteSnippet(snippet.id);
    await refresh();
    onDone();
  }

  return (
    <form aria-label="Snippet" className="panel stack" onSubmit={submit}>
      <label>
        Name
        <input
          disabled={busy}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
      </label>
      <label>
        Script
        <textarea
          disabled={busy}
          rows={6}
          className="mono"
          value={script}
          onChange={(e) => setScript(e.target.value)}
          placeholder="systemctl restart {{service}}"
        />
      </label>
      <p className="muted small">
        Use {"{{name}}"} for values you'll be asked for when running.
      </p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <div className="row end">
        {snippet && (
          <button
            type="button"
            className="ghost danger"
            disabled={busy}
            onClick={() => setConfirm(true)}
          >
            Delete
          </button>
        )}
        <span className="grow" />
        <button type="button" disabled={busy} onClick={onDone}>
          Cancel
        </button>
        <Button className="primary" busy={busy}>
          {busy ? "Saving…" : "Save"}
        </Button>
      </div>
      {confirm && snippet && (
        <Confirm
          title={`Delete ${snippet.label}?`}
          onConfirm={remove}
          onCancel={() => setConfirm(false)}
        />
      )}
    </form>
  );
}

function RunDialog({
  snippet,
  onClose,
}: {
  snippet: Snippet;
  onClose: () => void;
}) {
  const hosts = useStore((s) => s.inventory.hosts);
  const [vars, setVars] = useState<string[] | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<Id[]>([]);
  const [results, setResults] = useState<SnippetRun[] | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    void api
      .snippetVariables(snippet.script)
      .then((v) => {
        if (mounted) setVars(v);
      })
      .catch((e) => {
        if (mounted) setError(errorMessage(e));
      });
    return () => {
      mounted = false;
    };
  }, [snippet.script]);

  const ready =
    vars !== null &&
    selected.length > 0 &&
    vars.every((v) => values[v]?.length);
  const title = `Run ${snippet.label}`;

  async function run() {
    if (running) return;
    setRunning(true);
    setError(null);
    try {
      setResults(await api.runSnippet(snippet.script, values, selected));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setRunning(false);
    }
  }

  const label = (id: Id) => hosts.find((h) => h.id === id)?.label ?? id;

  return (
    <Overlay
      title={title}
      wide
      onDismiss={() => {
        if (!running) onClose();
      }}
    >
      <pre className="code">{snippet.script}</pre>
      {vars?.map((v) => (
        <label key={v}>
          {v}
          <input
            disabled={running}
            value={values[v] ?? ""}
            onChange={(e) => setValues({ ...values, [v]: e.target.value })}
          />
        </label>
      ))}
      <fieldset className="host-selector">
        <legend>Hosts</legend>
        <div className="host-options">
          {hosts.map((h) => (
            <label key={h.id} className="check">
              <input
                disabled={running}
                type="checkbox"
                aria-label={h.label}
                checked={selected.includes(h.id)}
                onChange={(e) =>
                  setSelected(
                    e.target.checked
                      ? [...selected, h.id]
                      : selected.filter((x) => x !== h.id),
                  )
                }
              />
              {h.label}
            </label>
          ))}
        </div>
      </fieldset>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {results && (
        <ul aria-label="Results" className="results">
          {results.map((r) => (
            <li
              key={r.host}
              aria-label={label(r.host)}
              className={r.error || r.exit_status ? "result bad" : "result ok"}
            >
              <strong>{label(r.host)}</strong>{" "}
              <span className="muted small">
                {r.error ? r.error.message : `exit ${r.exit_status ?? "?"}`}
              </span>
              {(r.stdout || r.stderr) && (
                <pre className="code">{r.stdout + r.stderr}</pre>
              )}
            </li>
          ))}
        </ul>
      )}
      <div className="row end">
        <button disabled={running} onClick={onClose}>
          Close
        </button>
        <Button
          icon={Play}
          className="primary"
          busy={running}
          disabled={!ready}
          onClick={run}
        >
          {running ? "Running…" : "Run"}
        </Button>
      </div>
    </Overlay>
  );
}

export default function SnippetsView() {
  const snippets = useStore((s) => s.inventory.snippets);
  const [editing, setEditing] = useState<Snippet | "new" | null>(null);
  const [running, setRunning] = useState<Snippet | null>(null);

  return (
    <section className="content">
      <PageHeader
        title="Snippets"
        description="Useful commands, ready for repeat flights."
        count={snippets.length}
        actions={
          <Button
            icon={Plus}
            className="primary"
            onClick={() => setEditing("new")}
          >
            New snippet
          </Button>
        }
      />
      {editing && (
        <Overlay
          title={editing === "new" ? "New snippet" : `Edit ${editing.label}`}
          wide
          onDismiss={() => setEditing(null)}
        >
          <SnippetForm
            snippet={editing === "new" ? null : editing}
            onDone={() => setEditing(null)}
          />
        </Overlay>
      )}
      {snippets.length === 0 && (
        <EmptyState icon={Braces} title="Keep your best commands close.">
          Save commands you run often, then run them on one or many hosts.
        </EmptyState>
      )}
      <div className="resource-list">
        {snippets.map((s) => (
          <article key={s.id} aria-label={s.label} className="card">
            <h3>{s.label}</h3>
            <pre className="code small">{s.script}</pre>
            <div className="row end">
              <button
                className="ghost"
                aria-label={`Edit ${s.label}`}
                onClick={() => setEditing(s)}
              >
                Edit
              </button>
              <button
                className="primary"
                aria-label={`Run ${s.label}`}
                onClick={() => setRunning(s)}
              >
                Run
              </button>
            </div>
          </article>
        ))}
      </div>
      {running && (
        <RunDialog snippet={running} onClose={() => setRunning(null)} />
      )}
    </section>
  );
}
