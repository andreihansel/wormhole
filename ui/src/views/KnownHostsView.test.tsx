import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { fakeBackend } from "../test/backend";
import KnownHostsView from "./KnownHostsView";

test("lists, removes and imports known hosts", async () => {
  let known = [
    { id: "a", pattern: "[10.0.0.1]:2222", key: "ssh-ed25519 AAAAone" },
    { id: "b", pattern: "|1|salt|hash", key: "ssh-rsa AAAAtwo" },
  ];
  const be = fakeBackend({
    known_hosts: () => known,
    delete_known_host: ({ id }: Record<string, unknown>) => {
      known = known.filter((k) => k.id !== id);
      return null;
    },
    import_known_hosts: () => 3,
  });
  render(<KnownHostsView />);
  const row = await screen.findByRole("row", { name: "[10.0.0.1]:2222" });
  expect(row).toHaveTextContent("ssh-ed25519");
  expect(screen.getByRole("row", { name: "|1|salt|hash" })).toHaveTextContent("hashed");
  await userEvent.click(within(row).getByRole("button", { name: "Remove" }));
  await userEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: "Remove" }));
  expect(be.called("delete_known_host")[0].args).toEqual({ id: "a" });
  expect(screen.queryByRole("row", { name: "[10.0.0.1]:2222" })).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Import ~/.ssh/known_hosts" }));
  expect(await screen.findByRole("status")).toHaveTextContent("3 new");
});
