import { describe, expect, it } from "vitest";
import { filterToolboxItems } from "../lib/toolbox";

describe("toolbox search", () => {
  it("returns every safe tool for an empty query", () => {
    expect(filterToolboxItems(" ")).toHaveLength(8);
  });

  it("matches titles, details, and keywords without case sensitivity", () => {
    expect(filterToolboxItems("RUNONCE").map((item) => item.id)).toEqual(["startup"]);
    expect(filterToolboxItems("快照").map((item) => item.id)).toEqual(["monitor"]);
    expect(filterToolboxItems("迁移对比").map((item) => item.id)).toEqual(["inventory"]);
    expect(filterToolboxItems("不存在")).toEqual([]);
  });
});
