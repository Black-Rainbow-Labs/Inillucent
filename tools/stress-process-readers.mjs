#!/usr/bin/env node
// Runs the `process_readers` durability suite many times at once under load, and counts what it saw.
//
// Bug 23: on 2026-09-29 one run of `inillucent-compat::durability::process_readers`, inside a merge
// cadence run with 24 test binaries at once, lost one acknowledged row and a reader saw a gap. In
// isolation and under lighter load it never failed. This harness makes that load on purpose: it
// starts several copies of the suite side by side, each in its own scratch directory
// (`READERS_DIR`), and keeps a number of busy loops running beside them, so the writer, the sixteen
// or more readers and their resynchronisation after every commit (`Pool::discard_all`) all compete
// for the processors. Each copy is one round; the harness reports every round's result and the
// totals, and keeps the output of every failed round.
//
// Usage, from the repository root, inside the processor share the rest of the work uses:
//
//   cargo test -p inillucent-compat --test durability --no-run      (prints the binary's path)
//   cargo build -p inillucent-cli --bin inillucent-shell
//   pwsh tools/capped.ps1 node tools/stress-process-readers.mjs --binary <durability.exe> \
//       --rounds 200 --copies 6 --burn 8 --out _agent_output/stress-readers
//
// The suite finds `inillucent-shell` the way every suite does (`cliproc::program`), so build it into
// the same target directory first. Exit code 0 means every round passed, 1 that at least one failed,
// and 2 that the harness could not run.

import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { Worker } from "node:worker_threads";

/** Reads `--name value` options into an object of defaults. */
function options(argv) {
  const found = { binary: "", rounds: 100, copies: 6, burn: 6, out: "_agent_output/stress-readers", filter: "process_readers::readers_see_only_prefixes" };
  for (let at = 0; at < argv.length; at += 2) {
    const name = argv[at].replace(/^--/, "");
    if (!(name in found)) {
      console.error(`unknown option --${name}`);
      process.exit(2);
    }
    found[name] = typeof found[name] === "number" ? Number(argv[at + 1]) : argv[at + 1];
  }
  if (!found.binary) {
    console.error("--binary <path to the durability test binary> is required");
    process.exit(2);
  }
  return found;
}

/** Starts one busy loop on a thread of its own, and returns it so it can be stopped. */
function burner() {
  return new Worker("for (;;) { Math.sqrt(Math.random()); }", { eval: true });
}

/** Runs one copy of the suite in its own directory and resolves with its exit code and output. */
function round(settings, number) {
  return new Promise((done) => {
    const directory = resolve(settings.out, `round-${number}`);
    mkdirSync(directory, { recursive: true });
    const child = spawn(settings.binary, [settings.filter, "--test-threads", "2"], {
      // The programs are built before the harness starts, so no copy starts a build of its own.
      env: { ...process.env, READERS_DIR: directory, INILLUCENT_PROGRAMS_BUILT: "1" },
    });
    let said = "";
    child.stdout.on("data", (chunk) => (said += chunk));
    child.stderr.on("data", (chunk) => (said += chunk));
    child.on("error", (error) => done({ number, code: -1, said: String(error) }));
    child.on("close", (code) => done({ number, code, said }));
  });
}

/** Runs every round, `copies` at a time, and returns the results in the order they ended. */
async function all(settings) {
  const results = [];
  let next = 1;
  async function lane() {
    while (next <= settings.rounds) {
      const number = next;
      next += 1;
      const result = await round(settings, number);
      results.push(result);
      const passed = result.code === 0 && /test result: ok\. [1-9]/.test(result.said);
      console.log(`round ${number}: ${passed ? "passed" : `FAILED (exit ${result.code})`}`);
      if (!passed) {
        writeFileSync(join(settings.out, `failed-round-${number}.txt`), result.said);
      }
    }
  }
  await Promise.all(Array.from({ length: settings.copies }, lane));
  return results;
}

const settings = options(process.argv.slice(2));
mkdirSync(settings.out, { recursive: true });
const loops = Array.from({ length: settings.burn }, burner);
const started = Date.now();
const results = await all(settings);
await Promise.all(loops.map((loop) => loop.terminate()));
const passed = results.filter((result) => result.code === 0 && /test result: ok\. [1-9]/.test(result.said));
const summary = {
  rounds: results.length,
  passed: passed.length,
  failed: results.length - passed.length,
  copies: settings.copies,
  burn: settings.burn,
  seconds: Math.round((Date.now() - started) / 1000),
};
writeFileSync(join(settings.out, "summary.json"), JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary));
process.exit(summary.failed === 0 ? 0 : 1);
