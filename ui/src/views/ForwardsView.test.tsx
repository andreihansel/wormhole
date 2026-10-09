import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import type { PortForward } from "../api";
import { useStore } from "../store";
import { fakeBackend } from "../test/backend";
import { host, inventory } from "../test/fixtures";
import ForwardsView from "./ForwardsView";

const db = host("db", "10.0.0.2");
const rule: PortForward = {
  id: "f1",
  label: "Postgres",
  host: db.id,
  kind: "local",
  bind_address: "127.0.0.1",
  bind_port: 15432,
  dest_host: "localhost",
  dest_port: 5432,
};

beforeEach(() => useStore.getState().reset());

async function setup(handlers = {}) {
  const be = fakeBackend({ inventory: () => inventory({ hosts: [db], forwards: [rule] }), ...handlers });
  await useStore.getState().refresh();
  render(<ForwardsView />);
  return be;
}

test("rule form saves a dynamic SOCKS forward", async () => {
  const be = await setup({ save_forward: ({ forward }: Record<string, unknown>) => forward });
  await userEvent.click(screen.getByRole("button", { name: "New rule" }));
  const form = screen.getByRole("form", { name: "Port forward" });
  await userEvent.type(within(form).getByLabelText("Name"), "SOCKS");
  await userEvent.selectOptions(within(form).getByLabelText("Type"), "dynamic");
  await userEvent.selectOptions(within(form).getByLabelText("Host"), db.id);
  await userEvent.type(within(form).getByLabelText("Bind port"), "1080");
  expect(within(form).queryByLabelText("Destination host")).not.toBeInTheDocument();
  await userEvent.click(within(form).getByRole("button", { name: "Save" }));
  expect(be.called("save_forward")[0].args.forward).toMatchObject({ label: "SOCKS", kind: "dynamic", host: db.id, bind_port: 1080 });
});

test("start/stop toggle reflects state", async () => {
  const be = await setup({
    forward_start: () => ({ rule: "f1", listen: "127.0.0.1:15432" }),
    forward_stop: () => null,
  });
  const card = screen.getByRole("article", { name: "Postgres" });
  expect(card).toHaveTextContent("127.0.0.1:15432 → localhost:5432 via db");
  await userEvent.click(within(card).getByRole("button", { name: "Start" }));
  expect(await within(card).findByText("Listening on 127.0.0.1:15432")).toBeInTheDocument();
  await userEvent.click(within(card).getByRole("button", { name: "Stop" }));
  expect(be.called("forward_stop")[0].args).toEqual({ rule: "f1" });
  expect(within(card).getByRole("button", { name: "Start" })).toBeInTheDocument();
});

test("start failures are shown", async () => {
  await setup({
    forward_start: () => {
      throw { kind: "forward_failed", message: "cannot listen on 127.0.0.1:15432: address in use" };
    },
  });
  const card = screen.getByRole("article", { name: "Postgres" });
  await userEvent.click(within(card).getByRole("button", { name: "Start" }));
  expect(await within(card).findByRole("alert")).toHaveTextContent("address in use");
});

test("delete a rule after confirming", async () => {
  const be = await setup({ delete_forward: () => null });
  const card = screen.getByRole("article", { name: "Postgres" });
  await userEvent.click(within(card).getByRole("button", { name: "Delete" }));
  await userEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));
  expect(be.called("delete_forward")[0].args).toEqual({ id: "f1" });
});
