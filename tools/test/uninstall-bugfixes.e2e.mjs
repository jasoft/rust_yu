import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { chromium } from "playwright";

// 针对“卸载流程”两个已修复缺陷的 GUI 回归验证（浏览器自动化，操作真实 WebView）：
// 1. 失效安装（没有可用卸载命令的 Legacy 程序）不得进入标准卸载流程。
// 2. 卸载流程内出现的错误在返回应用列表后不得残留在列表窗口。
// 场景 2 通过 addInitScript 注入 test double：仅拦截 plan_uninstall 命令并返回失败，
// 其余 Tauri 命令全部透传，因此不触发任何真实的文件/注册表删除。

const argv = process.argv.slice(2);
function argValue(name, fallback) {
  const index = argv.indexOf(name);
  return index >= 0 && argv[index + 1] ? argv[index + 1] : fallback;
}

const port = argValue("--port", "9223");
const artifactRoot = argValue("--artifacts", "target/test-logs/uninstall-bugfixes-e2e");
const startupTimeoutMs = Number(argValue("--startup-timeout-ms", "120000"));
const endpoint = `http://127.0.0.1:${port}`;

const evidence = {
  endpoint,
  startedAt: new Date().toISOString(),
  bugfix_no_uninstaller: { status: "pending" },
  bugfix_error_residue: { status: "pending" },
  force_name_search: { status: "pending" },
};

await mkdir(artifactRoot, { recursive: true });

let browser;
try {
  browser = await connectToWebView(endpoint, startupTimeoutMs);
  const page = await findRustYuPage(browser, startupTimeoutMs);
  page.setDefaultTimeout(30_000);
  await page.waitForFunction(() => "__TAURI_INTERNALS__" in window);

  await verifyNoUninstallerProgramIsBlocked(page);
  await verifyUninstallErrorIsClearedOnExit(page);
  await verifyForceUninstallNameSearch(page);

  evidence.completedAt = new Date().toISOString();
  evidence.success = true;
  await writeEvidence();
  console.log("卸载缺陷修复 E2E 验证通过。");
} catch (error) {
  evidence.completedAt = new Date().toISOString();
  evidence.success = false;
  evidence.error = error instanceof Error ? `${error.name}: ${error.message}\n${error.stack ?? ""}` : String(error);
  await writeEvidence();
  console.error(evidence.error);
  process.exit(1);
} finally {
  await browser?.close().catch(() => undefined);
}

async function listPrograms(page) {
  return page.evaluate(() =>
    window.__TAURI_INTERNALS__.invoke("list_programs", {
      options: { source: "all", search: null, refresh: false },
    }),
  );
}

async function selectProgramByName(page, name) {
  await page.getByTestId("program-search").fill(name);
  // 列表使用 react-data-grid，行 DOM 为 .rdg-row；点击名称单元格触发选中。
  const row = page.locator(".rdg-row", { hasText: name }).first();
  try {
    await row.waitFor({ state: "visible", timeout: 15_000 });
  } catch (error) {
    const diagnostics = await page.evaluate(() => ({
      rowCount: document.querySelectorAll(".rdg-row").length,
      firstRows: [...document.querySelectorAll(".rdg-row")]
        .slice(0, 5)
        .map((node) => node.textContent?.replace(/\s+/g, " ").trim().slice(0, 60) ?? ""),
      searchValue: document.querySelector('[data-testid="program-search"]')?.value ?? null,
      workflowStage: document.querySelector('[data-testid="uninstall-workflow"]')
        ?.getAttribute("data-workflow-stage") ?? null,
    }));
    await screenshot(page, "debug-select-failure.png");
    throw new Error(`未找到程序行 ${name}，页面诊断：${JSON.stringify(diagnostics)}`);
  }
  await row.locator(".program-grid-name-cell").click();
}

async function screenshot(page, name) {
  const file = path.join(artifactRoot, name);
  await page.screenshot({ path: file, fullPage: false });
  return file;
}

// 场景 1：失效安装保留可点击的卸载按钮；点击后弹出“已损坏”警告对话框，
// 确认后进入按名称分析残留的只读扫描流程（不运行任何卸载器）。
async function verifyNoUninstallerProgramIsBlocked(page) {
  const record = evidence.bugfix_no_uninstaller;
  const response = await listPrograms(page);
  const programs = response.programs ?? [];
  const target = programs.find(
    (program) =>
      program.is_broken_installation ||
      (program.uninstall_kind === "legacy" &&
        !program.uninstall_string?.trim() &&
        !program.quiet_uninstall_string?.trim()),
  );
  if (!target) {
    record.status = "skipped";
    record.reason = "当前系统清单中不存在失效安装（无卸载命令或被分析器判定失效），无法构造该场景";
    return;
  }
  record.program = { id: target.id, name: target.name };
  record.signal = target.is_broken_installation ? "is_broken_installation" : "missing_uninstall_command";

  await selectProgramByName(page, target.name);
  const uninstallButton = page.getByTestId("program-uninstall");
  await uninstallButton.waitFor({ state: "visible" });
  record.uninstall_button_enabled = (await uninstallButton.isDisabled()) === false;
  assert.equal(
    record.uninstall_button_enabled,
    true,
    "失效安装的卸载按钮必须保持可点击",
  );
  await uninstallButton.click();

  const dialog = page.getByTestId("broken-uninstall-dialog");
  await dialog.waitFor({ state: "visible" });
  record.warning_dialog_visible = true;
  record.evidence_dialog = await screenshot(page, "01-broken-uninstall-dialog.png");

  await page.getByTestId("broken-analyze-start").click();
  const workflow = page.getByTestId("analysis-workflow");
  await workflow.waitFor({ state: "visible" });
  const scan = page.getByTestId("analysis-scan");
  await scan.waitFor({ state: "visible" });
  // 按名称扫描为只读操作；等待扫描完成后断言结果页可继续。
  await page.waitForFunction(
    () => document.querySelector('[data-testid="analysis-scan"]')?.getAttribute("data-scan-status") === "complete",
    null,
    { timeout: 240_000 },
  );
  record.analysis_scan_complete = true;
  record.evidence_screenshot = await screenshot(page, "01b-broken-analysis-complete.png");
  record.status = "passed";

  // 重载页面回到应用列表，为下一个场景提供干净的初始状态。
  await page.reload();
  await page.getByTestId("program-search").waitFor({ state: "visible", timeout: 60_000 });
}

// 场景 2：卸载流程的报错不得残留在应用列表窗口。
//
// 复现方式：通过 Vite dev server 动态 import 应用自己的 store 模块（同一模块实例），
// 直接注入 error 状态——这等价于任何卸载失败路径留下的残留状态；随后驱动 UI
// 进入卸载确认页并点击“取消”退出（exitUninstallFlow），断言列表恢复干净。
// 全程不点击 workflow-start，不触发任何真实卸载命令。
async function verifyUninstallErrorIsClearedOnExit(page) {
  const record = evidence.bugfix_error_residue;

  const response = await listPrograms(page);
  const programs = response.programs ?? [];
  const target =
    programs.find((program) => /7-?zip/i.test(program.name)) ??
    programs.find(
      (program) =>
        program.uninstall_kind === "legacy" &&
        Boolean(program.uninstall_string?.trim() || program.quiet_uninstall_string?.trim()),
    );
  assert.ok(target, "清单中必须存在可用于退出流程场景的程序");
  record.program = { id: target.id, name: target.name };

  // 进入卸载确认页（不点击 workflow-start，不会触发真实卸载）。
  await selectProgramByName(page, target.name);
  const uninstallButton = page.getByTestId("program-uninstall");
  await uninstallButton.waitFor({ state: "visible" });
  assert.equal(await uninstallButton.isDisabled(), false, "正常程序的卸载按钮必须可用");
  await uninstallButton.click();
  const workflow = page.getByTestId("uninstall-workflow");
  await workflow.waitFor({ state: "visible" });
  assert.equal(await workflow.getAttribute("data-workflow-stage"), "confirm");

  // 模拟卸载失败：在工作流激活期间注入 store 错误状态，等价于真实失败路径
  // （plan/execute 失败时 set({ error })）留下的状态。
  const injected = await page.evaluate(async () => {
    const module = await import("/src/stores/programs.ts");
    const store = module.useProgramsStore;
    if (!store?.setState) return false;
    store.setState({
      error: "模拟：卸载计划失败（E2E 注入的残留错误）",
    });
    return true;
  });
  assert.ok(injected, "无法注入 store 错误状态");
  record.stub_verified = true;

  // 点击“取消”退出流程——这正是修复的 exitUninstallFlow 清理路径。
  const cancelButton = page.getByRole("button", { name: /取消|Cancel/ }).first();
  await cancelButton.click();
  await workflow.waitFor({ state: "detached" });

  record.list_error_residue_count = await page.locator(".table-message.error").count();
  assert.equal(
    record.list_error_residue_count,
    0,
    "返回列表后不得残留任何错误占位信息",
  );
  await page.getByTestId("program-search").fill("");
  record.list_row_count = await page.locator(".rdg-row").count();
  assert.ok(record.list_row_count > 0, "返回列表后必须能看到程序列表行");
  record.evidence_screenshot = await screenshot(page, "03-list-after-exit.png");
  record.status = "passed";
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

async function writeEvidence() {
  await writeFile(
    path.join(artifactRoot, "evidence.json"),
    JSON.stringify(evidence, null, 2),
    "utf8",
  );
}

// 场景 3：强制卸载支持按部分名称搜索候选。选中候选后自动生成计划；
// 计划阶段只扫描和收集指纹，不删除任何内容（不点击清理按钮）。
async function verifyForceUninstallNameSearch(page) {
  const record = evidence.force_name_search;

  await page.getByTestId("nav-apps").click();
  await page.getByRole("button", { name: /强制卸载|Force uninstall/ }).first().click();
  const nameSearch = page.getByTestId("force-name-search");
  await nameSearch.waitFor({ state: "visible" });

  // 部分名称（小写子串）必须能命中候选列表。
  const response = await listPrograms(page);
  const programs = response.programs ?? [];
  const sample = programs.find((program) => program.install_location);
  assert.ok(sample, "清单中必须存在有安装目录的程序用于构造搜索");
  const fragment = sample.name.slice(0, Math.max(3, Math.floor(sample.name.length / 2)));
  record.query = fragment;
  await nameSearch.fill(fragment);
  const candidateButton = page.locator('[data-testid="force-candidate"]').first();
  try {
    await candidateButton.waitFor({ state: "visible", timeout: 30_000 });
  } catch (error) {
    // 诊断：直接调用后端命令并读取 store 状态，区分后端失败与前端接线问题。
    const diagnostics = await page.evaluate(async (probe) => {
      let backend;
      try {
        const result = await window.__TAURI_INTERNALS__.invoke("search_force_uninstall_candidates", {
          request: { query: probe },
        });
        backend = { ok: true, count: result.length };
      } catch (invokeError) {
        backend = { ok: false, error: String(invokeError) };
      }
      let store;
      try {
        const module = await import("/src/stores/forceUninstall.ts");
        const state = module.useForceUninstallStore.getState();
        store = { candidates: state.candidates.length, error: state.error, loading: state.candidatesLoading };
      } catch (storeError) {
        store = { error: String(storeError) };
      }
      return { backend, store };
    }, fragment);
    await screenshot(page, "debug-force-search-failure.png");
    throw new Error(`候选搜索失败，诊断：${JSON.stringify(diagnostics)}`);
  }
  record.candidate_count = await page.locator('[data-testid="force-candidate"]').count();

  // 选中有安装目录的候选后应自动生成卸载计划（只读扫描，不做清理）。
  const targetCandidate = page.locator(`[data-testid="force-candidate"][data-program-name="${sample.name}"]`);
  if ((await targetCandidate.count()) > 0) {
    await targetCandidate.first().click();
  } else {
    await candidateButton.click();
  }
  await page.locator(".force-plan-body").waitFor({ state: "visible", timeout: 60_000 });
  record.plan_generated = true;
  record.evidence_screenshot = await screenshot(page, "04-force-plan-from-name.png");
  record.status = "passed";
}
