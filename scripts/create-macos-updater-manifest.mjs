import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const ASSET_PATH = /^\/repos\/GravityPoet\/FinalSub\/releases\/assets\/\d+$/;

export function createMacosUpdaterManifest({
  version,
  assetUrl,
  signature,
  notes = "",
  pubDate = new Date().toISOString(),
}) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error("Updater version must be valid SemVer");
  }
  const url = new URL(assetUrl);
  if (
    url.protocol !== "https:" ||
    url.hostname !== "api.github.com" ||
    !ASSET_PATH.test(url.pathname) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  ) {
    throw new Error("Updater asset URL must be an official FinalSub GitHub asset API URL");
  }
  const normalizedSignature = signature.trim();
  if (!/^[A-Za-z0-9+/]{80,}={0,2}$/.test(normalizedSignature)) {
    throw new Error("Updater signature is missing or invalid");
  }
  if (Number.isNaN(Date.parse(pubDate))) {
    throw new Error("Updater publication date must be an ISO date");
  }

  const platform = { url: url.href, signature: normalizedSignature };
  return {
    version,
    notes,
    pub_date: pubDate,
    platforms: {
      "darwin-aarch64-app": platform,
      "darwin-aarch64": platform,
      "darwin-x86_64-app": platform,
      "darwin-x86_64": platform,
    },
  };
}

async function main() {
  const [version, assetUrl, signaturePath, notesPath, outputPath] = process.argv.slice(2);
  if (!version || !assetUrl || !signaturePath || !notesPath || !outputPath) {
    throw new Error(
      "Usage: node scripts/create-macos-updater-manifest.mjs <version> <asset-url> <signature-file> <notes-file> <output-file>",
    );
  }
  const [signature, notes] = await Promise.all([
    readFile(resolve(signaturePath), "utf8"),
    readFile(resolve(notesPath), "utf8"),
  ]);
  const manifest = createMacosUpdaterManifest({
    version,
    assetUrl,
    signature,
    notes,
  });
  const absoluteOutput = resolve(outputPath);
  await mkdir(dirname(absoluteOutput), { recursive: true });
  const temporaryOutput = `${absoluteOutput}.${process.pid}.tmp`;
  await writeFile(temporaryOutput, `${JSON.stringify(manifest, null, 2)}\n`, {
    mode: 0o644,
  });
  await rename(temporaryOutput, absoluteOutput);
  console.log(`Prepared updater manifest: ${absoluteOutput}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    await main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
