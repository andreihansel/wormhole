import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { useStore } from "../store";
import { fakeBackend } from "../test/backend";
import HostForm from "./HostForm";
beforeEach(() => useStore.getState().reset());
test("Escape asks before discarding a host draft and Keep editing retains it", async () => {
  fakeBackend();
  const close = vi.fn();
  render(<HostForm host={null} defaultGroup={null} onClose={close} />);
  await userEvent.type(screen.getByLabelText("Label"), "unsaved-host");
  await userEvent.keyboard("{Escape}");
  expect(screen.getByRole("alertdialog")).toHaveTextContent("Discard changes?");
  expect(close).not.toHaveBeenCalled();
  await userEvent.click(screen.getByRole("button", { name: "Keep editing" }));
  expect(screen.getByLabelText("Label")).toHaveValue("unsaved-host");
  await userEvent.keyboard("{Escape}");
  await userEvent.click(screen.getByRole("button", { name: "Discard" }));
  expect(close).toHaveBeenCalledOnce();
});
test("a failed host save retains the form values and explains the error", async () => {
  fakeBackend({
    save_host: () => {
      throw { kind: "io", message: "Disk full" };
    },
  });
  render(<HostForm host={null} defaultGroup={null} onClose={() => {}} />);
  await userEvent.type(screen.getByLabelText("Label"), "web");
  await userEvent.type(screen.getByLabelText("Address"), "example.com");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Disk full");
  expect(screen.getByLabelText("Address")).toHaveValue("example.com");
});
