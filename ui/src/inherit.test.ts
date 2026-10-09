import { expect, test } from "vitest";
import { emptySettings, type Group } from "./api";
import { groupPath, inheritedFrom } from "./inherit";

const g = (id: string, parent: string | null, s: Partial<Group["settings"]> = {}): Group => ({
  id,
  name: id.toUpperCase(),
  parent,
  settings: { ...emptySettings(), ...s },
});

const groups = [g("root", null, { username: "deploy", port: 2222 }), g("child", "root", { port: 22 }), g("leaf", "child")];

test("groupPath is root first", () => {
  expect(groupPath(groups, "leaf").map((x) => x.id)).toEqual(["root", "child", "leaf"]);
  expect(groupPath(groups, null)).toEqual([]);
});

test("nearest group wins and sources are named", () => {
  const got = inheritedFrom(groups, "leaf");
  expect(got.port).toBe(22);
  expect(got.username).toBe("deploy");
  expect(got.from).toEqual({ port: "CHILD", username: "ROOT" });
});

test("ungrouped hosts inherit nothing", () => {
  expect(inheritedFrom(groups, null)).toEqual({ from: {} });
});
