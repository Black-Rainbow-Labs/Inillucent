// Builds the engine's WebAssembly module and puts it where the package loads it from.
//
//     node packages/web/build.mjs            # release build, the one that ships
//     node packages/web/build.mjs --debug    # a quicker build for working on the host
//
// It builds `inillucent-driver-capi` for `wasm32-wasip1` and copies the
// module to `packages/web/dist/inillucent.wasm`, with the repository's LICENSE
// beside the package. The target is installed by
// `rustup target add wasm32-wasip1`.
//
// The Rust stack is 1 MiB, named here so it is a decision and not a default.
// A WebAssembly module has two stacks: the Rust stack, in its own memory,
// which the engine measures, refusing a statement that nests past it with
// `too_big`; and the JavaScript engine's own call stack, which holds the
// WebAssembly frames and which the module cannot measure. Measured under Node
// 24: with an 8 MiB Rust stack, 1,000 nested subqueries overflowed V8's stack
// first and stopped the instance; with 1 MiB, 500 and 1,000 were refused with
// `too_big` and the instance kept working. 1 MiB is also a Windows main
// thread's stack. A statement that still overflows V8 first (thousands of
// nested parentheses) stops the instance, and `inillucent-web` replaces it.

import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** This package's folder. */
const here = dirname(fileURLToPath(import.meta.url));

/** The repository root. */
const root = resolve(here, '..', '..');

/** The Rust target the module is built for. */
export const TARGET = 'wasm32-wasip1';

/** The stack the module links with, in bytes. */
export const STACK_BYTES = 1024 * 1024;

/**
 * Returns the folder cargo builds into, which a worktree's own cargo configuration may move.
 */
function targetDirectory() {
  const text = execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  return JSON.parse(text).target_directory;
}

/**
 * Builds the module and copies it into `dist/`, returning where it went.
 *
 * @param profile - `release` or `debug`
 */
export function build(profile = 'release') {
  const args = ['build', '-p', 'inillucent-driver-capi', '--lib', '--target', TARGET];
  if (profile === 'release') args.push('--release');
  execFileSync('cargo', args, {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS: `-C link-arg=-zstack-size=${STACK_BYTES}` },
  });
  const built = join(targetDirectory(), TARGET, profile, 'inillucent_driver_capi.wasm');
  const output = join(here, 'dist', 'inillucent.wasm');
  mkdirSync(dirname(output), { recursive: true });
  copyFileSync(built, output);
  // The package ships the repository's licence, copied here rather than kept twice.
  copyFileSync(join(root, 'LICENSE'), join(here, 'LICENSE'));
  return { output, bytes: statSync(output).size };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { output, bytes } = build(process.argv.includes('--debug') ? 'debug' : 'release');
  console.log(`${output}: ${(bytes / 1024 / 1024).toFixed(2)} MiB`);
}
