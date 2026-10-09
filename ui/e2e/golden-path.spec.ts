import { expect, test } from "@playwright/test";

const shots = (name: string) => ({ path: `e2e/.out/screens/${name}.png` });

test("create vault → add host → connect → trust key → run a command", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByLabel("Master password", { exact: true })
    .fill("correct horse");
  await page
    .getByLabel("Confirm password", { exact: true })
    .fill("correct horse");
  await page.screenshot(shots("1-create-vault"));
  await page.getByRole("button", { name: "Create vault" }).click();

  await page.getByRole("button", { name: "New host" }).click();
  await page.getByLabel("Label").fill("web-1");
  await page.getByLabel("Address").fill("203.0.113.10");
  await page.getByLabel("Username").fill("deploy");
  await page.getByLabel("Tags").fill("prod, nginx");
  await page.screenshot(shots("2-host-form"));
  await page.getByRole("button", { name: "Save" }).click();
  await expect(
    page.getByRole("article").getByRole("heading", { name: "web-1" }),
  ).toBeVisible();
  await page.screenshot(shots("3-hosts"));

  await page.getByRole("button", { name: "Connect to web-1" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("SHA256:e2eFakeFingerprint");
  await page.screenshot(shots("4-host-key"));
  await dialog.getByRole("button", { name: "Trust and connect" }).click();

  const terminal = page.locator(".xterm-rows");
  await expect(terminal).toContainText("Welcome to web-1");
  await page.locator(".xterm").click();
  await page.keyboard.type("echo hello wormhole");
  await page.keyboard.press("Enter");
  await expect(terminal).toContainText(/hello wormhole\s*\$/);
  await page.screenshot(shots("5-terminal"));

  await page.getByRole("button", { name: "Lock" }).click();
  await expect(
    page.getByLabel("Master password", { exact: true }),
  ).toBeVisible();
});
