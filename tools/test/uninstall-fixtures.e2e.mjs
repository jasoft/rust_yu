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
  // 夹具目录现在包含多套 NSIS 深扫夹具；本脚本只勾选并安装自己需要的两个，
  // 不再假设目录数量恰好等于 2，也不使用“全部安装”。
  const fixtureCatalog = page.getByTestId("developer-fixture");
  await waitFor(async () => (await fixtureCatalog.count()) >= fixtures.length, 30_000, "fixture catalog");
  for (const fixture of fixtures) {
    const button = page.locator(`[data-testid="developer-fixture"][data-fixture-id="${fixture.id}"]`);
    assert.equal(await button.isEnabled(), true, `${fixture.id} installer must be available`);
    if ((await button.getAttribute("aria-pressed")) !== "true") await button.click();
    assert.equal(await button.getAttribute("aria-pressed"), "true", `${fixture.id} must be selected`);
  }

  // “安装所选”是夹具卡片下方的 primary-button；“全部安装”才是 developer-install-all。
  await page.locator(".developer-fixtures footer .primary-button").click();
  await page.getByTestId("developer-confirm-install").click();

  for (const fixture of fixtures) {
    const result = page.locator(
      `[data-testid="developer-fixture"][data-fixture-id="${fixture.id}"] [data-testid="developer-fixture-result"]`,
    );
    await waitFor(async () => {
      if ((await result.count()) !== 1) return false;
      return ["installed", "reboot_required"].includes(await result.getAttribute("data-status"));
    }, 360_000, `${fixture.id} installation result`);
  }

  return Promise.all(fixtures.map(async (fixture) => {
    const result = page.locator(
      `[data-testid="developer-fixture"][data-fixture-id="${fixture.id}"] [data-testid="developer-fixture-result"]`,
    );
    return { fixture_id: fixture.id, status: await result.getAttribute("data-status") };
  }));
}

async function uninstallFixture(page, fixture, artifactIndex) {
  // 夹具安装完成后页面会自动刷新列表，但后台刷新可能复用安装前启动的扫描，
  // 导致新装夹具暂时不可见；显式点击刷新并等它完成再搜索。
  const refreshButton = page.locator(".apps-page .header-actions .icon-button").first();
  await refreshButton.click();
  await page.waitForFunction(
    () => {
      const button = document.querySelector(".apps-page .header-actions .icon-button");
      return button instanceof HTMLButtonElement && !button.disabled;
    },
    null,
    { timeout: 300_000 },
  );

  const search = page.getByTestId("program-search");
  await search.fill(fixture.programName);
  const row = await selectProgramRow(page, fixture.programName);
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
  const uninstallerState = (await uninstallerRow.getAttribute("data-state")) ?? "";
  if (uninstallerState === "failed") {
    // 失败时必须把后端返回的错误带出来，否则整轮生命周期测试难以定位。
    const errorText = await page.locator(".uninstall-error").first().textContent().catch(() => "");
    await screenshot(page, `debug-uninstall-failed-${slug(fixture.programName)}.png`);
    throw new Error(`${fixture.programName} 卸载器执行失败：${errorText?.trim() ?? "（未捕获到错误信息）"}`);
  }
  assert.ok(["active", "done"].includes(uninstallerState));
  await screenshot(page, `${String(artifactIndex).padStart(2, "0")}-${slug(fixture.programName)}-uninstalling.png`);
  // 该状态覆盖“运行卸载器 + 核验移除 + 按名称扫描残留”三个后端阶段；
  // 大型 AppData/Program Files 的残留扫描可能超过 10 分钟，预算从卸载页出现起算。
  await waitFor(async () => (await scan.getAttribute("data-scan-status")) === "complete", 900_000, `${fixture.programName} residue scan`);
  const scanCategories = await scan.locator(".scan-location").evaluateAll((nodes) => nodes.map((node) => node.textContent?.replace(/\s+/g, " ").trim() ?? ""));
  assert.equal(scanCategories.length, 4, "all four scan categories must remain visible");

  // 这是本回归测试的核心：扫描结束后不得在用户来不及查看时自动跳页。
  await new Promise((resolve) => setTimeout(resolve, 1_200));
  assert.equal(await workflow.getAttribute("data-workflow-stage"), "scan", "scan results must wait for explicit Next");
  assert.equal(await page.getByTestId("workflow-scan-next").count(), 1, "the workflow must expose exactly one Next button");
  const scanLogLines = await scan.locator(".scan-live-log p").allTextContents();
  const scanCompletionLogCount = scanLogLines.filter((line) => line.includes("扫描完成")).length;
  assert.equal(scanCompletionLogCount, 1, "the residue scan completion event must be logged exactly once");
  await screenshot(page, `${String(artifactIndex).padStart(2, "0")}-${slug(fixture.programName)}-scan.png`);

  await page.getByTestId("workflow-scan-next").click();
  await waitFor(async () => ["review", "complete"].includes((await workflow.getAttribute("data-workflow-stage")) ?? ""), 30_000, `${fixture.programName} post-scan stage`);
  let reviewCount = 0;
  let reviewRows = [];
  if ((await workflow.getAttribute("data-workflow-stage")) === "review") {
    const review = page.getByTestId("workflow-review");
    const traceRows = review.locator(".trace-row");
    reviewCount = await traceRows.count();
    reviewRows = await traceRows.evaluateAll((nodes) => nodes.map((node) => {
      const checkbox = node.querySelector('input[type="checkbox"]');
      const confidence = node.querySelector(".confidence");
      return {
        path: node.querySelector(".trace-name small")?.textContent?.trim() ?? "",
        selected: checkbox instanceof HTMLInputElement && checkbox.checked,
        confidenceClass: confidence?.className ?? "",
        confidenceText: confidence?.textContent?.trim() ?? "",
      };
    }));

    if (fixture.id === "legacy-inno") {
      const installPrefix = "C:\\Program Files\\RustYu Legacy Test App\\";
      const installRows = reviewRows.filter((row) => row.path.startsWith(installPrefix));
      assert.ok(installRows.length > 0, "Legacy fixture must expose recorded install-directory residues");
      assert.ok(
        installRows.every((row) => row.selected && row.confidenceClass.split(/\s+/).includes("high")),
        "recorded install-directory residues must be high confidence and selected by default",
      );

      const appDataRows = reviewRows.filter((row) => row.path.includes("\\AppData\\"));
      assert.ok(appDataRows.length > 0, "Legacy fixture must expose its LocalAppData residue");
      assert.ok(
        appDataRows.every((row) => !row.selected && row.confidenceClass.split(/\s+/).includes("medium")),
        "name-matched AppData residues must be medium confidence and preserved by default",
      );
    }
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
  return { id: fixture.id, programName: fixture.programName, scanCategories, scanCompletionLogCount, reviewCount, reviewRows, summary };
}

// 在应用列表中选中指定程序。列表使用 react-data-grid：行 DOM 为 .rdg-row，
// 旧的 program-row/data-program-name 选择器在当前前端已不存在。
async function selectProgramRow(page, name) {
  const row = page.locator(".rdg-row", { hasText: name }).first();
  try {
    await row.waitFor({ state: "visible", timeout: 90_000 });
  } catch (error) {
    const diagnostics = await page.evaluate(() => ({
      rowCount: document.querySelectorAll(".rdg-row").length,
      firstRows: [...document.querySelectorAll(".rdg-row")]
        .slice(0, 5)
        .map((node) => node.textContent?.replace(/\s+/g, " ").trim().slice(0, 60) ?? ""),
      searchValue: document.querySelector('[data-testid="program-search"]')?.value ?? null,
    }));
    await screenshot(page, `debug-row-not-found-${slug(name)}.png`);
    throw new Error(`未找到程序行 ${name}，页面诊断：${JSON.stringify(diagnostics)}`);
  }
  // 点击名称单元格触发选中（复选框单元格会阻止行选中）。
  await row.locator(".program-grid-name-cell").click();
  return row;
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
