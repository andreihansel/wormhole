import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test } from "vitest";
import { useStore } from "../store";
import { fakeBackend } from "../test/backend";
import { host, inventory } from "../test/fixtures";
import SnippetsView from "./SnippetsView";

const web = host("web", "10.0.0.1");
const db = host("db", "10.0.0.2");
const restart = {
  id: "sn1",
  label: "Restart unit",
  script: "systemctl restart {{unit}}",
};

beforeEach(() => useStore.getState().reset());

async function setup(handlers = {}) {
  const be = fakeBackend({
    inventory: () => inventory({ hosts: [web, db], snippets: [restart] }),
    snippet_variables: ({ script }: Record<string, unknown>) =>
      [...String(script).matchAll(/\{\{\s*([\w-]+)\s*\}\}/g)].map((m) => m[1]),
    ...handlers,
  });
  await useStore.getState().refresh();
  render(<SnippetsView />);
  return be;
}

test("create a snippet", async () => {
  const be = await setup({
    save_snippet: ({ snippet }: Record<string, unknown>) => snippet,
  });
  await userEvent.click(screen.getByRole("button", { name: "New snippet" }));
  const form = screen.getByRole("form", { name: "Snippet" });
  await userEvent.type(within(form).getByLabelText("Name"), "Disk usage");
  await userEvent.type(within(form).getByLabelText("Script"), "df -h");
  await userEvent.click(within(form).getByRole("button", { name: "Save" }));
  expect(be.called("save_snippet")[0].args.snippet).toMatchObject({
    label: "Disk usage",
    script: "df -h",
  });
});

test("run prompts for variables and hosts, then shows per-host results", async () => {
  const be = await setup({
    run_snippet: () => [
      { host: web.id, stdout: "ok\n", stderr: "", exit_status: 0, error: null },
      {
        host: db.id,
        stdout: "",
        stderr: "",
        exit_status: null,
        error: { kind: "auth_failed", message: "authentication failed" },
      },
    ],
  });
  await userEvent.click(
    screen.getByRole("button", { name: "Run Restart unit" }),
  );
  const dialog = screen.getByRole("dialog", { name: "Run Restart unit" });
  const run = within(dialog).getByRole("button", { name: "Run" });
  expect(run).toBeDisabled();
  await userEvent.type(await within(dialog).findByLabelText("unit"), "nginx");
  await userEvent.click(within(dialog).getByRole("checkbox", { name: "web" }));
  await userEvent.click(within(dialog).getByRole("checkbox", { name: "db" }));
  await userEvent.click(run);
  expect(be.called("run_snippet")[0].args).toEqual({
    script: restart.script,
    vars: { unit: "nginx" },
    hosts: [web.id, db.id],
  });
  const results = await within(dialog).findByRole("list", { name: "Results" });
  expect(
    within(results).getByRole("listitem", { name: "web" }),
  ).toHaveTextContent("exit 0");
  expect(
    within(results).getByRole("listitem", { name: "web" }),
  ).toHaveTextContent("ok");
  expect(
    within(results).getByRole("listitem", { name: "db" }),
  ).toHaveTextContent("authentication failed");
});

test("delete a snippet after confirming", async () => {
  const be = await setup({ delete_snippet: () => null });
  await userEvent.click(
    screen.getByRole("button", { name: "Edit Restart unit" }),
  );
  await userEvent.click(screen.getByRole("button", { name: "Delete" }));
  await userEvent.click(
    within(screen.getByRole("alertdialog")).getByRole("button", {
      name: "Delete",
    }),
  );
  expect(be.called("delete_snippet")[0].args).toEqual({ id: "sn1" });
});

test("Escape closes the run overlay and restores focus to its launcher", async () => {
  await setup();
  const launcher = screen.getByRole("button", { name: "Run Restart unit" });
  await userEvent.click(launcher);
  await screen.findByLabelText("unit");
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(launcher).toHaveFocus();
});
