import assert from "node:assert/strict";
import test from "node:test";

import {
  countProgramsBySource,
  filterPrograms,
  hasMissingProgramIcons,
  hasUsableUninstallCommand,
  matchesProgramSource,
} from "../src/lib/programFilters.ts";

function program(name, install_source, publisher = null) {
  return {
    id: `${install_source}-${name}`,
    name,
    publisher,
    install_source,
    icon_path: `C:\\Apps\\${name}.exe`,
    icon_cache_path_32: `C:\\cache\\32\\${name}.png`,
    icon_cache_path_large: `C:\\cache\\128\\${name}.png`,
  };
}

const programs = [
  program("Registry App", "registry", "Acme"),
  program("MSI Tool", "msi", "Acme"),
  program("Store App", "store", "Microsoft"),
];

test("来源标签在同一份全量列表上筛选，保留图标字段", () => {
  const filtered = filterPrograms(programs, "msi", "tool");
  assert.equal(filtered.length, 1);
  assert.equal(filtered[0], programs[1]);
  assert.equal(filtered[0].icon_cache_path_32, programs[1].icon_cache_path_32);
});

test("为每个来源计算独立数量", () => {
  assert.deepEqual(countProgramsBySource(programs), {
    all: 3,
    registry: 1,
    msi: 1,
    store: 1,
  });
});

test("组合搜索和来源筛选时仍严格匹配来源", () => {
  assert.equal(matchesProgramSource("registry", "msi"), false);
  assert.equal(matchesProgramSource("registry", "registry"), true);
  assert.equal(matchesProgramSource("registry", "all"), true);
  assert.deepEqual(filterPrograms(programs, "msi", "Acme").map((item) => item.name), ["MSI Tool"]);
});

test("仅在存在图标来源且缓存不完整时请求预热", () => {
  assert.equal(hasMissingProgramIcons(programs), false);
  assert.equal(
    hasMissingProgramIcons([{ ...programs[0], icon_cache_path_large: null }]),
    true,
  );
  assert.equal(
    hasMissingProgramIcons([
      { ...programs[0], icon_path: null, icon_cache_path_32: null, icon_cache_path_large: null },
    ]),
    false,
  );
});

test("失效安装缺少卸载命令时不能进入标准卸载流程", () => {
  const legacy = { is_broken_installation: false, uninstall_kind: "legacy", uninstall_string: null, quiet_uninstall_string: null };
  assert.equal(hasUsableUninstallCommand(legacy), false);
  // 空白命令等同于缺失，不能放行到“运行卸载器”阶段才失败。
  assert.equal(
    hasUsableUninstallCommand({ is_broken_installation: false, uninstall_kind: "legacy", uninstall_string: "  ", quiet_uninstall_string: null }),
    false,
  );
  assert.equal(
    hasUsableUninstallCommand({ is_broken_installation: false, uninstall_kind: "legacy", uninstall_string: '"C:\\Apps\\unins.exe" /S', quiet_uninstall_string: null }),
    true,
  );
  assert.equal(
    hasUsableUninstallCommand({ is_broken_installation: false, uninstall_kind: "legacy", uninstall_string: null, quiet_uninstall_string: '"C:\\Apps\\unins.exe" /S' }),
    true,
  );
});

test("分析器判定安装记录失效时，即使卸载命令还在也必须拦截", () => {
  assert.equal(
    hasUsableUninstallCommand({ is_broken_installation: true, uninstall_kind: "legacy", uninstall_string: '"C:\\Apps\\missing\\unins.exe"', quiet_uninstall_string: null }),
    false,
  );
  assert.equal(
    hasUsableUninstallCommand({ is_broken_installation: true, uninstall_kind: "msi", uninstall_string: "MsiExec.exe /X{...}", quiet_uninstall_string: null }),
    false,
  );
});

test("MSI 与 Store 路由由后端构造命令，始终允许标准卸载", () => {
  for (const uninstall_kind of ["msi", "store"]) {
    assert.equal(
      hasUsableUninstallCommand({ is_broken_installation: false, uninstall_kind, uninstall_string: null, quiet_uninstall_string: null }),
      true,
    );
  }
});
