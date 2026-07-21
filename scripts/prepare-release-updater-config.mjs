import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { windowsSigningConfigFromEnvironment } from "./windows-signing-config.mjs";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "..");
const templatePath = resolve(repositoryRoot, "src-tauri", "tauri.release.conf.json");
const outputPath = resolve(
  repositoryRoot,
  "src-tauri",
  "target",
  "tauri.release.generated.conf.json",
);
const manifestUrl =
  "https://github.com/GravityPoet/FinalSub/releases/latest/download/latest.json";

const publicKey = process.env.FINALSUB_UPDATER_PUBLIC_KEY?.trim() ?? "";
if (publicKey.length < 40 || publicKey.length > 4096) {
  throw new Error("FINALSUB_UPDATER_PUBLIC_KEY is missing or has an invalid length");
}
if (/secret key|private key/i.test(publicKey)) {
  throw new Error("FINALSUB_UPDATER_PUBLIC_KEY must not contain a private key");
}
const hasEncodedKeyLine = publicKey
  .split(/\r?\n/)
  .some((line) => /^[A-Za-z0-9+/]{40,}={0,2}$/.test(line.trim()));
if (!hasEncodedKeyLine) {
  throw new Error("FINALSUB_UPDATER_PUBLIC_KEY is not a valid minisign public key");
}

const template = JSON.parse(await readFile(templatePath, "utf8"));
const requireWindowsSigning = ["1", "true"].includes(
  (process.env.FINALSUB_REQUIRE_WINDOWS_SIGNING ?? "").trim().toLowerCase(),
);
const windowsSigning = windowsSigningConfigFromEnvironment({
  required: requireWindowsSigning,
});
const generated = {
  ...template,
  bundle: {
    ...(template.bundle ?? {}),
    ...(windowsSigning
      ? {
          windows: {
            ...(template.bundle?.windows ?? {}),
            ...windowsSigning,
          },
        }
      : {}),
  },
  plugins: {
    ...(template.plugins ?? {}),
    updater: {
      endpoints: [manifestUrl],
      pubkey: publicKey,
    },
  },
};

await mkdir(dirname(outputPath), { recursive: true });
const temporaryPath = `${outputPath}.${process.pid}.tmp`;
await writeFile(temporaryPath, `${JSON.stringify(generated, null, 2)}\n`, {
  mode: 0o600,
});
await rename(temporaryPath, outputPath);
console.log(`Prepared updater release config: ${outputPath}`);
