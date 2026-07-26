#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const binary = path.join(path.dirname(fileURLToPath(import.meta.url)), "glass-native" + (process.platform === "win32" ? ".exe" : ""));
if (!existsSync(binary)) {
  console.error("Glass native binary is not installed; rerun npm install or set GLASS_SKIP_DOWNLOAD only when using a local binary.");
  process.exit(1);
}
const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
process.exit(result.status ?? 1);
