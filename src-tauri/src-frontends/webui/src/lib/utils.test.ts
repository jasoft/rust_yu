import { describe, expect, it } from "vitest";
import { formatBytes, formatSource } from "./utils";

describe("localized formatting", () => {
  it("renders zero bytes explicitly instead of leaving the value blank", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(null)).toBe("");
  });

  it("formats byte units and source identifiers through the shared locale", () => {
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatSource("msi")).toBe("MSI");
  });
});
