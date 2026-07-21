import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const REQUIRED_RELEASE_SECRET_NAMES = [
  "FINALSUB_UPDATER_PUBLIC_KEY",
  "TAURI_SIGNING_PRIVATE_KEY",
  "APPLE_CERTIFICATE",
  "APPLE_CERTIFICATE_PASSWORD",
  "APPLE_SIGNING_IDENTITY",
  "APPLE_ID",
  "APPLE_PASSWORD",
  "APPLE_TEAM_ID",
  "WINDOWS_CERTIFICATE",
  "WINDOWS_CERTIFICATE_PASSWORD",
  "WINDOWS_TIMESTAMP_URL",
];

function parseCargoPackageVersion(contents) {
  let inPackageSection = false;
  for (const rawLine of contents.split(/\r?\n/)) {
    const line = rawLine.replace(/\s+#.*$/, "").trim();
    const section = line.match(/^\[([^\]]+)]$/);
    if (section) {
      inPackageSection = section[1] === "package";
      continue;
    }
    if (!inPackageSection) {
      continue;
    }
    const version = line.match(/^version\s*=\s*"([^"]+)"\s*$/);
    if (version) {
      return version[1];
    }
  }
  throw new Error("src-tauri/Cargo.toml is missing [package].version");
}

function validateTimestampUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("WINDOWS_TIMESTAMP_URL must be an absolute URL");
  }
  if (!["http:", "https:"].includes(url.protocol)) {
    throw new Error("WINDOWS_TIMESTAMP_URL must use HTTP or HTTPS");
  }
  if (url.username || url.password || url.hash) {
    throw new Error(
      "WINDOWS_TIMESTAMP_URL must not contain credentials or a fragment",
    );
  }
}

function validateUpdaterPublicKey(value) {
  if (value.length < 40 || value.length > 4096) {
    throw new Error(
      "FINALSUB_UPDATER_PUBLIC_KEY is missing or has an invalid length",
    );
  }
  if (/secret key|private key/i.test(value)) {
    throw new Error("FINALSUB_UPDATER_PUBLIC_KEY must not contain a private key");
  }
  const hasEncodedKeyLine = value
    .split(/\r?\n/)
    .some((line) => /^[A-Za-z0-9+/]{40,}={0,2}$/.test(line.trim()));
  if (!hasEncodedKeyLine) {
    throw new Error(
      "FINALSUB_UPDATER_PUBLIC_KEY is not a valid minisign public key",
    );
  }
}

export async function validateReleasePreflight({
  repositoryRoot,
  tagName,
  environment = process.env,
}) {
  const [packageJson, tauriConfig, cargoManifest] = await Promise.all([
    readFile(resolve(repositoryRoot, "package.json"), "utf8").then(JSON.parse),
    readFile(
      resolve(repositoryRoot, "src-tauri", "tauri.conf.json"),
      "utf8",
    ).then(JSON.parse),
    readFile(resolve(repositoryRoot, "src-tauri", "Cargo.toml"), "utf8"),
  ]);

  const versions = {
    package: String(packageJson.version ?? "").trim(),
    tauri: String(tauriConfig.version ?? "").trim(),
    cargo: parseCargoPackageVersion(cargoManifest).trim(),
  };
  if (!versions.package) {
    throw new Error("package.json is missing version");
  }
  const mismatchedVersions = Object.entries(versions)
    .filter(([, version]) => version !== versions.package)
    .map(([source, version]) => `${source}=${version || "<missing>"}`);
  if (mismatchedVersions.length > 0) {
    throw new Error(
      `Release versions do not match package=${versions.package}: ${mismatchedVersions.join(", ")}`,
    );
  }

  const expectedTag = `v${versions.package}`;
  if (tagName !== expectedTag) {
    throw new Error(`Release tag must be exactly ${expectedTag}`);
  }

  const missing = REQUIRED_RELEASE_SECRET_NAMES.filter(
    (name) => !(environment[name] ?? "").trim(),
  );
  if (missing.length > 0) {
    throw new Error(`Missing required release secrets: ${missing.join(", ")}`);
  }
  validateUpdaterPublicKey(environment.FINALSUB_UPDATER_PUBLIC_KEY.trim());
  validateTimestampUrl(environment.WINDOWS_TIMESTAMP_URL.trim());
  if (!/^[A-Z0-9]{10}$/.test(environment.APPLE_TEAM_ID.trim())) {
    throw new Error("APPLE_TEAM_ID must be 10 uppercase letters or digits");
  }

  return {
    version: versions.package,
    requiredSecretCount: REQUIRED_RELEASE_SECRET_NAMES.length,
  };
}

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "..");
const invokedPath = process.argv[1]
  ? pathToFileURL(resolve(process.argv[1])).href
  : "";

if (import.meta.url === invokedPath) {
  const tagName = process.argv[2] ?? process.env.GITHUB_REF_NAME ?? "";
  try {
    const result = await validateReleasePreflight({
      repositoryRoot,
      tagName,
    });
    console.log(
      `Release preflight passed for v${result.version}; ${result.requiredSecretCount} required secret names are set.`,
    );
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
