import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import Confirm from "./Confirm";
import { Button, Overlay } from "./ui";
test("destructive confirmation initially focuses Cancel and keeps focus inside", async () => {
  render(
    <Confirm title="Remove server?" onCancel={() => {}} onConfirm={() => {}} />,
  );
  expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  await userEvent.tab({ shift: true });
  expect(screen.getByRole("button", { name: "Delete" })).toHaveFocus();
  await userEvent.tab();
  expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
});
test("an overlay cannot dismiss with Escape while work is pending", async () => {
  const dismiss = vi.fn();
  render(
    <Overlay title="Import key" onDismiss={dismiss}>
      <Button busy>Importing…</Button>
    </Overlay>,
  );
  await userEvent.keyboard("{Escape}");
  expect(dismiss).not.toHaveBeenCalled();
});
