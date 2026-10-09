import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { useStore } from "../store";
import { fakeBackend } from "../test/backend";
import { group, host, inventory } from "../test/fixtures";
import HostsView from "./HostsView";

const prod = group("Production", null, { username: "deploy", port: 2222 });
const eu = group("EU", prod.id);
const staging = group("Staging");
const web = host("web-eu", "10.0.0.1", { group: eu.id, tags: ["nginx"] });
const db = host("db-prod", "10.0.0.2", { group: prod.id, tags: ["postgres"] });
const test1 = host("test-box", "192.168.1.5", { group: staging.id });

async function setup(handlers = {}) {
  const inv = inventory({ groups: [prod, eu, staging], hosts: [web, db, test1] });
  const be = fakeBackend({ inventory: () => inv, ...handlers });
  useStore.getState().reset();
  await useStore.getState().refresh();
  render(<HostsView />);
  return be;
}

function hostCards() {
  return screen.queryAllByRole("article").map((a) => within(a).getByRole("heading").textContent);
}

beforeEach(() => useStore.getState().reset());

test("sidebar shows the group tree and selecting a group filters to it and its subgroups", async () => {
  await setup();
  const tree = screen.getByRole("tree", { name: "Groups" });
  const prodItem = within(tree).getByRole("treeitem", { name: /Production/ });
  expect(within(prodItem).getByRole("treeitem", { name: /EU/ })).toBeInTheDocument();
  expect(hostCards()).toEqual(["db-prod", "test-box", "web-eu"]);
  await userEvent.click(within(prodItem).getAllByRole("button")[0]);
  expect(hostCards()).toEqual(["db-prod", "web-eu"]);
});

test("search filters by label, address and tag", async () => {
  await setup();
  const search = screen.getByRole("searchbox", { name: "Search hosts" });
  await userEvent.type(search, "postgres");
  expect(hostCards()).toEqual(["db-prod"]);
  await userEvent.clear(search);
  await userEvent.type(search, "192.168");
  expect(hostCards()).toEqual(["test-box"]);
});

test("host form validates the port", async () => {
  const be = await setup({ save_host: () => null });
  await userEvent.click(screen.getByRole("button", { name: "New host" }));
  const form = screen.getByRole("form", { name: "Host" });
  await userEvent.type(within(form).getByLabelText("Address"), "example.com");
  await userEvent.type(within(form).getByLabelText("Port"), "70000");
  await userEvent.click(within(form).getByRole("button", { name: "Save" }));
  expect(within(form).getByRole("alert")).toHaveTextContent("Port must be between 1 and 65535");
  expect(be.called("save_host")).toHaveLength(0);
});

test("host form shows inherited values as placeholders", async () => {
  await setup();
  await userEvent.click(screen.getByRole("button", { name: "Edit web-eu" }));
  const form = screen.getByRole("form", { name: "Host" });
  expect(within(form).getByLabelText("Username")).toHaveAttribute("placeholder", "deploy (from Production)");
  expect(within(form).getByLabelText("Port")).toHaveAttribute("placeholder", "2222 (from Production)");
});

test("saving a new host sends it to the backend and refreshes", async () => {
  const be = await setup({ save_host: ({ host }: Record<string, unknown>) => host });
  await userEvent.click(screen.getByRole("button", { name: "New host" }));
  const form = screen.getByRole("form", { name: "Host" });
  await userEvent.type(within(form).getByLabelText("Label"), "cache");
  await userEvent.type(within(form).getByLabelText("Address"), "10.0.0.9");
  await userEvent.type(within(form).getByLabelText("Tags"), "redis, eu");
  await userEvent.click(within(form).getByRole("button", { name: "Save" }));
  const saved = be.called("save_host")[0].args.host as Record<string, unknown>;
  expect(saved).toMatchObject({ label: "cache", address: "10.0.0.9", tags: ["redis", "eu"] });
  expect(be.called("inventory").length).toBeGreaterThan(1);
});

test("connect opens a terminal tab", async () => {
  const be = await setup({ open_ssh_session: () => "session-1" });
  await userEvent.click(screen.getByRole("button", { name: "Connect to db-prod" }));
  expect(be.called("open_ssh_session")[0].args).toMatchObject({ host: db.id });
  const s = useStore.getState();
  expect(s.tabs).toEqual([{ session: "session-1", title: "db-prod", host: db.id, closed: false }]);
  expect(s.view).toBe("terminals");
});

test("connection errors are shown on the host", async () => {
  await setup({
    open_ssh_session: () => {
      throw { kind: "auth_failed", message: "authentication failed for 10.0.0.2" };
    },
  });
  await userEvent.click(screen.getByRole("button", { name: "Connect to db-prod" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("authentication failed");
  expect(useStore.getState().tabs).toEqual([]);
});

test("deleting a host asks for confirmation", async () => {
  const be = await setup({ delete_host: () => null });
  await userEvent.click(screen.getByRole("button", { name: "Edit test-box" }));
  await userEvent.click(screen.getByRole("button", { name: "Delete" }));
  const dialog = screen.getByRole("alertdialog");
  expect(dialog).toHaveTextContent("Delete test-box?");
  await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(be.called("delete_host")).toHaveLength(0);
  await userEvent.click(screen.getByRole("button", { name: "Delete" }));
  await userEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Delete" }));
  expect(be.called("delete_host")[0].args).toEqual({ id: test1.id });
});
