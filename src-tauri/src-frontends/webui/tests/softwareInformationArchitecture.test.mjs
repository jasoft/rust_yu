import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import test from "node:test";

const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const startup = readFileSync(new URL("../src/components/StartupManager.tsx", import.meta.url), "utf8");

test("removes the ambiguous evidence-and-policy destination", () => {
  assert.equal(existsSync(new URL("../src/components/EvidenceCenter.tsx", import.meta.url)), false);
  assert.doesNotMatch(app, /activeNav === "evidence"/);
  assert.doesNotMatch(app, /<EvidenceCenter/);
});

test("places each software task in a user-oriented destination", () => {
  assert.match(app, /<SoftwareRecordDialog/);
  assert.match(app, /activeNav === "inventory"/);
  assert.match(app, /<SoftwareInventoryComparison/);
  assert.match(app, /<CleanupSafetyRules/);
  assert.match(startup, /<SoftwareHibernation/);
  assert.match(startup, /components\.startupmanager\.views\.software/);
});
