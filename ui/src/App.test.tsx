import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import App from "./App";
import { useStore } from "./store";
import { fakeBackend } from "./test/backend";

vi.mock("./term", () => import("./test/fakeTerm"));

beforeEach(() => useStore.getState().reset());
afterEach(() => vi.useRealTimers());

test("locked vault shows the lock screen, unlocking shows the app", async () => {
  let unlocked = false;
  fakeBackend({
    vault_status: () => ({ initialized: true, unlocked }),
    vault_unlock: () => {
      unlocked = true;
      return null;
    },
  });
  render(<App />);
  await userEvent.type(
    await screen.findByLabelText("Master password"),
    "pw{Enter}",
  );
  const nav = await screen.findByRole("navigation", { name: "Sidebar" });
  for (const name of [
    "Hosts",
    "Terminals",
    "Keychain",
    "Snippets",
    "Port forwarding",
    "SFTP",
    "Known hosts",
    "Settings",
  ]) {
    expect(nav).toHaveTextContent(name);
  }
});

test("sidebar switches views", async () => {
  fakeBackend();
  render(<App />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Keychain" }),
  );
  expect(useStore.getState().view).toBe("keychain");
  expect(screen.getByRole("button", { name: "Keychain" })).toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("auto-locks after the configured idle time", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const be = fakeBackend({
    get_settings: () => ({ session_logs: false, auto_lock_minutes: 1 }),
    vault_lock: () => null,
  });
  render(<App />);
  await screen.findByRole("navigation", { name: "Sidebar" });
  await act(() => vi.advanceTimersByTimeAsync(45_000));
  act(
    () => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "a" })),
  );
  await act(() => vi.advanceTimersByTimeAsync(45_000));
  expect(be.called("vault_lock")).toHaveLength(0);
  await act(() => vi.advanceTimersByTimeAsync(20_000));
  expect(be.called("vault_lock")).toHaveLength(1);
  expect(await screen.findByLabelText("Master password")).toBeInTheDocument();
});

test("under StrictMode each backend event is handled exactly once", async () => {
  const { StrictMode } = await import("react");
  const { terms } = await import("./test/fakeTerm");
  terms.length = 0;
  const be = fakeBackend();
  render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
  await screen.findByRole("navigation", { name: "Sidebar" });
  act(() =>
    useStore.getState().openTab({ session: "s1", title: "web", host: null }),
  );
  await act(() =>
    be.emit({
      type: "session_output",
      session: "s1",
      data: Array.from(new TextEncoder().encode("once")),
    }),
  );
  const live = terms.filter((t) => !t.disposed);
  expect(live).toHaveLength(1);
  expect(live[0].written).toEqual(["once"]);
});

test("under StrictMode host key prompts still reach the dialog", async () => {
  const { StrictMode } = await import("react");
  const be = fakeBackend();
  render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
  await screen.findByRole("navigation", { name: "Sidebar" });
  await act(() =>
    be.emit({
      type: "host_key_prompt",
      prompt: "p1",
      host: "web",
      port: 22,
      algorithm: "ssh-ed25519",
      fingerprint: "SHA256:x",
    }),
  );
  expect(screen.getByRole("dialog")).toHaveTextContent("SHA256:x");
});

test("a failed lock remains visible and can be dismissed without losing the workspace", async () => {
  fakeBackend({
    vault_lock: () => {
      throw { kind: "io", message: "Lock failed" };
    },
  });
  render(<App />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Lock" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent("Lock failed");
  await userEvent.click(
    screen.getByRole("button", { name: "Dismiss notification" }),
  );
  expect(screen.queryByRole("alert")).toBeNull();
  expect(
    screen.getByRole("button", { name: "Hosts" }),
  ).toBeInTheDocument();
});
