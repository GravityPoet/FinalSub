import assert from "node:assert/strict";
import test from "node:test";

import { createMacosUpdaterManifest } from "./create-macos-updater-manifest.mjs";

const signature = "A".repeat(100);
const assetUrl =
  "https://api.github.com/repos/GravityPoet/FinalSub/releases/assets/123456";

test("creates Universal macOS updater entries for both CPU architectures", () => {
  const manifest = createMacosUpdaterManifest({
    version: "1.2.3",
    assetUrl,
    signature,
    notes: "Update notes",
    pubDate: "2026-08-24T00:00:00.000Z",
  });

  assert.equal(manifest.version, "1.2.3");
  assert.deepEqual(Object.keys(manifest.platforms), [
    "darwin-aarch64-app",
    "darwin-aarch64",
    "darwin-x86_64-app",
    "darwin-x86_64",
  ]);
  for (const platform of Object.values(manifest.platforms)) {
    assert.deepEqual(platform, { url: assetUrl, signature });
  }
});

test("rejects updater downloads outside the official FinalSub asset API", () => {
  assert.throws(
    () =>
      createMacosUpdaterManifest({
        version: "1.2.3",
        assetUrl: "https://example.com/FinalSub.app.tar.gz",
        signature,
      }),
    /official FinalSub GitHub asset API URL/,
  );
});

test("rejects missing updater signatures", () => {
  assert.throws(
    () =>
      createMacosUpdaterManifest({
        version: "1.2.3",
        assetUrl,
        signature: "",
      }),
    /signature is missing or invalid/,
  );
});
