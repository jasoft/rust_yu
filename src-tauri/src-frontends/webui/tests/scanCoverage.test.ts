import { describe, expect, it } from "vitest";
import { activeCoverageCount } from "../src/lib/scanCoverage";

describe("deep analysis scan coverage", () => {
  it("counts every area that was actually queried", () => {
    const coverage = [
      { area: "registry", status: "partial", item_count: 24 },
      { area: "install_files", status: "complete", item_count: 5090 },
      { area: "user_data", status: "complete", item_count: 2704 },
      { area: "shortcuts", status: "skipped", item_count: 0 },
      { area: "services", status: "failed", item_count: 0 },
    ] as const;

    expect(activeCoverageCount(coverage)).toBe(4);
  });
});
