import { describe, expect, it } from "vitest";
import { formatWindowsDate } from "./date";

describe("formatWindowsDate", () => {
  it("formats registry date-only values with the local short-date pattern", () => {
    const expected = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "2-digit", day: "2-digit" }).format(
      new Date(2024, 0, 15),
    );

    expect(formatWindowsDate("20240115")).toBe(expected);
    expect(formatWindowsDate("2024/01/15")).toBe(expected);
  });

  it("converts timestamps to the local calendar date", () => {
    const timestamp = "2024-01-15T12:30:00.000Z";
    const expected = new Intl.DateTimeFormat(undefined, { year: "numeric", month: "2-digit", day: "2-digit" }).format(
      new Date(timestamp),
    );

    expect(formatWindowsDate(timestamp)).toBe(expected);
  });

  it("rejects empty and invalid dates", () => {
    expect(formatWindowsDate(null)).toBeNull();
    expect(formatWindowsDate("not-a-date")).toBeNull();
    expect(formatWindowsDate("2024-02-31")).toBeNull();
  });
});
