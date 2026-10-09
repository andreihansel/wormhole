import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => cleanup());

// Browser APIs used by Radix layout primitives; jsdom has no layout engine.
class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver = TestResizeObserver;

// jsdom cannot create GPU or 2D contexts; graphics lifecycle has dedicated tests.
HTMLCanvasElement.prototype.getContext = (() =>
  null) as typeof HTMLCanvasElement.prototype.getContext;

// jsdom has no media decoder; lifecycle tests replace these with spies.
HTMLMediaElement.prototype.play = () => Promise.resolve();
HTMLMediaElement.prototype.pause = () => {};
HTMLMediaElement.prototype.load = () => {};
