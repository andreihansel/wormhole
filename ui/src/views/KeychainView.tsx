import { useState, type FormEvent } from "react";
import { api, errorMessage, isApiError, type Identity } from "../api";
import { KeyRound, Plus, Upload, UserRound, Copy } from "lucide-react";
import {
  Button,
  EmptyState,
  Overlay,
  PageHeader,
  PasswordField,
} from "../components/ui";
import Confirm from "../components/Confirm";
import { useStore } from "../store";

function GenerateForm({ onDone }: { onDone: () => void }) {
  const refresh = useStore((s) => s.refresh);
  const [label, setLabel] = useState("");
  const [type, setType] = useState<"ed25519" | "rsa">("ed25519");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    setError(null);
    setBusy(true);
    try {
      await api.keyGenerate(
        label.trim() || type,
        type === "rsa" ? { type: "rsa", bits: 4096 } : { type: "ed25519" },
      );
      await refresh();
      onDone();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form aria-label="Generate key" className="panel stack" onSubmit={submit}>
      <label>
        Label
        <input
          disabled={busy}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          placeholder="laptop"
        />
      </label>
      <label>
        Type
        <select
          disabled={busy}
          value={type}
          onChange={(e) => setType(e.target.value as "ed25519" | "rsa")}
        >
          <option value="ed25519">Ed25519 (recommended)</option>
          <option value="rsa">RSA 4096</option>
        </select>
      </label>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <div className="row end">
        <button type="button" disabled={busy} onClick={onDone}>
          Cancel
        </button>
        <Button className="primary" busy={busy}>
          {busy ? "Generating…" : "Generate"}
        </Button>
      </div>
    </form>
  );
}

function ImportForm({ onDone }: { onDone: () => void }) {
  const refresh = useStore((s) => s.refresh);
  const [label, setLabel] = useState("");
  const [text, setText] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [busy, setBusy] = useState(false);
  const [needsPassphrase, setNeedsPassphrase] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.keyImport(label.trim() || "imported", text, passphrase || null);
      await refresh();
      onDone();
    } catch (err) {
      if (
        isApiError(err) &&
        (err.kind === "needs_passphrase" || err.kind === "bad_passphrase")
      )
        setNeedsPassphrase(true);
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form aria-label="Import key" className="panel stack" onSubmit={submit}>
      <label>
        Label
        <input
          disabled={busy}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
      </label>
      <label>
        Private key
        <textarea
          disabled={busy}
          rows={6}
          className="mono"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
        />
      </label>
      {needsPassphrase && (
        <PasswordField
          disabled={busy}
          label="Passphrase"
          autoFocus
          value={passphrase}
          onChange={(e) => setPassphrase(e.target.value)}
        />
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <p className="muted small">
        The key is stored decrypted inside your encrypted vault.
      </p>
      <div className="row end">
        <button type="button" disabled={busy} onClick={onDone}>
          Cancel
        </button>
        <Button className="primary" busy={busy} disabled={!text.trim()}>
          {busy ? "Importing…" : "Import"}
        </Button>
      </div>
    </form>
  );
}

function IdentityForm({
  identity,
  onDone,
}: {
  identity: Identity | null;
  onDone: () => void;
}) {
  const { inventory, refresh } = useStore();
  const [draftId] = useState(() => identity?.id ?? crypto.randomUUID());
  const [label, setLabel] = useState(identity?.label ?? "");
  const [username, setUsername] = useState(identity?.username ?? "");
  const [key, setKey] = useState(identity?.key ?? "");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const saved = await api.saveIdentity({
        id: draftId,
        label: label.trim() || username.trim() || "identity",
        username: username.trim() || null,
        key: key || null,
      });
      if (password) await api.setPassword(saved.id, password);
      await refresh();
      onDone();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <form aria-label="Identity" className="panel stack" onSubmit={submit}>
      <label>
        Label
        <input
          disabled={busy}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
      </label>
      <label>
        Username
        <input
          disabled={busy}
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
      </label>
      <label>
        Key
        <select
          disabled={busy}
          value={key}
          onChange={(e) => setKey(e.target.value)}
        >
          <option value="">No key</option>
          {inventory.keys.map((k) => (
            <option key={k.id} value={k.id}>
              {k.label}
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
          identity && inventory.has_password.includes(identity.id)
            ? "•••••• (stored)"
            : "Optional"
        }
      />
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <div className="row end">
        <button type="button" disabled={busy} onClick={onDone}>
          Cancel
        </button>
        <Button className="primary" busy={busy}>
          {busy ? "Saving…" : "Save"}
        </Button>
      </div>
    </form>
  );
}

export default function KeychainView() {
  const { inventory, refresh } = useStore();
  const [panel, setPanel] = useState<
    "generate" | "import" | Identity | "identity" | null
  >(null);
  const [deleting, setDeleting] = useState<{
    kind: "key" | "identity";
    id: string;
    label: string;
  } | null>(null);

  async function confirmDelete() {
    if (!deleting) return;
    await (deleting.kind === "key"
      ? api.deleteKey(deleting.id)
      : api.deleteIdentity(deleting.id));
    setDeleting(null);
    await refresh();
  }

  return (
    <section className="content">
      <PageHeader
        title="Keychain"
        description="Credentials, secured in your vault."
        actions={
          <>
            <Button icon={Upload} onClick={() => setPanel("import")}>
              Import key
            </Button>
            <Button icon={UserRound} onClick={() => setPanel("identity")}>
              New identity
            </Button>
            <Button
              icon={Plus}
              className="primary"
              onClick={() => setPanel("generate")}
            >
              Generate key
            </Button>
          </>
        }
      />
      {panel && (
        <Overlay
          title={
            panel === "generate"
              ? "Generate key"
              : panel === "import"
                ? "Import key"
                : panel === "identity"
                  ? "New identity"
                  : "Edit identity"
          }
          onDismiss={() => setPanel(null)}
          wide
        >
          {panel === "generate" && (
            <GenerateForm
              onDone={() =>
                setPanel((current) => (current === panel ? null : current))
              }
            />
          )}
          {panel === "import" && (
            <ImportForm
              onDone={() =>
                setPanel((current) => (current === panel ? null : current))
              }
            />
          )}
          {(panel === "identity" || typeof panel === "object") && (
            <IdentityForm
              identity={typeof panel === "object" ? panel : null}
              onDone={() =>
                setPanel((current) => (current === panel ? null : current))
              }
            />
          )}
        </Overlay>
      )}

      <h3 className="section-title">SSH keys</h3>
      {inventory.keys.length === 0 && (
        <EmptyState icon={KeyRound} title="No keys yet.">
          Generate a key or import one you already use.
        </EmptyState>
      )}
      <div className="resource-list">
        {inventory.keys.map((k) => (
          <article key={k.id} aria-label={k.label} className="card">
            <h3>{k.label}</h3>
            <p className="mono small">{k.fingerprint}</p>
            <p className="mono small truncate" title={k.public_key}>
              {k.public_key}
            </p>
            <div className="row end">
              <button
                className="ghost danger"
                onClick={() =>
                  setDeleting({ kind: "key", id: k.id, label: k.label })
                }
              >
                Delete
              </button>
              <Button
                icon={Copy}
                onClick={() => {
                  void navigator.clipboard
                    .writeText(k.public_key)
                    .catch(() => {});
                }}
              >
                Copy public key
              </Button>
            </div>
          </article>
        ))}
      </div>

      <h3 className="section-title">Identities</h3>
      {inventory.identities.length === 0 && (
        <EmptyState icon={UserRound} title="One identity. Many connections.">
          Identities bundle a username with a key or password, reusable across
          hosts.
        </EmptyState>
      )}
      <div className="resource-list">
        {inventory.identities.map((i) => (
          <article key={i.id} aria-label={i.label} className="card">
            <h3>{i.label}</h3>
            <p className="muted small">
              {i.username ?? "no username"}
              {i.key
                ? ` · key ${inventory.keys.find((k) => k.id === i.key)?.label ?? "?"}`
                : ""}
              {inventory.has_password.includes(i.id) ? " · password" : ""}
            </p>
            <div className="row end">
              <button
                className="ghost danger"
                onClick={() =>
                  setDeleting({ kind: "identity", id: i.id, label: i.label })
                }
              >
                Delete
              </button>
              <button onClick={() => setPanel(i)}>Edit</button>
            </div>
          </article>
        ))}
      </div>
      {deleting && (
        <Confirm
          title={`Delete ${deleting.label}?`}
          body={
            deleting.kind === "key"
              ? "The private key is removed from the vault."
              : "Hosts using it will need new credentials."
          }
          onConfirm={confirmDelete}
          onCancel={() => setDeleting(null)}
        />
      )}
    </section>
  );
}
