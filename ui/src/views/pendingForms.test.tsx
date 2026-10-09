import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { useStore } from "../store";
import { fakeBackend } from "../test/backend";
import { inventory, host } from "../test/fixtures";
import KeychainView from "./KeychainView";
import HostForm from "./HostForm";
import SftpView from "./SftpView";
import SnippetsView from "./SnippetsView";
import SettingsView from "./SettingsView";
beforeEach(() => useStore.getState().reset());
test("pending generation keeps its dialog and draft until completion", async () => {
  let finish!: (value: unknown) => void;
  fakeBackend({ key_generate: () => new Promise((r) => (finish = r)) });
  render(<KeychainView />);
  await userEvent.click(screen.getByRole("button", { name: "Generate key" }));
  await userEvent.type(screen.getByLabelText("Label"), "laptop");
  await userEvent.click(
    screen.getByRole("button", { name: "Generate" }),
  );
  await userEvent.keyboard("{Escape}");
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  expect(screen.getByLabelText("Label")).toBeDisabled();
  expect(screen.getByLabelText("Label")).toHaveValue("laptop");
  await act(async () => finish({}));
  expect(screen.queryByRole("dialog")).toBeNull();
});
test("host saving freezes the submitted draft and prevents concurrent deletion", async () => {
  let finish!: (value: unknown) => void;
  const current = host("First", "first.example.test");
  const close = vi.fn();
  const be = fakeBackend({ save_host: () => new Promise((r) => (finish = r)) });
  render(<HostForm host={current} defaultGroup={null} onClose={close} />);
  await userEvent.click(
    screen.getByRole("button", { name: "Save" }),
  );
  expect(screen.getByLabelText("Address")).toBeDisabled();
  expect(
    screen.getByRole("button", { name: "Delete" }),
  ).toBeDisabled();
  expect(screen.getByRole("button", { name: "Show Password" })).toBeDisabled();
  await userEvent.type(screen.getByLabelText("Address"), "ignored");
  expect(screen.getByLabelText("Address")).toHaveValue("first.example.test");
  expect(be.called("save_host")).toHaveLength(1);
  await act(async () => finish(current));
  expect(close).toHaveBeenCalledOnce();
});
test("SFTP switching blocks transfers until the new host home and listing are ready", async () => {
  let finishHome!: (value: string) => void;
  const first = host("First", "first.example.test");
  const second = { ...first, id: "second", label: "Second" };
  const be = fakeBackend({
    inventory: () => inventory({ hosts: [first, second] }),
    home_dir: () => "/local",
    local_list: () => [
      { name: "file.txt", path: "/local/file.txt", is_dir: false, size: 10 },
    ],
    sftp_open: ({ host }) => host,
    sftp_close: () => null,
    sftp_home: ({ sftp }) =>
      sftp === first.id
        ? "/first-home"
        : new Promise<string>((r) => (finishHome = r)),
    sftp_list: () => [],
    sftp_upload: () => null,
  });
  await useStore.getState().refresh();
  render(<SftpView />);
  await userEvent.selectOptions(
    screen.getByRole("combobox", { name: "Host" }),
    first.id,
  );
  await userEvent.click(screen.getByRole("row", { name: "file.txt" }));
  await userEvent.selectOptions(
    screen.getByRole("combobox", { name: "Host" }),
    "second",
  );
  const upload = screen.getByRole("button", { name: "Upload →" });
  expect(upload).toBeDisabled();
  expect(screen.getByLabelText("Remote path")).toHaveValue("");
  await userEvent.click(upload);
  expect(be.called("sftp_upload")).toHaveLength(0);
  await act(async () => finishHome("/second-home"));
  expect(upload).toBeEnabled();
  await userEvent.click(upload);
  expect(be.called("sftp_upload")[0].args).toMatchObject({
    sftp: "second",
    remote: "/second-home/file.txt",
  });
});
test("snippet saving freezes edits and Delete, then restores the draft on failure", async () => {
  let fail!: (reason: unknown) => void;
  const snippet = { id: "snippet", label: "Status", script: "uptime" };
  fakeBackend({
    save_snippet: () => new Promise((_, reject) => (fail = reject)),
  });
  useStore.setState({ inventory: inventory({ snippets: [snippet] }) });
  render(<SnippetsView />);
  await userEvent.click(screen.getByRole("button", { name: "Edit Status" }));
  await userEvent.click(
    screen.getByRole("button", { name: "Save" }),
  );
  expect(screen.getByLabelText("Script")).toBeDisabled();
  expect(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "Delete",
    }),
  ).toBeDisabled();
  await act(async () => fail({ kind: "io", message: "Disk full" }));
  expect(screen.getByRole("alert")).toHaveTextContent("Disk full");
  expect(screen.getByLabelText("Script")).toBeEnabled();
  expect(screen.getByLabelText("Script")).toHaveValue("uptime");
});
test("settings cannot accept a newer edit while an older snapshot saves", async () => {
  let finish!: (value: unknown) => void;
  const be = fakeBackend({
    save_settings: () => new Promise((r) => (finish = r)),
  });
  render(<SettingsView />);
  const minutes = await screen.findByLabelText("Auto-lock after (minutes)");
  await userEvent.clear(minutes);
  await userEvent.type(minutes, "30");
  await userEvent.click(screen.getByRole("button", { name: "Save settings" }));
  expect(minutes).toBeDisabled();
  expect(screen.getByRole("checkbox")).toBeDisabled();
  await act(async () => finish(null));
  expect(minutes).toBeEnabled();
  expect(minutes).toHaveValue(30);
  expect(be.called("save_settings")[0].args.settings).toMatchObject({
    auto_lock_minutes: 30,
  });
});
