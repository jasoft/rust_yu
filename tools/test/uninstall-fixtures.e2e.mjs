import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { chromium } from "playwright";

const options = parseArguments(process.argv.slice(2));
const endpoint = `http://127.0.0.1:${options.port}`;
const fixtures = [
  { id: "xplorer-msi", programName: "Xplorer" },
  { id: "legacy-inno", programName: "RustYu Legacy Test App" },
];
const evidence = {
  endpoint,
  startedAt: new Date().toISOString(),
  fixtures: [],
};

await mkdir(options.artifacts, { recursive: true });

let browser;
try {
  browser = await connectToWebView(endpoint, options.startupTimeoutMs);
  const page = await findRustYuPage(browser, options.startupTimeoutMs);
  page.setDefaultTimeout(30_000);

  await page.waitForFunction(() => "__TAURI_INTERNALS__" in window);
  await screenshot(page, "00-apps.png");

  await enableDeveloperMode(page);
  const installResults = await installFixtures(page);
  evidence.installResults = installResults;
  await screenshot(page, "01-fixtures-installed.png");

  await page.getByTestId("nav-apps").click();
  for (const [index, fixture] of fixtures.entries()) {
    const result = await uninstallFixture(page, fixture, index + 2);
    evidence.fixtures.push(result);
  }

  evidence.completedAt = new Date().toISOString();
  evidence.success = true;
  await writeEvidence();
  await page.close().catch(() => undefined);
} catch (error) {
  evidence.completedAt = new Date().toISOString();
  evidence.success = false;
  evidence.error = error instanceof Error ? `${error.name}: ${error.message}\n${error.stack ?? ""}` : String(error);
  await writeEvidence();
  throw error;
} finally {
  await browser?.close().catch(() => undefined);
}

async function enableDeveloperMode(page) {
  await page.getByTestId("nav-settings").click();
  const toggle = page.getByTestId("developer-mode-toggle");
  await toggle.waitFor({ state: "visible" });
  if ((await toggle.getAttribute("aria-checked")) !== "true") await toggle.click();
  await page.getByTestId("nav-developer").click();
  await page.getByTestId("developer-tools").waitFor({ state: "visible" });
}

async function installFixtures(page) {
  const fixtureButtons = page.getByTestId("developer-fixture");
  await waitFor(async () => (await fixtureButtons.count()) === fixtures.length, 30_000, "fixture catalog");
  for (const fixture of fixtures) {
    const button = page.locator(`[data-testid="developer-fixture"][data-fixture-id="${fixture.id}"]`);
    assert.equal(await button.isEnabled(), true, `${fixture.id} installer must be available`);
  }

  await page.getByTestId("developer-install-all").click();
  await page.getByTestId("developer-confirm-install").click();
  await waitFor(async () => {
    const results = page.getByTestId("developer-fixture-result");
    if ((await results.count()) !== fixtures.length) return false;
    const statuses = await results.evaluateAll((nodes) => nodes.map((node) => node.getAttribute("data-status")));
    return statuses.every((status) => status === "installed" || status === "reboot_required");
  }, 360_000, "fixture installation results");

  return page.getByTestId("developer-fixture-result").evaluateAll((nodes) =>
    nodes.map((node) => ({ status: node.getAttribute("data-status"), text: node.textContent?.trim() ?? "" })),
  );
}

async function uninstallFixture(page, fixture, artifactIndex) {
  const search = page.getByTestId("program-search");
  await search.fill(fixture.programName);
  const row = page.locator(`[data-testid="program-row"][data-program-name="${fixture.programName}"]`);
  await row.waitFor({ state: "visible", timeout: 90_000 });
  await row.click();
  await page.getByTestId("program-uninstall").click();

  const workflow = page.getByTestId("uninstall-workflow");
  await workflow.waitFor({ state: "visible" });
  assert.equal(await workflow.getAttribute("data-workflow-stage"), "confirm");
  await page.getByTestId("workflow-start").click();

  const scan = page.getByTestId("workflow-scan");
  await scan.waitFor({ state: "visible", timeout: 420_000 });
  assert.equal(await workflow.getAttribute("data-workflow-stage"), "scan", "built-in uninstall must open directly inside the scan page");
  assert.equal(await page.getByTestId("workflow-uninstall").count(), 0, "legacy uninstall progress page must not exist");
  assert.equal(await page.getByTestId("workflow-cleanup").count(), 0, "legacy cleanup progress page must not exist");
  const uninstallerRow = page.getByTestId("workflow-uninstaller-row");
  await uninstallerRow.waitFor({ state: "visible" });
  assert.ok(["active", "done"].includes((await uninstallerRow.getAttribute("data-state")) ?? ""));
  await screenshot(page, `${String(artifactIndex).padStart(2, "0")}-${slug(fixture.programName)}-uninstalling.png`);
  await waitFor(async () => (await scan.getAttribute("data-scan-status")) === "complete", 420_000, `${fixture.programName} residue scan`);
  const scanCategories = await scan.locator(".scan-location").evaluateAll((nodes) => nodes.map((node) => node.textContent?.replace(/\s+/g, " ").trim() ?? ""));
  assert.equal(scanCategories.length, 4, "all four scan categories must remain visible");

  // 这是本回归测试的核心：扫描结束后不得在用户来不及查看时自动跳页。
  await new Promise((resolve) => setTimeout(resolve, 1_200));
  assert.equal(await workflow.getAttribute("data-workflow-stage"), "scan", "scan results must wait for explicit Next");
  assert.equal(await page.getByTestId("workflow-scan-next").count(), 1, "the workflow must expose exactly one Next button");
  await screenshot(page, `${String(artifactIndex).padStart(2, "0")}-${slug(fixture.programName)}-scan.png`);

  await page.getByTestId("workflow-scan-next").click();
  await waitFor(async () => ["review", "complete"].includes((await workflow.getAttribute("data-workflow-stage")) ?? ""), 30_000, `${fixture.programName} post-scan stage`);
  let reviewCount = 0;
  if ((await workflow.getAttribute("data-workflow-stage")) === "review") {
    const review = page.getByTestId("workflow-review");
    reviewCount = await review.locator(".trace-row").count();
    await screenshot(page, `${String(artifactIndex).padStart(2, "0")}-${slug(fixture.programName)}-review.png`);
    const clean = page.getByTestId("workflow-clean");
    if (await clean.isEnabled()) {
      await clean.click();
      await page.getByTestId("workflow-confirm-clean").click();
      assert.equal(await page.getByTestId("workflow-cleanup").count(), 0, "cleanup must remain on the review page");
    } else {
      await page.getByTestId("workflow-skip-cleanup").click();
    }
  }

  await page.getByTestId("workflow-complete").waitFor({ state: "visible", timeout: 420_000 });
  await screenshot(page, `${String(artifactIndex).padStart(2, "0")}-${slug(fixture.programName)}-complete.png`);
  const summary = await page.getByTestId("workflow-complete").locator(".summary-grid").innerText();
  await page.getByTestId("workflow-done").click();
  await page.getByTestId("program-search").waitFor({ state: "visible" });
  return { id: fixture.id, programName: fixture.programName, scanCategories, reviewCount, summary };
}

async function connectToWebView(url, timeoutMs) {
  let lastError;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      return await chromium.connectOverCDP(url);
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => setTimeout(resolve, 750));
    }
  }
  throw new Error(`WebView2 CDP endpoint did not become ready within ${timeoutMs} ms: ${lastError}`);
}

async function findRustYuPage(connectedBrowser, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const context of connectedBrowser.contexts()) {
      for (const page of context.pages()) {
        if (await page.getByTestId("nav-apps").count().catch(() => 0)) return page;
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error("Connected to WebView2, but the Rust Yu page was not found");
}

async function waitFor(predicate, timeoutMs, description) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      if (await predicate()) return;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(`Timed out waiting for ${description}${lastError ? `: ${lastError}` : ""}`);
}

async function screenshot(page, filename) {
  await page.screenshot({ path: path.join(options.artifacts, filename), fullPage: true });
}

async function writeEvidence() {
  await writeFile(path.join(options.artifacts, "evidence.json"), `${JSON.stringify(evidence, null, 2)}\n`, "utf8");
}

function slug(value) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function parseArguments(args) {
  const parsed = { port: 9223, artifacts: path.resolve("target/test-logs/uninstall-fixtures-e2e"), startupTimeoutMs: 120_000 };
  for (let index = 0; index < args.length; index += 1) {
    const value = args[index];
    if (value === "--port") parsed.port = Number(args[++index]);
    else if (value === "--artifacts") parsed.artifacts = path.resolve(args[++index]);
    else if (value === "--startup-timeout-ms") parsed.startupTimeoutMs = Number(args[++index]);
    else throw new Error(`Unknown argument: ${value}`);
  }
  assert.ok(Number.isInteger(parsed.port) && parsed.port > 0 && parsed.port < 65536, "--port must be a valid TCP port");
  assert.ok(Number.isFinite(parsed.startupTimeoutMs) && parsed.startupTimeoutMs > 0, "--startup-timeout-ms must be positive");
  return parsed;
}
