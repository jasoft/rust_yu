const STORAGE_KEY = "rust-yu.developer-mode";

function getStorage(): Storage | null {
  try {
    return typeof globalThis.localStorage === "undefined" ? null : globalThis.localStorage;
  } catch {
    return null;
  }
}

export function getDeveloperModeEnabled(): boolean {
  return getStorage()?.getItem(STORAGE_KEY) === "enabled";
}

export function setDeveloperModeEnabled(enabled: boolean): void {
  const storage = getStorage();
  if (!storage) return;
  if (enabled) storage.setItem(STORAGE_KEY, "enabled");
  else storage.removeItem(STORAGE_KEY);
}
