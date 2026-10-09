import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { useStore } from "../store";
import { fakeBackend } from "../test/backend";
import SettingsView from "./SettingsView";

beforeEach(() => useStore.getState().reset());

test("saves auto-lock and session log settings", async () => {
  const be = fakeBackend({ save_settings: () => null });
  render(<SettingsView />);
  const minutes = await screen.findByLabelText("Auto-lock after (minutes)");
  expect(minutes).toHaveValue(15);
  await userEvent.clear(minutes);
  await userEvent.type(minutes, "5");
  await userEvent.click(screen.getByRole("checkbox", { name: /Record session logs/ }));
  await userEvent.click(screen.getByRole("button", { name: "Save settings" }));
  expect(be.called("save_settings")[0].args).toEqual({ settings: { session_logs: true, auto_lock_minutes: 5 } });
});

test("imports ~/.ssh/config and shows the summary", async () => {
  const be = fakeBackend({
    import_ssh_config: () => ({ created: 3, updated: 1, keys_imported: 2, warnings: ["ProxyCommand is not supported"] }),
  });
  render(<SettingsView />);
  await userEvent.click(await screen.findByRole("button", { name: "Import ~/.ssh/config" }));
  const status = await screen.findByRole("status");
  expect(status).toHaveTextContent("3 hosts added, 1 updated, 2 keys imported");
  expect(status).toHaveTextContent("ProxyCommand is not supported");
  expect(be.called("inventory").length).toBeGreaterThan(0);
});

test("exports the vault to a chosen file with an export password", async () => {
  const be = fakeBackend({ "plugin:dialog|save": () => "/tmp/backup.whx", vault_export: () => null });
  render(<SettingsView />);
  const form = await screen.findByRole("form", { name: "Export vault" });
  await userEvent.type(within(form).getByLabelText("Export password"), "exp");
  await userEvent.click(within(form).getByRole("button", { name: "Export…" }));
  expect(be.called("vault_export")[0].args).toEqual({ path: "/tmp/backup.whx", password: "exp" });
  expect(await within(form).findByRole("status")).toHaveTextContent("Exported");
});

test("change password validates confirmation", async () => {
  const be = fakeBackend({ vault_change_password: () => null });
  render(<SettingsView />);
  const form = await screen.findByRole("form", { name: "Change master password" });
  await userEvent.type(within(form).getByLabelText("Current password"), "old");
  await userEvent.type(within(form).getByLabelText("New password"), "new1");
  await userEvent.type(within(form).getByLabelText("Confirm new password"), "new2");
  await userEvent.click(within(form).getByRole("button", { name: "Change password" }));
  expect(within(form).getByRole("alert")).toHaveTextContent("do not match");
  await userEvent.clear(within(form).getByLabelText("Confirm new password"));
  await userEvent.type(within(form).getByLabelText("Confirm new password"), "new1");
  await userEvent.click(within(form).getByRole("button", { name: "Change password" }));
  expect(be.called("vault_change_password")[0].args).toEqual({ old: "old", new: "new1" });
});
