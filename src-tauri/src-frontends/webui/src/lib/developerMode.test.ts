import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getDeveloperModeEnabled, setDeveloperModeEnabled } from "./developerMode";

describe("developer mode preference", () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
  };

  beforeEach(() => {
    values.clear();
    vi.stubGlobal("localStorage", storage);
  });
  afterEach(() => vi.unstubAllGlobals());

  it("is disabled by default", () => {
    expect(getDeveloperModeEnabled()).toBe(false);
  });

  it("persists enabled state and removes it when disabled", () => {
    setDeveloperModeEnabled(true);
    expect(getDeveloperModeEnabled()).toBe(true);

    setDeveloperModeEnabled(false);
    expect(getDeveloperModeEnabled()).toBe(false);
  });
});
