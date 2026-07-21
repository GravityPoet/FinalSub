import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  REQUIRED_RELEASE_SECRET_NAMES,
  validateReleasePreflight,
} from "./release-preflight.mjs";

async function createRepositoryFixture({
  packageVersion = "1.2.3",
  tauriVersion = packageVersion,
  cargoVersion = packageVersion,
} = {}) {
  const repositoryRoot = await mkdtemp(join(tmpdir(), "finalsub-release-"));
  await mkdir(join(repositoryRoot, "src-tauri"));
  await Promise.all([
    writeFile(
      join(repositoryRoot, "package.json"),
      `${JSON.stringify({ version: packageVersion })}\n`,
    ),
    writeFile(
      join(repositoryRoot, "src-tauri", "tauri.conf.json"),
      `${JSON.stringify({ version: tauriVersion })}\n`,
    ),
    writeFile(
      join(repositoryRoot, "src-tauri", "Cargo.toml"),
      `[package]\nname = "finalsub"\nversion = "${cargoVersion}"\n`,
    ),
  ]);
  return repositoryRoot;
}

function completeEnvironment() {
  const environment = Object.fromEntries(
    REQUIRED_RELEASE_SECRET_NAMES.map((name) => [
      name,
      name === "WINDOWS_TIMESTAMP_URL"
        ? "https://timestamp.example.com"
        : `fixture-${name}`,
    ]),
  );
  environment.FINALSUB_UPDATER_PUBLIC_KEY = `untrusted comment: minisign public key\n${"A".repeat(56)}`;
  environment.APPLE_TEAM_ID = "FIXTURE123";
  return environment;
}

test("accepts an exact tag, matching versions, and complete secret names", async (t) => {
  const repositoryRoot = await createRepositoryFixture();
  t.after(() => rm(repositoryRoot, { recursive: true, force: true }));

  const result = await validateReleasePreflight({
    repositoryRoot,
    tagName: "v1.2.3",
    environment: completeEnvironment(),
  });

  assert.deepEqual(result, {
    version: "1.2.3",
    requiredSecretCount: REQUIRED_RELEASE_SECRET_NAMES.length,
  });
});

test("rejects a tag that does not exactly match the package version", async (t) => {
  const repositoryRoot = await createRepositoryFixture();
  t.after(() => rm(repositoryRoot, { recursive: true, force: true }));

  await assert.rejects(
    validateReleasePreflight({
      repositoryRoot,
      tagName: "v1.2.4",
      environment: completeEnvironment(),
    }),
    /Release tag must be exactly v1\.2\.3/,
  );
});

test("rejects mismatched Tauri and Cargo versions before creating a draft", async (t) => {
  const repositoryRoot = await createRepositoryFixture({
    tauriVersion: "1.2.2",
    cargoVersion: "1.2.1",
  });
  t.after(() => rm(repositoryRoot, { recursive: true, force: true }));

  await assert.rejects(
    validateReleasePreflight({
      repositoryRoot,
      tagName: "v1.2.3",
      environment: completeEnvironment(),
    }),
    /tauri=1\.2\.2, cargo=1\.2\.1/,
  );
});

test("reports missing secret names without including configured values", async (t) => {
  const repositoryRoot = await createRepositoryFixture();
  t.after(() => rm(repositoryRoot, { recursive: true, force: true }));
  const environment = completeEnvironment();
  environment.APPLE_PASSWORD = "";
  environment.WINDOWS_CERTIFICATE = "";

  await assert.rejects(
    validateReleasePreflight({
      repositoryRoot,
      tagName: "v1.2.3",
      environment,
    }),
    (error) => {
      assert.match(error.message, /APPLE_PASSWORD/);
      assert.match(error.message, /WINDOWS_CERTIFICATE/);
      assert.doesNotMatch(error.message, /fixture-APPLE_ID/);
      return true;
    },
  );
});

test("rejects a timestamp URL that could disclose embedded credentials", async (t) => {
  const repositoryRoot = await createRepositoryFixture();
  t.after(() => rm(repositoryRoot, { recursive: true, force: true }));
  const environment = completeEnvironment();
  environment.WINDOWS_TIMESTAMP_URL = [
    "https://",
    "username",
    ":",
    "password",
    "@timestamp.example.com",
  ].join("");

  await assert.rejects(
    validateReleasePreflight({
      repositoryRoot,
      tagName: "v1.2.3",
      environment,
    }),
    /must not contain credentials/,
  );
});

test("rejects a private updater key in the public-key secret", async (t) => {
  const repositoryRoot = await createRepositoryFixture();
  t.after(() => rm(repositoryRoot, { recursive: true, force: true }));
  const environment = completeEnvironment();
  environment.FINALSUB_UPDATER_PUBLIC_KEY =
    `untrusted comment: minisign secret key\n${"A".repeat(56)}`;

  await assert.rejects(
    validateReleasePreflight({
      repositoryRoot,
      tagName: "v1.2.3",
      environment,
    }),
    /must not contain a private key/,
  );
});

test("rejects an invalid Apple team identifier", async (t) => {
  const repositoryRoot = await createRepositoryFixture();
  t.after(() => rm(repositoryRoot, { recursive: true, force: true }));
  const environment = completeEnvironment();
  environment.APPLE_TEAM_ID = "too-short";

  await assert.rejects(
    validateReleasePreflight({
      repositoryRoot,
      tagName: "v1.2.3",
      environment,
    }),
    /APPLE_TEAM_ID must be 10 uppercase letters or digits/,
  );
});
