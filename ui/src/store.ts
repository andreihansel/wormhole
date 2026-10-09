import { create } from "zustand";
import { api, type Id, type Inventory } from "./api";

export type View = "hosts" | "terminals" | "keychain" | "snippets" | "forwards" | "sftp" | "known_hosts" | "settings";

export interface Tab {
  session: Id;
  title: string;
  host: Id | null;
  closed: boolean;
  /** Why the session ended, once it has. */
  reason?: string;
}

interface State {
  inventory: Inventory;
  view: View;
  tabs: Tab[];
  activeTab: Id | null;
  refresh: () => Promise<void>;
  setView: (v: View) => void;
  openTab: (tab: Omit<Tab, "closed">) => void;
  markClosed: (session: Id, reason: string) => void;
  closeTab: (session: Id) => void;
  setActiveTab: (session: Id) => void;
  reset: () => void;
}

const empty = (): Inventory => ({
  groups: [],
  hosts: [],
  identities: [],
  keys: [],
  snippets: [],
  forwards: [],
  has_password: [],
});

export const useStore = create<State>((set) => ({
  inventory: empty(),
  view: "hosts",
  tabs: [],
  activeTab: null,
  refresh: async () => set({ inventory: await api.inventory() }),
  setView: (view) => set({ view }),
  openTab: (tab) =>
    set((s) => ({ tabs: [...s.tabs, { ...tab, closed: false }], activeTab: tab.session, view: "terminals" })),
  markClosed: (session, reason) =>
    set((s) => ({ tabs: s.tabs.map((t) => (t.session === session ? { ...t, closed: true, reason } : t)) })),
  closeTab: (session) =>
    set((s) => {
      const tabs = s.tabs.filter((t) => t.session !== session);
      const activeTab = s.activeTab === session ? (tabs.at(-1)?.session ?? null) : s.activeTab;
      return { tabs, activeTab };
    }),
  setActiveTab: (activeTab) => set({ activeTab }),
  reset: () => set({ inventory: empty(), view: "hosts", tabs: [], activeTab: null }),
}));
