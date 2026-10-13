import { homedir } from "node:os";
import { spawnSync } from "node:child_process";

if (process.env.RUSTFLAGS && !process.env.CARGO_ENCODED_RUSTFLAGS) {
  throw new Error("Use CARGO_ENCODED_RUSTFLAGS for custom package compiler flags.");
}
const roots = [...new Set([homedir(), homedir().replaceAll("\\", "/")])];
const flags = [process.env.CARGO_ENCODED_RUSTFLAGS, ...roots.map(root => `--remap-path-prefix=${root}=/build-home`)].filter(Boolean).join("\x1f");
const result = spawnSync("node", ["node_modules/@tauri-apps/cli/tauri.js", "build", "--features", "custom-protocol"], {
  stdio: "inherit", env: { ...process.env, CARGO_ENCODED_RUSTFLAGS: flags },
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
