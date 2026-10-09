import { StrictMode } from "react";
import { act, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import VaultBackdrop from "./VaultBackdrop";

let hidden = false;
let reduced = false;
let motionChange: (() => void) | undefined;
const removeMotionListener = vi.fn();
const play = vi.fn<() => Promise<void>>();
const pause = vi.fn();
const release = vi.fn();

beforeEach(() => {
  hidden = reduced = false;
  motionChange = undefined;
  removeMotionListener.mockReset();
  play.mockReset().mockResolvedValue();
  pause.mockReset();
  release.mockReset();
  vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(play);
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(pause);
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(release);
  vi.stubGlobal("matchMedia", () => ({
    get matches() {
      return reduced;
    },
    addEventListener: (_: string, listener: () => void) => {
      motionChange = listener;
    },
    removeEventListener: removeMotionListener,
  }));
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

test("keeps its matching poster until the video can play", () => {
  const { container } = render(<VaultBackdrop />);
  const video = container.querySelector("video")!;
  expect(container.querySelector(".vault-poster")).toBeInTheDocument();
  expect(video).not.toHaveClass("is-ready");
  fireEvent.canPlay(video);
  expect(video).toHaveClass("is-ready");
});

test("reduced motion uses the poster and responds to preference changes", () => {
  reduced = true;
  const { container } = render(<VaultBackdrop />);
  const video = container.querySelector("video")!;
  expect(video).not.toBeVisible();
  expect(play).not.toHaveBeenCalled();
  act(() => {
    reduced = false;
    motionChange?.();
  });
  expect(play).toHaveBeenCalledTimes(1);
  act(() => {
    reduced = true;
    motionChange?.();
  });
  expect(video).not.toBeVisible();
  expect(pause).toHaveBeenCalled();
});

test("pauses while hidden and resumes only when visible", () => {
  hidden = true;
  render(<VaultBackdrop />);
  expect(play).not.toHaveBeenCalled();
  expect(pause).toHaveBeenCalledTimes(1);
  act(() => {
    hidden = false;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  expect(play).toHaveBeenCalledTimes(1);
  act(() => {
    hidden = true;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  expect(pause).toHaveBeenCalledTimes(2);
});

test("decode failure and rejected playback leave the poster visible", async () => {
  play.mockRejectedValue(new Error("decoder unavailable"));
  const { container } = render(<VaultBackdrop />);
  const video = container.querySelector("video")!;
  await waitFor(() => expect(video).not.toBeVisible());
  expect(container.querySelector(".vault-poster")).toBeVisible();
});

test("a media error stops playback and reveals the poster", () => {
  const { container } = render(<VaultBackdrop />);
  const video = container.querySelector("video")!;
  fireEvent.canPlay(video);
  fireEvent.error(video);
  expect(video).not.toBeVisible();
  expect(pause).toHaveBeenCalled();
});

test("StrictMode cleans up listeners and releases media on unmount", () => {
  const add = vi.spyOn(document, "addEventListener");
  const remove = vi.spyOn(document, "removeEventListener");
  const { container, unmount } = render(
    <StrictMode>
      <VaultBackdrop />
    </StrictMode>,
  );
  const video = container.querySelector("video")!;
  expect(video).toHaveAttribute("src", "/media/black-hole.mp4");
  unmount();
  expect(video).not.toHaveAttribute("src");
  expect(release).toHaveBeenCalledTimes(2);
  expect(removeMotionListener).toHaveBeenCalledTimes(2);
  const added = add.mock.calls.filter(
    ([event]) => event === "visibilitychange",
  );
  const removed = remove.mock.calls.filter(
    ([event]) => event === "visibilitychange",
  );
  expect(added).toHaveLength(2);
  expect(removed).toEqual(added);
});

test("fixed-frame review mode never starts playback", () => {
  const { container } = render(<VaultBackdrop still />);
  expect(container.querySelector("video")).not.toBeVisible();
  expect(play).not.toHaveBeenCalled();
});

test("hiding during pending playback does not turn an expected abort into failure", async () => {
  let reject!: (reason: DOMException) => void;
  play.mockImplementationOnce(
    () =>
      new Promise<void>((_, fail) => {
        reject = fail;
      }),
  );
  const { container } = render(<VaultBackdrop />);
  act(() => {
    hidden = true;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await act(async () => {
    reject(new DOMException("Interrupted by pause", "AbortError"));
  });
  act(() => {
    hidden = false;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  expect(play).toHaveBeenCalledTimes(2);
  expect(container.querySelector("video")).toBeVisible();
});

test("an old play attempt cannot hide a newer successful resume", async () => {
  let reject!: (reason: Error) => void;
  play.mockImplementationOnce(
    () =>
      new Promise<void>((_, fail) => {
        reject = fail;
      }),
  );
  const { container } = render(<VaultBackdrop />);
  act(() => {
    reduced = true;
    motionChange?.();
  });
  act(() => {
    reduced = false;
    motionChange?.();
  });
  expect(play).toHaveBeenCalledTimes(2);
  await act(async () => {
    reject(new Error("Old playback request failed"));
  });
  expect(container.querySelector("video")).toBeVisible();
});
