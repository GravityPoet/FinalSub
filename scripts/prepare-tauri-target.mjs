import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

if (process.platform === "darwin") {
  const targetDirectory = resolve("src-tauri", "target");
  await mkdir(targetDirectory, { recursive: true });
  await writeFile(resolve(targetDirectory, ".metadata_never_index"), "", { flag: "a" });
}
