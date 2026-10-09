import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { startEvents } from "../events";
import { useStore } from "../store";
import { fakeBackend } from "../test/backend";
import { terms } from "../test/fakeTerm";
import TerminalsView from "./TerminalsView";

vi.mock("../term", () => import("../test/fakeTerm"));

const bytes = (s: string) => Array.from(new TextEncoder().encode(s));
let stop: () => void;

beforeEach(async () => {
  terms.length = 0;
  useStore.getState().reset();
});
afterEach(() => stop?.());

async function setup(handlers = {}) {
  const be = fakeBackend({
    session_write: () => null,
    session_resize: () => null,
    session_close: () => null,
    session_broadcast: ({ ids }: Record<string, unknown>) => (ids as string[]).length,
    open_local_session: () => "local-1",
    ...handlers,
  });
  stop = await startEvents();
  return be;
}

function open(session: string, title: string) {
  act(() => useStore.getState().openTab({ session, title, host: null }));
}

test("pty output (including output before mount) reaches the terminal", async () => {
  const be = await setup();
  open("s1", "web");
  await be.emit({ type: "session_output", session: "s1", data: bytes("Welcome\r\n") });
  render(<TerminalsView />);
  expect(terms[0].written).toEqual(["Welcome\r\n"]);
  await act(() => be.emit({ type: "session_output", session: "s1", data: bytes("$ ") }));
  expect(terms[0].written).toEqual(["Welcome\r\n", "$ "]);
});

test("keystrokes and resizes go to the session", async () => {
  const be = await setup();
  open("s1", "web");
  render(<TerminalsView />);
  terms[0].type("ls\r");
  terms[0].resize(120, 40);
  expect(be.called("session_write")[0].args).toEqual({ id: "s1", data: "ls\r" });
  expect(be.called("session_resize")[0].args).toEqual({ id: "s1", cols: 120, rows: 40 });
});

test("tabs switch and close", async () => {
  const be = await setup();
  open("s1", "web");
  open("s2", "db");
  render(<TerminalsView />);
  expect(screen.getByRole("tab", { name: /db/, selected: true })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("tab", { name: /web/ }));
  expect(useStore.getState().activeTab).toBe("s1");
  await userEvent.click(screen.getByRole("button", { name: "Close web" }));
  expect(be.called("session_close")[0].args).toEqual({ id: "s1" });
  expect(useStore.getState().tabs.map((t) => t.session)).toEqual(["s2"]);
});

test("split view renders every pane at once", async () => {
  await setup();
  open("s1", "web");
  open("s2", "db");
  render(<TerminalsView />);
  expect(screen.getAllByRole("tabpanel").filter((p) => !p.hidden)).toHaveLength(1);
  await userEvent.click(screen.getByRole("switch", { name: "Split view" }));
  expect(screen.getAllByRole("tabpanel").filter((p) => !p.hidden)).toHaveLength(2);
});

test("broadcast routes input to all open sessions", async () => {
  const be = await setup();
  open("s1", "web");
  open("s2", "db");
  render(<TerminalsView />);
  await userEvent.click(screen.getByRole("switch", { name: "Broadcast input" }));
  terms[1].type("uptime\r");
  expect(be.called("session_broadcast")[0].args).toEqual({ ids: ["s1", "s2"], data: "uptime\r" });
  expect(be.called("session_write")).toHaveLength(0);
});

test("ended sessions are marked and stop accepting input", async () => {
  const be = await setup();
  open("s1", "web");
  render(<TerminalsView />);
  await act(() => be.emit({ type: "session_closed", session: "s1", reason: { kind: "exit", code: 0 } }));
  expect(screen.getByRole("status")).toHaveTextContent("Session ended (exit 0)");
  terms[0].type("x");
  expect(be.called("session_write")).toHaveLength(0);
});

test("new local terminal opens a tab", async () => {
  const be = await setup();
  render(<TerminalsView />);
  await userEvent.click(screen.getByRole("button", { name: "New local terminal" }));
  expect(be.called("open_local_session")).toHaveLength(1);
  expect(useStore.getState().tabs[0]).toMatchObject({ session: "local-1", title: "Local" });
});
