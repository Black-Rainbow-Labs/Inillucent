#!/usr/bin/env node
// Runs SQL cases through the pinned sqlite3 shell and inillucent-shell and reports every case whose
// output differs, every case where inillucent-shell crashed or hung, and every case where
// inillucent-shell took much longer than sqlite3.
//
// A case file has the same form as compat/corpus/usage/*.sql: a case starts at a line
// `-- case: <name>` and runs to the next one. Text after a `|` on the marker line is ignored, so a
// research note can carry its source URL there. Each case runs in a fresh directory against a
// fresh database file, with `.mode quote` and `.headers on`, exactly as differential::usage_corpus
// runs the corpus, so a case that passes here passes there.
//
// Usage:
//   node tools/bug-hunt/probe.mjs [--ours <inillucent-shell>] [--jobs N] [--out <dir>]
//                                 [--slow-ratio R] [--filter <text>] <cases.sql>...
//
// The report goes to <out>/report.json and a readable summary to standard output. The exit code is
// 0 when every case matched, 1 when any case differed, crashed, hung or was slow, and 2 when the
// run could not start.

import { spawn } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { cpus, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const PREAMBLE = '.mode quote\n.headers on\n';
const CASE_LIMIT_MS = 60_000;

/**
 * Reads the command line into an options object.
 * @param argv - the arguments after the script name
 */
function parseArguments(argv) {
  const options = { files: [], jobs: Math.max(1, Math.floor(cpus().length / 2)), slowRatio: 10, filter: '' };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const next = () => argv[++index];
    if (argument === '--ours') options.ours = next();
    else if (argument === '--reference') options.reference = next();
    else if (argument === '--jobs') options.jobs = Number(next());
    else if (argument === '--out') options.out = next();
    else if (argument === '--slow-ratio') options.slowRatio = Number(next());
    else if (argument === '--filter') options.filter = next();
    else options.files.push(argument);
  }
  options.reference ??= join(ROOT, '.sqlite-ref', '3.53.4', 'shell', process.platform === 'win32' ? 'sqlite3.exe' : 'sqlite3');
  options.out ??= join(ROOT, '_agent_output', 'bug-hunt');
  return options;
}

/**
 * Splits a case file into named cases, the way differential::usage_corpus does.
 * @param file - the file name, recorded on each case
 * @param text - the file's contents
 */
function parseCases(file, text) {
  const cases = [];
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith('-- case:')) {
      const name = line.slice('-- case:'.length).split('|')[0].trim();
      cases.push({ name, file, script: '' });
      continue;
    }
    if (cases.length > 0) cases[cases.length - 1].script += `${line}\n`;
  }
  return cases;
}

/**
 * Removes the two lines a shell prints under an error to point at the failing offset, the same
 * normalisation the corpus suite applies.
 * @param text - a shell's whole output
 */
function withoutCarets(text) {
  const lines = text.split('\n');
  const kept = [];
  for (let index = 0; index < lines.length; index += 1) {
    kept.push(lines[index]);
    const pointer = lines[index + 2] ?? '';
    if (lines[index].includes('rror near line ') && (pointer.endsWith('^--- error here') || pointer.trimStart().startsWith('error here ---^'))) {
      index += 2;
    }
  }
  return kept.join('\n');
}

/**
 * Runs one shell over a fresh database and resolves with its output, exit status and duration.
 * @param program - the shell to run
 * @param directory - an empty directory for the case's files
 * @param script - the case's SQL, without the preamble
 */
function runShell(program, directory, script) {
  return new Promise((resolvePromise) => {
    rmSync(directory, { recursive: true, force: true });
    mkdirSync(directory, { recursive: true });
    const started = process.hrtime.bigint();
    const child = spawn(program, [join(directory, 'case.db')], { cwd: directory });
    const chunks = { out: [], err: [] };
    child.stdout.on('data', (chunk) => chunks.out.push(chunk));
    child.stderr.on('data', (chunk) => chunks.err.push(chunk));
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; child.kill(); }, CASE_LIMIT_MS);
    child.on('close', (code, signal) => {
      clearTimeout(timer);
      const ms = Number(process.hrtime.bigint() - started) / 1e6;
      const text = Buffer.concat(chunks.out).toString('utf8') + Buffer.concat(chunks.err).toString('utf8');
      resolvePromise({ text: withoutCarets(text.replace(/\r\n/g, '\n')), code, signal, ms, timedOut });
    });
    child.stdin.on('error', () => {});
    child.stdin.end(PREAMBLE + script);
  });
}

/**
 * Says whether a shell's result looks like a crash rather than an ordinary error.
 * @param result - what runShell resolved with
 */
function crashed(result) {
  if (result.timedOut) return 'TIMEOUT';
  if (/panicked at|stack overflow|thread '.*' has overflowed/i.test(result.text)) return 'PANIC';
  // 0xC00000FD is a stack overflow on Windows, 0xC0000005 an access violation, 101 a Rust panic.
  if (result.code === 3221225725 || result.code === -1073741571) return 'STACK OVERFLOW';
  if (result.code === 3221225477 || result.code === -1073741819) return 'ACCESS VIOLATION';
  if (result.code === 101) return 'PANIC';
  if (result.signal) return `SIGNAL ${result.signal}`;
  return null;
}

/**
 * Runs one case through both shells and classifies the outcome.
 * @param testCase - the parsed case
 * @param index - its position, used for a short unique directory name
 * @param options - the run options
 */
async function runCase(testCase, index, options) {
  const base = join(tmpdir(), 'inillucent-bug-hunt', `${process.pid}-${index}`);
  const reference = await runShell(options.reference, join(base, 'r'), testCase.script);
  const ours = await runShell(options.ours, join(base, 'o'), testCase.script);
  rmSync(base, { recursive: true, force: true });
  const crash = crashed(ours);
  const differs = reference.text !== ours.text;
  const slow = ours.ms > 1000 && ours.ms > reference.ms * options.slowRatio;
  return { ...testCase, crash, differs, slow, sqlite: reference.text, inillucent: ours.text, sqliteMs: reference.ms, oursMs: ours.ms };
}

/**
 * Runs every case with a fixed number of cases in flight.
 * @param cases - the parsed cases
 * @param options - the run options
 */
async function runAll(cases, options) {
  const results = new Array(cases.length);
  let next = 0;
  const worker = async () => {
    while (next < cases.length) {
      const index = next++;
      results[index] = await runCase(cases[index], index, options);
    }
  };
  await Promise.all(Array.from({ length: options.jobs }, worker));
  return results;
}

/**
 * Prints the first differing line of each failing case and writes the full report.
 * @param results - every case's outcome
 * @param options - the run options
 */
function report(results, options) {
  const failing = results.filter((r) => r.crash || r.differs || r.slow);
  for (const r of failing) {
    const tags = [r.crash, r.differs && 'DIFF', r.slow && `SLOW ${r.oursMs.toFixed(0)}ms vs ${r.sqliteMs.toFixed(0)}ms`].filter(Boolean).join(' ');
    console.log(`\n=== ${r.file} :: ${r.name} [${tags}]`);
    if (r.differs || r.crash) {
      const a = r.sqlite.split('\n');
      const b = r.inillucent.split('\n');
      const at = a.findIndex((line, i) => line !== b[i]);
      const from = Math.max(0, at === -1 ? Math.min(a.length, b.length) : at);
      console.log(`  sqlite    : ${a.slice(from, from + 3).join(' / ').slice(0, 400)}`);
      console.log(`  inillucent: ${b.slice(from, from + 3).join(' / ').slice(0, 400)}`);
    }
  }
  mkdirSync(options.out, { recursive: true });
  writeFileSync(join(options.out, 'report.json'), JSON.stringify(failing, null, 2));
  console.log(`\n${results.length} cases, ${failing.length} failing (${results.filter((r) => r.crash).length} crashed or hung, ${results.filter((r) => r.differs).length} differ, ${results.filter((r) => r.slow).length} slow). Report: ${join(options.out, 'report.json')}`);
  return failing.length;
}

/**
 * Loads the cases, runs them and sets the exit code.
 */
async function main() {
  const options = parseArguments(process.argv.slice(2));
  options.ours ??= process.env.INILLUCENT_SHELL;
  if (!options.ours || !existsSync(options.ours) || !existsSync(options.reference) || options.files.length === 0) {
    console.error('usage: probe.mjs --ours <inillucent-shell> [--reference <sqlite3>] <cases.sql>...');
    process.exit(2);
  }
  let cases = options.files.flatMap((file) => parseCases(file, readFileSync(file, 'utf8')));
  if (options.filter) cases = cases.filter((c) => c.name.includes(options.filter));
  const failing = report(await runAll(cases, options), options);
  process.exit(failing === 0 ? 0 : 1);
}

main();
