import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test } from "vitest";
import { startEvents } from "../events";
import { fakeBackend } from "../test/backend";
import HostKeyDialog from "./HostKeyDialog";

let stop: () => void;
afterEach(() => stop?.());

const prompt = (id: string, host: string) => ({
  type: "host_key_prompt" as const,
  prompt: id,
  host,
  port: 22,
  algorithm: "ssh-ed25519",
  fingerprint: `SHA256:fp-${host}`,
});

test("unknown key shows fingerprint and answers accept/reject in order", async () => {
  const be = fakeBackend({ host_key_respond: () => null });
  stop = await startEvents();
  render(<HostKeyDialog />);
  await act(() => be.emit(prompt("p1", "bastion")));
  await act(() => be.emit(prompt("p2", "internal")));
  const dialog = screen.getByRole("dialog");
  expect(dialog).toHaveTextContent("bastion");
  expect(dialog).toHaveTextContent("SHA256:fp-bastion");
  await userEvent.click(
    screen.getByRole("button", { name: "Trust and connect" }),
  );
  expect(screen.getByRole("dialog")).toHaveTextContent("internal");
  await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(be.called("host_key_respond").map((c) => c.args)).toEqual([
    { prompt: "p1", accept: true },
    { prompt: "p2", accept: false },
  ]);
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

test("changed key shows a blocking warning without a trust option", async () => {
  const be = fakeBackend();
  stop = await startEvents();
  render(<HostKeyDialog />);
  await act(() =>
    be.emit({
      type: "host_key_changed",
      host: "web",
      port: 2222,
      fingerprint: "SHA256:evil",
    }),
  );
  const dialog = screen.getByRole("alertdialog");
  expect(dialog).toHaveTextContent("HOST KEY HAS CHANGED");
  expect(dialog).toHaveTextContent("web:2222");
  expect(dialog).toHaveTextContent("SHA256:evil");
  expect(
    screen.queryByRole("button", { name: /trust/i }),
  ).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
});

test("Escape rejects an unknown host instead of silently dismissing it", async () => {
  const be = fakeBackend({ host_key_respond: () => null });
  stop = await startEvents();
  render(<HostKeyDialog />);
  await act(() => be.emit(prompt("p1", "bastion")));
  await userEvent.keyboard("{Escape}");
  expect(be.called("host_key_respond")[0]?.args).toEqual({
    prompt: "p1",
    accept: false,
  });
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
