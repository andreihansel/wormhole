import { useCallback, useEffect, useState } from "react";
import {
  api,
  errorMessage,
  type Id,
  type LocalEntry,
  type RemoteEntry,
} from "../api";
import {
  ArrowUp,
  ArrowRight,
  ArrowLeft,
  File,
  Folder,
  HardDrive,
  Server,
} from "lucide-react";
import { Button, PageHeader, Tip } from "../components/ui";
import { subscribe } from "../events";
import { useStore } from "../store";

interface Row {
  name: string;
  path: string;
  dir: boolean;
  size: number;
}

interface Transfer {
  id: Id;
  name: string;
  done: number;
  total: number;
}

const parent = (p: string) => p.replace(/\/[^/]+\/?$/, "") || "/";
const join = (dir: string, name: string) => `${dir.replace(/\/$/, "")}/${name}`;

function size(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(1)} GB`;
}

function Pane(props: {
  label: string;
  disabled?: boolean;
  pathLabel: string;
  path: string;
  rows: Row[] | null;
  selected: string | null;
  onSelect: (path: string) => void;
  onOpen: (path: string) => void;
  onPath: (path: string) => void;
}) {
  const [draft, setDraft] = useState(props.path);
  useEffect(() => setDraft(props.path), [props.path]);
  return (
    <div className="fs-pane">
      <h3 className="fs-heading">
        {props.label === "Local files" ? (
          <HardDrive size={16} aria-hidden="true" />
        ) : (
          <Server size={16} aria-hidden="true" />
        )}
        {props.label}
      </h3>
      <div className="row">
        <Tip label={`Up (${props.label})`}>
          <button
            disabled={props.disabled}
            aria-label={`Up (${props.label})`}
            onClick={() => props.onOpen(parent(props.path))}
          >
            <ArrowUp size={16} aria-hidden="true" />
          </button>
        </Tip>
        <input
          disabled={props.disabled}
          aria-label={props.pathLabel}
          className="grow mono"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && props.onPath(draft)}
        />
      </div>
      <div role="grid" aria-label={props.label} className="fs-list">
        {props.rows === null && <p className="empty">Not connected.</p>}
        {props.rows?.length === 0 && (
          <p className="empty">This folder is empty.</p>
        )}
        {props.rows?.map((r) => (
          <div
            key={r.path}
            role="row"
            tabIndex={props.disabled ? -1 : 0}
            aria-disabled={props.disabled || undefined}
            onKeyDown={(e) => {
              if (props.disabled) return;
              if (e.key === " " || e.key === "Enter") {
                e.preventDefault();
                if (e.key === "Enter" && r.dir) props.onOpen(r.path);
                else props.onSelect(r.path);
              }
              if (e.key === "ArrowDown") {
                e.preventDefault();
                (
                  e.currentTarget.nextElementSibling as HTMLElement | null
                )?.focus();
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                (
                  e.currentTarget.previousElementSibling as HTMLElement | null
                )?.focus();
              }
            }}
            aria-label={r.name}
            aria-selected={props.selected === r.path}
            className={props.selected === r.path ? "fs-row selected" : "fs-row"}
            onClick={() => !props.disabled && props.onSelect(r.path)}
            onDoubleClick={() =>
              !props.disabled && r.dir && props.onOpen(r.path)
            }
          >
            <span role="gridcell" aria-hidden className="fs-icon">
              {r.dir ? <Folder size={16} /> : <File size={16} />}
            </span>
            <span role="gridcell" className="grow">
              {r.name}
            </span>
            <span role="gridcell" className="muted small">
              {r.dir ? "" : size(r.size)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function SftpView() {
  const hosts = useStore((s) => s.inventory.hosts);
  const [localPath, setLocalPath] = useState("");
  const [local, setLocal] = useState<LocalEntry[]>([]);
  const [host, setHost] = useState("");
  const [sftp, setSftp] = useState<Id | null>(null);
  const [remotePath, setRemotePath] = useState("");
  const [remote, setRemote] = useState<RemoteEntry[] | null>(null);
  const [selLocal, setSelLocal] = useState<string | null>(null);
  const [selRemote, setSelRemote] = useState<string | null>(null);
  const [transfers, setTransfers] = useState<Transfer[]>([]);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const listLocal = useCallback(async (path: string) => {
    try {
      setLocal(await api.localList(path));
      setLocalPath(path);
      setSelLocal(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  const listRemote = useCallback(
    async (path: string, session = sftp) => {
      if (!session) return;
      try {
        setRemote(await api.sftpList(session, path));
        setRemotePath(path);
        setSelRemote(null);
      } catch (e) {
        setError(errorMessage(e));
      }
    },
    [sftp],
  );

  useEffect(() => {
    void api
      .homeDir()
      .then(listLocal)
      .catch((e) => setError(errorMessage(e)));
  }, [listLocal]);

  useEffect(
    () =>
      subscribe((e) => {
        if (e.type === "transfer_progress")
          setTransfers((ts) =>
            ts.map((t) =>
              t.id === e.transfer ? { ...t, done: e.done, total: e.total } : t,
            ),
          );
      }),
    [],
  );

  async function chooseHost(id: string) {
    if (connecting || transfers.length) return;
    setConnecting(true);
    setHost(id);
    setError(null);
    setSftp(null);
    setRemote(null);
    setRemotePath("");
    setSelRemote(null);
    if (sftp) await api.sftpClose(sftp).catch(() => {});
    if (!id) {
      setConnecting(false);
      return;
    }
    try {
      const session = await api.sftpOpen(id);
      const home = await api.sftpHome(session);
      const entries = await api.sftpList(session, home);
      setRemotePath(home);
      setRemote(entries);
      setSftp(session);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setConnecting(false);
    }
  }

  async function transfer(kind: "upload" | "download") {
    if (!sftp || connecting || transfers.length) return;
    const from = kind === "upload" ? selLocal : selRemote;
    if (!from) return;
    const name = from.split("/").pop()!;
    const id = crypto.randomUUID();
    setTransfers((ts) => [...ts, { id, name, done: 0, total: 0 }]);
    setError(null);
    try {
      if (kind === "upload") {
        await api.sftpUpload(sftp, id, from, join(remotePath, name));
        await listRemote(remotePath);
      } else {
        await api.sftpDownload(sftp, id, from, join(localPath, name));
        await listLocal(localPath);
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setTransfers((ts) => ts.filter((t) => t.id !== id));
    }
  }

  const localRows: Row[] = local.map((e) => ({
    name: e.name,
    path: e.path,
    dir: e.is_dir,
    size: e.size,
  }));
  const remoteRows: Row[] | null =
    remote?.map((e) => ({
      name: e.name,
      path: e.path,
      dir: e.kind === "dir",
      size: e.size,
    })) ?? null;
  const selectedRemoteIsFile = remote?.some(
    (e) => e.path === selRemote && e.kind !== "dir",
  );
  const selectedLocalIsFile = local.some(
    (e) => e.path === selLocal && !e.is_dir,
  );

  return (
    <section className="content sftp">
      <PageHeader
        title="SFTP"
        description="Two destinations. One secure connection."
        actions={
          <label className="inline">
            Host
            <select
              disabled={connecting || transfers.length > 0}
              value={host}
              onChange={(e) => chooseHost(e.target.value)}
            >
              <option value="">Choose a host…</option>
              {hosts.map((h) => (
                <option key={h.id} value={h.id}>
                  {h.label}
                </option>
              ))}
            </select>
          </label>
        }
      />
      {connecting && (
        <p role="status" className="muted small">
          Connecting…
        </p>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <div className="fs">
        <Pane
          label="Local files"
          disabled={transfers.length > 0}
          pathLabel="Local path"
          path={localPath}
          rows={localRows}
          selected={selLocal}
          onSelect={setSelLocal}
          onOpen={listLocal}
          onPath={listLocal}
        />
        <div className="fs-actions">
          <Button
            icon={ArrowRight}
            aria-label="Upload →"
            disabled={
              connecting ||
              !sftp ||
              !selectedLocalIsFile ||
              transfers.length > 0
            }
            onClick={() => transfer("upload")}
          >
            Upload
          </Button>
          <Button
            icon={ArrowLeft}
            aria-label="← Download"
            disabled={
              connecting ||
              !sftp ||
              !selectedRemoteIsFile ||
              transfers.length > 0
            }
            onClick={() => transfer("download")}
          >
            Download
          </Button>
        </div>
        <Pane
          label="Remote files"
          disabled={connecting || !sftp || transfers.length > 0}
          pathLabel="Remote path"
          path={remotePath}
          rows={remoteRows}
          selected={selRemote}
          onSelect={setSelRemote}
          onOpen={(p) => listRemote(p)}
          onPath={(p) => listRemote(p)}
        />
      </div>
      {transfers.length > 0 && (
        <ul className="transfers">
          {transfers.map((t) => {
            const pct = t.total ? Math.round((t.done / t.total) * 100) : 0;
            return (
              <li key={t.id}>
                {t.name}
                <div
                  role="progressbar"
                  aria-label={t.name}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={pct}
                  className="bar"
                >
                  <div style={{ transform: `scaleX(${pct / 100})` }} />
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
