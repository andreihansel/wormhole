// A fake Tauri backend for component tests: real invoke/listen plumbing,
// scripted command handlers, and a record of every call.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import type { AppEvent, Inventory } from "../api";

type Handler = (args: Record<string, unknown>) => unknown;

export interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

export function emptyInventory(): Inventory {
  return { groups: [], hosts: [], identities: [], keys: [], snippets: [], forwards: [], has_password: [] };
}

export function fakeBackend(handlers: Record<string, Handler> = {}) {
  const calls: Call[] = [];
  const defaults: Record<string, Handler> = {
    vault_status: () => ({ initialized: true, unlocked: true }),
    inventory: () => emptyInventory(),
    get_settings: () => ({ session_logs: false, auto_lock_minutes: 15 }),
    forwards_active: () => [],
    known_hosts: () => [],
  };
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args });
      const h = handlers[cmd] ?? defaults[cmd];
      if (!h) throw { kind: "test", message: `unhandled command ${cmd}` };
      return h(args);
    },
    { shouldMockEvents: true },
  );
  return {
    calls,
    called: (cmd: string) => calls.filter((c) => c.cmd === cmd),
    emit: (e: AppEvent) => emit("wh-event", e),
  };
}

// Unmount before tearing down the mocks, so components can unlisten cleanly.
afterEach(() => {
  cleanup();
  clearMocks();
});
