// Thin wrapper around xterm.js so components can be tested without a canvas.
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

export interface TermHandle {
  write(data: Uint8Array | string): void;
  onData(cb: (data: string) => void): void;
  onResize(cb: (cols: number, rows: number) => void): void;
  fit(): void;
  focus(): void;
  dispose(): void;
}

export function createTerm(el: HTMLElement): TermHandle {
  const styles = getComputedStyle(document.documentElement);
  const token = (name: string, fallback: string) =>
    styles.getPropertyValue(name).trim() || fallback;
  const ansi = {
    black: token("--ansi-black", "#42536a"),
    red: token("--ansi-red", "#ef8593"),
    green: token("--ansi-green", "#8ecfa7"),
    yellow: token("--ansi-yellow", "#e9c686"),
    blue: token("--ansi-blue", "#89b8f0"),
    magenta: token("--ansi-magenta", "#b69ad9"),
    cyan: token("--ansi-cyan", "#8bcfd5"),
    white: token("--ansi-white", "#c9d6e7"),
  };
  const term = new Terminal({
    cursorBlink: true,
    fontFamily: token("--mono", "monospace"),
    fontSize: 13,
    theme: {
      ...ansi,
      background: token("--term-bg", "#080d15"),
      foreground: token("--term-text", "#dce7f4"),
      cursor: token("--accent", "#8cc8ff"),
      selectionBackground: token("--term-selection", "#253d59"),
      brightBlack: "#71839b",
      brightRed: "#ffabb5",
      brightGreen: "#b1e7c4",
      brightYellow: "#f8dda8",
      brightBlue: "#b5d8ff",
      brightMagenta: "#d7baf6",
      brightCyan: "#b2eef2",
      brightWhite: "#f2f6fc",
    },
    scrollback: 10000,
    allowProposedApi: false,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(el);
  return {
    write: (d) => term.write(d),
    onData: (cb) => void term.onData(cb),
    onResize: (cb) => void term.onResize(({ cols, rows }) => cb(cols, rows)),
    fit: () => {
      try {
        fit.fit();
      } catch {
        /* element not visible yet */
      }
    },
    focus: () => term.focus(),
    dispose: () => term.dispose(),
  };
}
