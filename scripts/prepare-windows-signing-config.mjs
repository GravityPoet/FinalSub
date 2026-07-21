import { mkdir, rename, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { windowsSigningConfigFromEnvironment } from "./windows-signing-config.mjs";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(scriptDirectory, "..");
const outputPath = resolve(
  repositoryRoot,
  "src-tauri",
  "target",
  "tauri.windows-signing.generated.conf.json",
);
const generated = {
  bundle: {
    windows: windowsSigningConfigFromEnvironment({ required: true }),
  },
};

await mkdir(dirname(outputPath), { recursive: true });
const temporaryPath = `${outputPath}.${process.pid}.tmp`;
await writeFile(temporaryPath, `${JSON.stringify(generated, null, 2)}\n`, {
  mode: 0o600,
});
await rename(temporaryPath, outputPath);
console.log(`Prepared Windows signing config: ${outputPath}`);
