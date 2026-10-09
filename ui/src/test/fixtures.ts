import { emptySettings, type ConnSettings, type Group, type Host, type Inventory } from "../api";
import { emptyInventory } from "./backend";

let n = 0;
const id = (p: string) => `${p}-${++n}`;

export function host(label: string, address: string, extra: Partial<Host> & { settings?: Partial<ConnSettings> } = {}): Host {
  const { settings, ...rest } = extra;
  return { id: id("h"), label, address, group: null, tags: [], ...rest, settings: { ...emptySettings(), ...settings } };
}

export function group(name: string, parent: string | null = null, settings: Partial<ConnSettings> = {}): Group {
  return { id: id("g"), name, parent, settings: { ...emptySettings(), ...settings } };
}

export function inventory(parts: Partial<Inventory>): Inventory {
  return { ...emptyInventory(), ...parts };
}
