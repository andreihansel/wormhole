import { afterEach, expect, test, vi } from "vitest";
import { fakeBackend } from "./test/backend";
import { attachOutput, startEvents, subscribe } from "./events";

let stop: (() => void) | undefined;
afterEach(() => stop?.());

const bytes = (s: string) => Array.from(new TextEncoder().encode(s));

test("output before a pane attaches is buffered, then streamed live", async () => {
  const be = fakeBackend();
  stop = await startEvents();
  await be.emit({ type: "session_output", session: "s1", data: bytes("banner\r\n") });
  const seen: string[] = [];
  const detach = attachOutput("s1", (d) => seen.push(new TextDecoder().decode(d)));
  expect(seen).toEqual(["banner\r\n"]);
  await be.emit({ type: "session_output", session: "s1", data: bytes("$ ") });
  await be.emit({ type: "session_output", session: "other", data: bytes("nope") });
  expect(seen).toEqual(["banner\r\n", "$ "]);
  detach();
});

test("subscribers receive every event", async () => {
  const be = fakeBackend();
  stop = await startEvents();
  const fn = vi.fn();
  const unsub = subscribe(fn);
  await be.emit({ type: "session_closed", session: "s1", reason: { kind: "exit", code: 0 } });
  expect(fn).toHaveBeenCalledWith({ type: "session_closed", session: "s1", reason: { kind: "exit", code: 0 } });
  unsub();
  await be.emit({ type: "session_closed", session: "s2", reason: { kind: "disconnected" } });
  expect(fn).toHaveBeenCalledTimes(1);
});

test("stopping one listener does not drop other subscribers", async () => {
  const be = fakeBackend();
  const stopA = await startEvents();
  stop = await startEvents();
  const fn = vi.fn();
  subscribe(fn);
  stopA();
  await be.emit({ type: "session_closed", session: "s1", reason: { kind: "exit", code: 0 } });
  expect(fn).toHaveBeenCalledTimes(1);
});
