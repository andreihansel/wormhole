import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import type { SshKey } from "../api";
import { useStore } from "../store";
import { fakeBackend } from "../test/backend";
import { inventory } from "../test/fixtures";
import KeychainView from "./KeychainView";

const key: SshKey = { id: "k1", label: "laptop", public_key: "ssh-ed25519 AAAAC3Nz laptop", fingerprint: "SHA256:abc", certificate: null };

beforeEach(() => useStore.getState().reset());

async function setup(handlers = {}, inv = inventory({ keys: [key] })) {
  const be = fakeBackend({ inventory: () => inv, ...handlers });
  await useStore.getState().refresh();
  render(<KeychainView />);
  return be;
}

test("lists keys and copies the public key", async () => {
  const user = userEvent.setup();
  await setup();
  const card = screen.getByRole("article", { name: "laptop" });
  expect(card).toHaveTextContent("SHA256:abc");
  await user.click(within(card).getByRole("button", { name: "Copy public key" }));
  expect(await navigator.clipboard.readText()).toBe("ssh-ed25519 AAAAC3Nz laptop");
});

test("generate key", async () => {
  const be = await setup({ key_generate: () => key });
  await userEvent.click(screen.getByRole("button", { name: "Generate key" }));
  const form = screen.getByRole("form", { name: "Generate key" });
  await userEvent.type(within(form).getByLabelText("Label"), "laptop");
  await userEvent.selectOptions(within(form).getByLabelText("Type"), "rsa");
  await userEvent.click(within(form).getByRole("button", { name: "Generate" }));
  expect(be.called("key_generate")[0].args).toEqual({ label: "laptop", kind: { type: "rsa", bits: 4096 } });
});

test("import asks for a passphrase when the key needs one", async () => {
  let calls = 0;
  const be = await setup({
    key_import: ({ passphrase }: Record<string, unknown>) => {
      calls++;
      if (!passphrase) throw { kind: "needs_passphrase", message: "this key is encrypted; a passphrase is required" };
      return key;
    },
  });
  await userEvent.click(screen.getByRole("button", { name: "Import key" }));
  const form = screen.getByRole("form", { name: "Import key" });
  await userEvent.type(within(form).getByLabelText("Label"), "old");
  await userEvent.type(within(form).getByLabelText("Private key"), "-----BEGIN OPENSSH PRIVATE KEY-----");
  await userEvent.click(within(form).getByRole("button", { name: "Import" }));
  expect(await within(form).findByRole("alert")).toHaveTextContent("passphrase");
  await userEvent.type(within(form).getByLabelText("Passphrase"), "pp");
  await userEvent.click(within(form).getByRole("button", { name: "Import" }));
  expect(calls).toBe(2);
  expect(be.called("key_import")[1].args).toMatchObject({ label: "old", passphrase: "pp" });
});

test("create identity with key and password", async () => {
  const be = await setup({ save_identity: ({ identity }: Record<string, unknown>) => identity, set_password: () => null });
  await userEvent.click(screen.getByRole("button", { name: "New identity" }));
  const form = screen.getByRole("form", { name: "Identity" });
  await userEvent.type(within(form).getByLabelText("Label"), "ops");
  await userEvent.type(within(form).getByLabelText("Username"), "ops");
  await userEvent.selectOptions(within(form).getByLabelText("Key"), "k1");
  await userEvent.type(within(form).getByLabelText("Password"), "pw");
  await userEvent.click(within(form).getByRole("button", { name: "Save" }));
  const saved = be.called("save_identity")[0].args.identity as Record<string, unknown>;
  expect(saved).toMatchObject({ label: "ops", username: "ops", key: "k1" });
  expect(be.called("set_password")[0].args).toEqual({ owner: saved.id, password: "pw" });
});

test("deleting a key asks for confirmation", async () => {
  const be = await setup({ delete_key: () => null });
  const card = screen.getByRole("article", { name: "laptop" });
  await userEvent.click(within(card).getByRole("button", { name: "Delete" }));
  expect(screen.getByRole("alertdialog")).toHaveTextContent("Delete laptop?");
  await userEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));
  expect(be.called("delete_key")[0].args).toEqual({ id: "k1" });
});

test("deleting an identity asks for confirmation", async () => {
  const ident = { id: "i1", label: "ops", username: "ops", key: null };
  const be = await setup({ delete_identity: () => null }, inventory({ identities: [ident] }));
  await userEvent.click(within(screen.getByRole("article", { name: "ops" })).getByRole("button", { name: "Delete" }));
  await userEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));
  expect(be.called("delete_identity")[0].args).toEqual({ id: "i1" });
});
