// Typed wrappers over the Tauri commands in src-tauri/src/lib.rs.
// Tauri maps camelCase argument keys to the Rust snake_case parameters.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Id = string;

export interface ConnSettings {
  port: number | null;
  username: string | null;
  identity: Id | null;
  jump_chain: Id[] | null;
  env: Record<string, string>;
  startup_snippet: Id | null;
  agent_forwarding: boolean | null;
}

export interface Host {
  id: Id;
  label: string;
  address: string;
  group: Id | null;
  tags: string[];
  settings: ConnSettings;
}

export interface Group {
  id: Id;
  name: string;
  parent: Id | null;
  settings: ConnSettings;
}

export interface Identity {
  id: Id;
  label: string;
  username: string | null;
  key: Id | null;
}

export interface SshKey {
  id: Id;
  label: string;
  public_key: string;
  fingerprint: string;
  certificate: string | null;
}

export interface Snippet {
  id: Id;
  label: string;
  script: string;
}

export type ForwardKind = "local" | "remote" | "dynamic";

export interface PortForward {
  id: Id;
  label: string;
  host: Id;
  kind: ForwardKind;
  bind_address: string;
  bind_port: number;
  dest_host: string;
  dest_port: number;
}

export interface Inventory {
  groups: Group[];
  hosts: Host[];
  identities: Identity[];
  keys: SshKey[];
  snippets: Snippet[];
  forwards: PortForward[];
  has_password: Id[];
}

export interface Resolved {
  host: Id;
  address: string;
  port: number;
  username: string | null;
  identity: Id | null;
  key: Id | null;
  jump_chain: Id[];
  env: Record<string, string>;
  startup_snippet: Id | null;
  agent_forwarding: boolean;
}

export interface KnownHost {
  id: Id;
  pattern: string;
  key: string;
}

export interface VaultStatus {
  initialized: boolean;
  unlocked: boolean;
}

export interface Settings {
  session_logs: boolean;
  auto_lock_minutes: number;
}

export interface ImportSummary {
  created: number;
  updated: number;
  keys_imported: number;
  warnings: string[];
}

export interface ActiveForward {
  rule: Id;
  listen: string;
}

export interface SnippetRun {
  host: Id;
  stdout: string;
  stderr: string;
  exit_status: number | null;
  error: ApiError | null;
}

export interface RemoteEntry {
  name: string;
  path: string;
  kind: "file" | "dir" | "symlink" | "other";
  size: number;
  mode: number;
  mtime: number | null;
}

export interface LocalEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
}

/** Every command failure has this shape. */
export interface ApiError {
  kind: string;
  message: string;
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}

export function errorMessage(e: unknown): string {
  if (isApiError(e)) return e.message;
  return e instanceof Error ? e.message : String(e);
}

export type CloseReason = { kind: "exit"; code: number } | { kind: "disconnected" } | { kind: "closed_by_user" };

export type AppEvent =
  | { type: "session_output"; session: Id; data: number[] }
  | { type: "session_closed"; session: Id; reason: CloseReason }
  | { type: "host_key_prompt"; prompt: Id; host: string; port: number; algorithm: string; fingerprint: string }
  | { type: "host_key_changed"; host: string; port: number; fingerprint: string }
  | { type: "transfer_progress"; transfer: Id; done: number; total: number };

export function onEvent(cb: (e: AppEvent) => void): Promise<UnlistenFn> {
  return listen<AppEvent>("wh-event", (e) => cb(e.payload));
}

export const api = {
  vaultStatus: () => invoke<VaultStatus>("vault_status"),
  vaultCreate: (password: string) => invoke<void>("vault_create", { password }),
  vaultUnlock: (password: string) => invoke<void>("vault_unlock", { password }),
  vaultLock: () => invoke<void>("vault_lock"),
  vaultChangePassword: (old: string, next: string) => invoke<void>("vault_change_password", { old, new: next }),
  vaultExport: (path: string, password: string) => invoke<void>("vault_export", { path, password }),
  vaultImport: (path: string, password: string) => invoke<number>("vault_import", { path, password }),

  inventory: () => invoke<Inventory>("inventory"),
  saveHost: (host: Host) => invoke<Host>("save_host", { host }),
  deleteHost: (id: Id) => invoke<void>("delete_host", { id }),
  saveGroup: (group: Group) => invoke<Group>("save_group", { group }),
  deleteGroup: (id: Id, moveContents: boolean) => invoke<void>("delete_group", { id, moveContents }),
  saveIdentity: (identity: Identity) => invoke<Identity>("save_identity", { identity }),
  deleteIdentity: (id: Id) => invoke<void>("delete_identity", { id }),
  setPassword: (owner: Id, password: string | null) => invoke<void>("set_password", { owner, password }),
  resolveHost: (id: Id) => invoke<Resolved>("resolve_host", { id }),
  keyGenerate: (label: string, kind: { type: "ed25519" } | { type: "rsa"; bits: number }) =>
    invoke<SshKey>("key_generate", { label, kind }),
  keyImport: (label: string, privateKey: string, passphrase: string | null) =>
    invoke<SshKey>("key_import", { label, privateKey, passphrase }),
  deleteKey: (id: Id) => invoke<void>("delete_key", { id }),
  saveSnippet: (snippet: Snippet) => invoke<Snippet>("save_snippet", { snippet }),
  deleteSnippet: (id: Id) => invoke<void>("delete_snippet", { id }),
  snippetVariables: (script: string) => invoke<string[]>("snippet_variables", { script }),
  saveForward: (forward: PortForward) => invoke<PortForward>("save_forward", { forward }),
  deleteForward: (id: Id) => invoke<void>("delete_forward", { id }),
  knownHosts: () => invoke<KnownHost[]>("known_hosts"),
  deleteKnownHost: (id: Id) => invoke<void>("delete_known_host", { id }),

  importSshConfig: (path: string | null = null) => invoke<ImportSummary>("import_ssh_config", { path }),
  importKnownHosts: (path: string | null = null) => invoke<number>("import_known_hosts", { path }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),

  openSshSession: (host: Id, cols: number, rows: number) => invoke<Id>("open_ssh_session", { host, cols, rows }),
  openLocalSession: (cols: number, rows: number) => invoke<Id>("open_local_session", { cols, rows }),
  sessionWrite: (id: Id, data: string) => invoke<void>("session_write", { id, data }),
  sessionResize: (id: Id, cols: number, rows: number) => invoke<void>("session_resize", { id, cols, rows }),
  sessionClose: (id: Id) => invoke<void>("session_close", { id }),
  sessionBroadcast: (ids: Id[], data: string) => invoke<number>("session_broadcast", { ids, data }),
  hostKeyRespond: (prompt: Id, accept: boolean) => invoke<void>("host_key_respond", { prompt, accept }),

  forwardStart: (rule: Id) => invoke<ActiveForward>("forward_start", { rule }),
  forwardStop: (rule: Id) => invoke<void>("forward_stop", { rule }),
  forwardsActive: () => invoke<ActiveForward[]>("forwards_active"),
  runSnippet: (script: string, vars: Record<string, string>, hosts: Id[]) =>
    invoke<SnippetRun[]>("run_snippet", { script, vars, hosts }),

  sftpOpen: (host: Id) => invoke<Id>("sftp_open", { host }),
  sftpClose: (sftp: Id) => invoke<void>("sftp_close", { sftp }),
  sftpHome: (sftp: Id) => invoke<string>("sftp_home", { sftp }),
  sftpList: (sftp: Id, path: string) => invoke<RemoteEntry[]>("sftp_list", { sftp, path }),
  sftpMkdir: (sftp: Id, path: string) => invoke<void>("sftp_mkdir", { sftp, path }),
  sftpRename: (sftp: Id, from: string, to: string) => invoke<void>("sftp_rename", { sftp, from, to }),
  sftpRemove: (sftp: Id, path: string) => invoke<void>("sftp_remove", { sftp, path }),
  sftpUpload: (sftp: Id, transfer: Id, local: string, remote: string) =>
    invoke<number>("sftp_upload", { sftp, transfer, local, remote }),
  sftpDownload: (sftp: Id, transfer: Id, remote: string, local: string) =>
    invoke<number>("sftp_download", { sftp, transfer, remote, local }),
  localList: (path: string) => invoke<LocalEntry[]>("local_list", { path }),
  homeDir: () => invoke<string>("home_dir"),
};

export function emptySettings(): ConnSettings {
  return {
    port: null,
    username: null,
    identity: null,
    jump_chain: null,
    env: {},
    startup_snippet: null,
    agent_forwarding: null,
  };
}
