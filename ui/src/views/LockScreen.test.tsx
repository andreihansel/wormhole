import { StrictMode } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { fakeBackend } from "../test/backend";
import LockScreen from "./LockScreen";

beforeEach(() => localStorage.clear());

test("cycles the requested quotes once per lock-screen opening and wraps", () => {
  fakeBackend();
  const quotes = [
    "Beam me up, Scotty!",
    "Live long and prosper.",
    "To boldly go where no man has gone before.",
    "In space no one can hear you scream.",
    "Traveling through hyperspace ain't like dusting crops, boy!",
    "Beam me up, Scotty!",
  ];
  for (const quote of quotes) {
    const view = render(<LockScreen initialized onUnlocked={() => {}} />);
    expect(screen.getByText(`“${quote}”`)).toBeVisible();
    view.rerender(<LockScreen initialized onUnlocked={() => {}} />);
    expect(screen.getByText(`“${quote}”`)).toBeVisible();
    view.unmount();
  }
});

test("StrictMode counts one quote opening, and editing keeps that quote", async () => {
  fakeBackend();
  const view = render(
    <StrictMode><LockScreen initialized onUnlocked={() => {}} /></StrictMode>,
  );
  expect(screen.getByText("“Beam me up, Scotty!”")).toBeVisible();
  await userEvent.type(screen.getByLabelText("Master password"), "keep-quote");
  expect(screen.getByText("“Beam me up, Scotty!”")).toBeVisible();
  view.unmount();
  render(<LockScreen initialized onUnlocked={() => {}} />);
  expect(screen.getByText("“Live long and prosper.”")).toBeVisible();
});

test("first run asks to create a master password and confirms it", async () => {
  const be = fakeBackend({ vault_create: () => null });
  const onUnlocked = vi.fn();
  render(<LockScreen initialized={false} onUnlocked={onUnlocked} />);
  await userEvent.type(screen.getByLabelText("Master password"), "s3cret");
  await userEvent.type(screen.getByLabelText("Confirm password"), "different");
  await userEvent.click(screen.getByRole("button", { name: "Create vault" }));
  expect(screen.getByRole("alert")).toHaveTextContent("Passwords do not match");
  expect(be.called("vault_create")).toHaveLength(0);

  await userEvent.clear(screen.getByLabelText("Confirm password"));
  await userEvent.type(screen.getByLabelText("Confirm password"), "s3cret");
  await userEvent.click(screen.getByRole("button", { name: "Create vault" }));
  expect(be.called("vault_create")[0].args).toEqual({ password: "s3cret" });
  expect(onUnlocked).toHaveBeenCalled();
});

test("shows error on wrong password", async () => {
  fakeBackend({
    vault_unlock: () => {
      throw { kind: "bad_password", message: "incorrect password" };
    },
  });
  const onUnlocked = vi.fn();
  render(<LockScreen initialized onUnlocked={onUnlocked} />);
  await userEvent.type(screen.getByLabelText("Master password"), "nope{Enter}");
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "incorrect password",
  );
  expect(onUnlocked).not.toHaveBeenCalled();
});

test("unlock calls the backend then onUnlocked", async () => {
  const be = fakeBackend({ vault_unlock: () => null });
  const onUnlocked = vi.fn();
  render(<LockScreen initialized onUnlocked={onUnlocked} />);
  await userEvent.type(
    screen.getByLabelText("Master password"),
    "right{Enter}",
  );
  expect(be.called("vault_unlock")[0].args).toEqual({ password: "right" });
  expect(onUnlocked).toHaveBeenCalled();
});

test("password visibility preserves the entered password", async () => {
  fakeBackend();
  render(<LockScreen initialized onUnlocked={() => {}} />);
  const password = screen.getByLabelText("Master password");
  await userEvent.type(password, "keep-this");
  await userEvent.click(
    screen.getByRole("button", { name: "Show Master password" }),
  );
  expect(password).toHaveAttribute("type", "text");
  expect(password).toHaveValue("keep-this");
  await userEvent.click(
    screen.getByRole("button", { name: "Hide Master password" }),
  );
  expect(password).toHaveAttribute("type", "password");
});

test("pending unlock announces work and prevents a second submission", async () => {
  let finish!: () => void;
  const be = fakeBackend({
    vault_unlock: () =>
      new Promise<void>((resolve) => {
        finish = resolve;
      }),
  });
  render(<LockScreen initialized onUnlocked={() => {}} />);
  await userEvent.type(screen.getByLabelText("Master password"), "right");
  await userEvent.click(screen.getByRole("button", { name: "Unlock" }));
  expect(screen.getByRole("button", { name: /Unlocking/ })).toBeDisabled();
  await userEvent.type(screen.getByLabelText("Master password"), "{Enter}");
  expect(be.called("vault_unlock")).toHaveLength(1);
  finish();
});

test("the vault has a decorative full-screen video with no playback controls", () => {
  fakeBackend();
  const { container } = render(
    <LockScreen initialized onUnlocked={() => {}} />,
  );
  const video = container.querySelector("video");
  expect(video).toBeInTheDocument();
  expect(video).toHaveAttribute("loop");
  expect(video).toHaveAttribute("playsinline");
  expect(video).not.toHaveAttribute("controls");
  expect(video).toHaveAttribute("aria-hidden", "true");
  expect(screen.queryByRole("button", { name: /Pause|Resume/ })).toBeNull();
  expect(container.querySelector(".lock-card")).toHaveClass("vault-glass");
});
