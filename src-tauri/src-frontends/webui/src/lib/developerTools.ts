import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type DeveloperFixtureKind = "msi" | "inno" | "nsis" | "script";
export type FixtureInstallStatus = "installed" | "reboot_required" | "failed";

export interface DeveloperFixture {
  id: string;
  kind: DeveloperFixtureKind;
  path: string;
  available: boolean;
  size: number | null;
  /** 该夹具当前是否已安装在系统中（按 ARP 卸载键判断）。 */
  installed: boolean;
}

export interface FixtureInstallResult {
  fixture_id: string;
  status: FixtureInstallStatus;
  exit_code: number | null;
  error_code: string | null;
}

export interface InstallDeveloperFixturesResponse {
  results: FixtureInstallResult[];
}

export interface DeveloperFixtureInstallProgress {
  fixture_id: string;
  index: number;
  total: number;
  phase: "installing" | "completed";
  status: FixtureInstallStatus | null;
}

export async function listDeveloperFixtures(): Promise<DeveloperFixture[]> {
  return invoke<DeveloperFixture[]>("list_developer_fixtures");
}

export async function installDeveloperFixtures(fixtureIds: string[]): Promise<InstallDeveloperFixturesResponse> {
  return invoke<InstallDeveloperFixturesResponse>("install_developer_fixtures", {
    request: { fixture_ids: fixtureIds, confirm: true },
  });
}

export function listenToFixtureInstallProgress(
  onProgress: (progress: DeveloperFixtureInstallProgress) => void,
): Promise<UnlistenFn> {
  return listen<DeveloperFixtureInstallProgress>("developer-fixture-install-progress", (event) => {
    onProgress(event.payload);
  });
}

export function getDeveloperCommandErrorCode(error: unknown): string | null {
  if (typeof error === "object" && error !== null && "code" in error) {
    const code = (error as { code?: unknown }).code;
    return typeof code === "string" ? code : null;
  }
  return null;
}
