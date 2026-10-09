import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test } from "vitest";
import { startEvents } from "../events";
import { useStore } from "../store";
import { fakeBackend } from "../test/backend";
import { host, inventory } from "../test/fixtures";
import SftpView from "./SftpView";

const web = host("web", "10.0.0.1");
let stop: () => void;
beforeEach(() => useStore.getState().reset());
afterEach(() => stop?.());

async function setup(handlers = {}) {
  const be = fakeBackend({
    inventory: () => inventory({ hosts: [web] }),
    home_dir: () => "/Users/me",
    local_list: ({ path }: Record<string, unknown>) =>
      path === "/Users/me"
        ? [
            {
              name: "Documents",
              path: "/Users/me/Documents",
              is_dir: true,
              size: 0,
            },
            {
              name: "notes.txt",
              path: "/Users/me/notes.txt",
              is_dir: false,
              size: 12,
            },
          ]
        : [],
    sftp_open: () => "sftp-1",
    sftp_home: () => "/home/wh",
    sftp_list: ({ path }: Record<string, unknown>) =>
      path === "/home/wh"
        ? [
            {
              name: "logs",
              path: "/home/wh/logs",
              kind: "dir",
              size: 0,
              mode: 0o755,
              mtime: 1,
            },
            {
              name: "app.conf",
              path: "/home/wh/app.conf",
              kind: "file",
              size: 2048,
              mode: 0o644,
              mtime: 1,
            },
          ]
        : [
            {
              name: "today.log",
              path: "/home/wh/logs/today.log",
              kind: "file",
              size: 5,
              mode: 0o644,
              mtime: 1,
            },
          ],
    sftp_upload: () => 12,
    sftp_download: () => 2048,
    ...handlers,
  });
  stop = await startEvents();
  await useStore.getState().refresh();
  render(<SftpView />);
  return be;
}

const rows = (pane: HTMLElement) =>
  within(pane)
    .queryAllByRole("row")
    .map((r) => r.getAttribute("aria-label"));

test("dual pane lists local and remote after choosing a host", async () => {
  const be = await setup();
  const local = await screen.findByRole("grid", { name: "Local files" });
  expect(
    await within(local).findByRole("row", { name: "notes.txt" }),
  ).toBeInTheDocument();
  await userEvent.selectOptions(screen.getByLabelText("Host"), web.id);
  const remote = await screen.findByRole("grid", { name: "Remote files" });
  expect(
    await within(remote).findByRole("row", { name: "app.conf" }),
  ).toBeInTheDocument();
  expect(rows(remote)).toEqual(["logs", "app.conf"]);
  expect(be.called("sftp_open")[0].args).toEqual({ host: web.id });
  await userEvent.dblClick(within(remote).getByRole("row", { name: "logs" }));
  expect(
    await within(remote).findByRole("row", { name: "today.log" }),
  ).toBeInTheDocument();
  expect(screen.getByLabelText("Remote path")).toHaveValue("/home/wh/logs");
});

test("upload sends the selected local file to the remote directory with progress", async () => {
  let release!: () => void;
  const be = await setup({
    sftp_upload: () => new Promise((r) => (release = () => r(12))),
  });
  await userEvent.selectOptions(screen.getByLabelText("Host"), web.id);
  const local = screen.getByRole("grid", { name: "Local files" });
  await userEvent.click(
    await within(local).findByRole("row", { name: "notes.txt" }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Upload →" }));
  const call = be.called("sftp_upload")[0].args;
  expect(call).toMatchObject({
    sftp: "sftp-1",
    local: "/Users/me/notes.txt",
    remote: "/home/wh/notes.txt",
  });
  await act(() =>
    be.emit({
      type: "transfer_progress",
      transfer: call.transfer as string,
      done: 6,
      total: 12,
    }),
  );
  expect(
    screen.getByRole("progressbar", { name: "notes.txt" }),
  ).toHaveAttribute("aria-valuenow", "50");
  await act(async () => release());
  expect(be.called("sftp_list").length).toBeGreaterThan(1);
});

test("download sends the selected remote file to the local directory", async () => {
  const be = await setup();
  await userEvent.selectOptions(screen.getByLabelText("Host"), web.id);
  const remote = await screen.findByRole("grid", { name: "Remote files" });
  await userEvent.click(
    await within(remote).findByRole("row", { name: "app.conf" }),
  );
  await userEvent.click(screen.getByRole("button", { name: "← Download" }));
  expect(be.called("sftp_download")[0].args).toMatchObject({
    remote: "/home/wh/app.conf",
    local: "/Users/me/app.conf",
  });
});

test("files can be selected and folders opened entirely with the keyboard", async () => {
  await setup();
  const file = await screen.findByRole("row", { name: "notes.txt" });
  file.focus();
  await userEvent.keyboard(" ");
  expect(file).toHaveAttribute("aria-selected", "true");
  const folder = screen.getByRole("row", { name: "Documents" });
  folder.focus();
  await userEvent.keyboard("{Enter}");
  await waitFor(() =>
    expect(screen.getByLabelText("Local path")).toHaveValue(
      "/Users/me/Documents",
    ),
  );
});
