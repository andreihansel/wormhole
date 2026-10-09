// Recording stand-in for ../term (xterm.js cannot render in jsdom).
import type { TermHandle } from "../term";

export interface FakeTerm extends TermHandle {
  el: HTMLElement;
  written: string[];
  type(data: string): void;
  resize(cols: number, rows: number): void;
  disposed: boolean;
}

export const terms: FakeTerm[] = [];

export function createTerm(el: HTMLElement): TermHandle {
  let dataCb: (d: string) => void = () => {};
  let resizeCb: (c: number, r: number) => void = () => {};
  const t: FakeTerm = {
    el,
    written: [],
    disposed: false,
    write: (d) => void t.written.push(typeof d === "string" ? d : new TextDecoder().decode(d)),
    onData: (cb) => void (dataCb = cb),
    onResize: (cb) => void (resizeCb = cb),
    fit: () => {},
    focus: () => {},
    dispose: () => void (t.disposed = true),
    type: (d) => dataCb(d),
    resize: (c, r) => resizeCb(c, r),
  };
  terms.push(t);
  return t;
}
