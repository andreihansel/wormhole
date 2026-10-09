// One listener for backend events, fanned out to subscribers. Terminal
// output is buffered per session until a pane attaches, so nothing printed
// before the pane mounts (e.g. the login banner) is lost.
import { onEvent, type AppEvent, type Id } from "./api";

type Writer = (data: Uint8Array) => void;

const subscribers = new Set<(e: AppEvent) => void>();
const writers = new Map<Id, Writer>();
const pending = new Map<Id, Uint8Array[]>();

function dispatch(e: AppEvent) {
  if (e.type === "session_output") {
    const data = Uint8Array.from(e.data);
    const w = writers.get(e.session);
    if (w) w(data);
    else pending.set(e.session, [...(pending.get(e.session) ?? []), data]);
  }
  if (e.type === "session_closed") pending.delete(e.session);
  for (const fn of subscribers) fn(e);
}

/** Starts listening; returns a function that stops only this listener. */
export async function startEvents(): Promise<() => void> {
  return onEvent(dispatch);
}

export function subscribe(fn: (e: AppEvent) => void): () => void {
  subscribers.add(fn);
  return () => subscribers.delete(fn);
}

/** Routes a session's output to `writer`, first replaying anything buffered. */
export function attachOutput(session: Id, writer: Writer): () => void {
  for (const chunk of pending.get(session) ?? []) writer(chunk);
  pending.delete(session);
  writers.set(session, writer);
  return () => {
    if (writers.get(session) === writer) writers.delete(session);
  };
}
