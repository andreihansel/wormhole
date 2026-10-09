import type { ConnSettings, Group, Id } from "./api";

type Inherited = Partial<ConnSettings> & { from?: Record<string, string> };

const KEYS = ["port", "username", "identity", "jump_chain", "startup_snippet", "agent_forwarding"] as const;

/** Ancestors of `groupId`, root first, inclusive. */
export function groupPath(groups: Group[], groupId: Id | null): Group[] {
  const byId = new Map(groups.map((g) => [g.id, g]));
  const path: Group[] = [];
  const seen = new Set<Id>();
  for (let cur = groupId ? byId.get(groupId) : undefined; cur && !seen.has(cur.id); cur = cur.parent ? byId.get(cur.parent) : undefined) {
    seen.add(cur.id);
    path.unshift(cur);
  }
  return path;
}

/** Values a host in `groupId` would inherit, nearest group first (mirrors wh-core). */
export function inheritedFrom(groups: Group[], groupId: Id | null): Inherited {
  const out: Inherited = { from: {} };
  for (const g of groupPath(groups, groupId).reverse()) {
    for (const k of KEYS) {
      const v = g.settings[k];
      if (v !== null && out[k] === undefined) {
        (out as Record<string, unknown>)[k] = v;
        out.from![k] = g.name;
      }
    }
  }
  return out;
}
