import assert from "node:assert/strict";
import test from "node:test";

import {
  countProgramsBySource,
  filterPrograms,
  hasMissingProgramIcons,
} from "../src/lib/programFilters.ts";

function program(name, install_source, publisher = null) {
  return {
    id: `${install_source}-${name}`,
    name,
    publisher,
    install_source,
    icon_path: `C:\\Apps\\${name}.exe`,
    icon_cache_path_32: `C:\\cache\\32\\${name}.png`,
    icon_cache_path_48: `C:\\cache\\48\\${name}.png`,
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

test("仅在存在图标来源且缓存不完整时请求预热", () => {
  assert.equal(hasMissingProgramIcons(programs), false);
  assert.equal(
    hasMissingProgramIcons([{ ...programs[0], icon_cache_path_48: null }]),
    true,
  );
  assert.equal(
    hasMissingProgramIcons([
      { ...programs[0], icon_path: null, icon_cache_path_32: null, icon_cache_path_48: null },
    ]),
    false,
  );
});
