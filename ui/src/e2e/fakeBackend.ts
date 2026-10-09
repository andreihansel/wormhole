// In-memory backend for Playwright runs (VITE_E2E=1). Never bundled otherwise.
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import {
  emptySettings,
  type AppEvent,
  type Host,
  type Inventory,
} from "../api";
import { visualInventory } from "./visualFixtures";

const state = {
  initialized: false,
  unlocked: false,
  password: "",
  inventory: {
    groups: [],
    hosts: [],
    identities: [],
    keys: [],
    snippets: [],
    forwards: [],
    has_password: [],
  } as Inventory,
  trusted: new Set<string>(),
  pending: new Map<string, (accept: boolean) => void>(),
  sessions: new Map<string, Host>(),
};

const fixture = new URLSearchParams(location.search).get("fixture");
if (fixture === "populated" || fixture === "empty") {
  Object.assign(state, {
    initialized: true,
    unlocked: true,
    password: "visual-only",
    inventory: fixture === "populated" ? visualInventory : state.inventory,
  });
  visualInventory.hosts.forEach((h) => state.trusted.add(h.address));
}
const trustFixture = new URLSearchParams(location.search).get("trust");
if (trustFixture) state.trusted.clear();
const activeForwards = new Map<string, string>();
const send = (e: AppEvent) => emit("wh-event", e);
const bytes = (s: string) => Array.from(new TextEncoder().encode(s));
const id = () => crypto.randomUUID();
const fail = (kind: string, message: string) => {
  throw { kind, message };
};

mockIPC(
  async (cmd, raw) => {
    const a = (raw ?? {}) as Record<string, any>;
    switch (cmd) {
      case "vault_status":
        return { initialized: state.initialized, unlocked: state.unlocked };
      case "vault_create":
        Object.assign(state, {
          initialized: true,
          unlocked: true,
          password: a.password,
        });
        return null;
      case "vault_unlock":
        if (a.password !== state.password)
          fail("bad_password", "incorrect password");
        state.unlocked = true;
        return null;
      case "vault_lock":
        if (new URLSearchParams(location.search).get("error") === "lock")
          fail("io", "Unable to lock vault: simulated disk error");
        state.unlocked = false;
        return null;
      case "inventory":
        return state.inventory;
      case "get_settings":
        return { session_logs: false, auto_lock_minutes: 15 };
      case "forwards_active":
        return [...activeForwards].map(([rule, listen]) => ({ rule, listen }));
      case "known_hosts":
        return fixture === "populated"
          ? [
              {
                id: "trust-1",
                pattern: "edge-01.example.test",
                key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFakeSyntheticVisualReviewKeyThatIsNeverARealHostKey",
              },
            ]
          : [];
      case "save_settings":
      case "sftp_close":
      case "set_password":
        return null;
      case "forward_start":
        activeForwards.set(a.rule, "127.0.0.1:15432");
        return { rule: a.rule, listen: "127.0.0.1:15432" };
      case "forward_stop":
        activeForwards.delete(a.rule);
        return null;
      case "home_dir":
        return "/Users/observer";
      case "local_list":
        return [
          {
            name: "Documents",
            path: `${a.path}/Documents`,
            is_dir: true,
            size: 0,
          },
          {
            name: "deployment-manifest.yaml",
            path: `${a.path}/deployment-manifest.yaml`,
            is_dir: false,
            size: 2451,
          },
        ];
      case "sftp_open":
        return "visual-sftp";
      case "sftp_home":
        return "/home/deploy";
      case "sftp_list":
        return [
          {
            name: "services",
            path: `${a.path}/services`,
            kind: "dir",
            size: 0,
            mode: 493,
            mtime: 1,
          },
          {
            name: "production-config.toml",
            path: `${a.path}/production-config.toml`,
            kind: "file",
            size: 1854,
            mode: 420,
            mtime: 1,
          },
        ];
      case "snippet_variables":
        return [...String(a.script).matchAll(/\{\{\s*([\w-]+)\s*\}\}/g)].map(
          (m) => m[1],
        );
      case "run_snippet":
        return a.hosts.map((host: string) => ({
          host,
          stdout:
            "Filesystem      Size  Used Avail Use% Mounted on\n/dev/nvme0n1p1  120G   48G   72G  40% /\n",
          stderr: "",
          exit_status: 0,
          error: null,
        }));
      case "open_local_session": {
        const session = id();
        const host = {
          id: session,
          label: "Local",
          address: "localhost",
          tags: [],
          group: null,
          settings: emptySettings(),
        };
        state.sessions.set(session, host);
        setTimeout(
          () =>
            void send({
              type: "session_output",
              session,
              data: bytes("Local synthetic terminal\r\nobserver@wormhole ~ $ "),
            }),
          10,
        );
        return session;
      }
      case "save_host": {
        const hosts = state.inventory.hosts.filter((h) => h.id !== a.host.id);
        state.inventory = { ...state.inventory, hosts: [...hosts, a.host] };
        return a.host;
      }
      case "open_ssh_session": {
        const host = state.inventory.hosts.find((h) => h.id === a.host)!;
        if (trustFixture === "changed") {
          void send({
            type: "host_key_changed",
            host: host.address,
            port: 22,
            fingerprint: "SHA256:ChangedSyntheticFingerprintForReviewOnly",
          });
          fail("host_key_changed", "Host key has changed; connection refused");
        }
        if (!state.trusted.has(host.address)) {
          const prompt = id();
          const accepted = new Promise<boolean>((resolve) =>
            state.pending.set(prompt, resolve),
          );
          void send({
            type: "host_key_prompt",
            prompt,
            host: host.address,
            port: 22,
            algorithm: "ssh-ed25519",
            fingerprint: "SHA256:e2eFakeFingerprint",
          });
          if (!(await accepted)) fail("host_key_rejected", "host key rejected");
          state.trusted.add(host.address);
        }
        const session = id();
        state.sessions.set(session, host);
        setTimeout(
          () =>
            void send({
              type: "session_output",
              session,
              data: bytes(`Welcome to ${host.label}\r\n$ `),
            }),
          10,
        );
        return session;
      }
      case "host_key_respond":
        state.pending.get(a.prompt)?.(!!a.accept);
        state.pending.delete(a.prompt);
        return null;
      case "session_write": {
        // A tiny "shell": echo keystrokes, and run `echo` on Enter.
        const s = a.id as string;
        const buf = ((state as any)[s] ?? "") + a.data;
        if (a.data.includes("\r")) {
          const line = buf.replace(/\r$/, "");
          const out = line.startsWith("echo ") ? line.slice(5) + "\r\n" : "";
          (state as any)[s] = "";
          void send({
            type: "session_output",
            session: s,
            data: bytes(`\r\n${out}$ `),
          });
        } else {
          (state as any)[s] = buf;
          void send({
            type: "session_output",
            session: s,
            data: bytes(a.data),
          });
        }
        return null;
      }
      case "session_resize":
      case "session_close":
        return null;
      default:
        fail("e2e", `fake backend does not implement ${cmd}`);
    }
  },
  { shouldMockEvents: true },
);
