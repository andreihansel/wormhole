import { useEffect, useState } from "react";
import { api, errorMessage, type AppEvent } from "../api";
import { Button, Feedback, Overlay } from "./ui";
import { ShieldCheck, ShieldAlert } from "lucide-react";
import { subscribe } from "../events";

type Prompt = Extract<AppEvent, { type: "host_key_prompt" }>;
type Changed = Extract<AppEvent, { type: "host_key_changed" }>;

/** Global trust-on-first-use prompt and changed-key warning. */
export default function HostKeyDialog() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [prompts, setPrompts] = useState<Prompt[]>([]);
  const [changed, setChanged] = useState<Changed[]>([]);

  useEffect(
    () =>
      subscribe((e) => {
        if (e.type === "host_key_prompt") setPrompts((p) => [...p, e]);
        if (e.type === "host_key_changed") setChanged((c) => [...c, e]);
      }),
    [],
  );

  async function answer(p: Prompt, accept: boolean) {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.hostKeyRespond(p.prompt, accept);
      setPrompts((all) => all.filter((x) => x.prompt !== p.prompt));
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const warning = changed[0];
  if (warning) {
    return (
      <Overlay
        title="HOST KEY HAS CHANGED"
        alert
        onDismiss={() => setChanged((c) => c.slice(1))}
      >
        <ShieldAlert className="error" size={28} aria-hidden="true" />
        <p>
          <strong>
            {warning.host}:{warning.port}
          </strong>{" "}
          presented a different key than the one you trusted. Someone could be
          intercepting the connection, or the server was reinstalled. The
          connection was refused.
        </p>
        <p className="mono">{warning.fingerprint}</p>
        <p className="muted small">
          If the change is expected, remove the old entry under Known hosts and
          connect again.
        </p>
        <div className="row end">
          <button onClick={() => setChanged((c) => c.slice(1))}>Close</button>
        </div>
      </Overlay>
    );
  }

  const p = prompts[0];
  if (!p) return null;
  return (
    <Overlay
      key={p.prompt}
      title="Unknown host"
      onDismiss={() => {
        void answer(p, false);
      }}
    >
      <ShieldCheck className="accent" size={28} aria-hidden="true" />
      <p>
        The authenticity of{" "}
        <strong>
          {p.host}
          {p.port !== 22 ? `:${p.port}` : ""}
        </strong>{" "}
        can't be established. Check that this fingerprint matches the server
        before trusting it.
      </p>
      <p className="muted small">{p.algorithm}</p>
      <p className="mono">{p.fingerprint}</p>
      {error && <Feedback>{error}</Feedback>}
      <div className="row end">
        <button disabled={busy} onClick={() => answer(p, false)}>
          Cancel
        </button>
        <Button className="primary" busy={busy} onClick={() => answer(p, true)}>
          {busy ? "Responding…" : "Trust and connect"}
        </Button>
      </div>
    </Overlay>
  );
}
