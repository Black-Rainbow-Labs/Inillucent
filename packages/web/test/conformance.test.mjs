// The shared conformance suite, run through the WebAssembly build.
//
// `drivers/conformance/suite.json` is what "the binding is correct" means in
// this repository. This runner loads `dist/inillucent.wasm` with the package's
// own WASI host and memory file system, holds one connection for a case, and
// grades every step. A case that asks for `per_call` is satisfied by that: the
// suite says a runner whose connection already continues one session does
// nothing for it, and a C ABI connection is a session, as in the Python runner. It runs under Node, which is the same WebAssembly engine Chromium uses
// and the same JavaScript the package runs in a browser; the browser test
// beside it covers what only a browser has.
//
// The suite writes integers past 2^53 as bare JSON numbers. `JSON.parse` would
// round them, so the suite is read with the reviver's source text and those
// integers become `BigInt`s, which bind exactly.
//
// It writes `_agent_output/conformance/web.json`, the record every runner
// leaves, and needs the module built: `node packages/web/build.mjs`.

import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import { Engine } from '../src/index.mjs';

/** This package's folder. */
const here = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The repository root. */
const root = resolve(here, '..', '..');

/** Which runner this is, as the suite's `skipped_by` names it. */
const LANGUAGE = 'web';

/** The module, which `build.mjs` puts here. */
const wasm = join(here, 'dist', 'inillucent.wasm');

/**
 * Reads the suite, keeping every integer exact.
 *
 * @param text - the suite's JSON
 */
function readSuite(text) {
  return JSON.parse(text, (_key, value, context) => {
    if (typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value) && /^-?\d+$/.test(context?.source ?? '')) {
      return BigInt(context.source);
    }
    return value;
  });
}

/** The suite, read once. */
const suite = readSuite(readFileSync(join(root, 'drivers', 'conformance', 'suite.json'), 'utf8'));

/** What this runner cannot do, out of the suite's own record of it. */
const lacks = new Set(suite.skipped_by?.[LANGUAGE]?.lacks ?? []);

/**
 * Reads a value out of the suite's one-key object form, as the JavaScript value it binds as.
 *
 * @param described - the one-key object
 */
function valueOf(described) {
  if ('null' in described) return null;
  if ('int' in described) return typeof described.int === 'bigint' ? described.int : BigInt(described.int);
  if ('real' in described) return described.real === 'NaN' ? NaN : Number(described.real);
  if ('text' in described) return described.text;
  if ('blob' in described) return Uint8Array.from(described.blob);
  throw new Error(`${JSON.stringify(described)} names no value kind`);
}

/**
 * Renders a value for a failure message.
 *
 * @param value - any value a cell holds
 */
function show(value) {
  if (typeof value === 'bigint') return `${value}n`;
  if (value instanceof Uint8Array) return `bytes[${[...value].join(',')}]`;
  if (typeof value === 'number' && Object.is(value, -0)) return '-0';
  return JSON.stringify(value) ?? String(value);
}

/**
 * Compares an expected value to what the engine answered, kind and all.
 *
 * An integer must come back as an integer (a number or a bigint with the same
 * value), a real as a number, text as a string and a blob as bytes. A real is
 * compared with `Object.is`, so -0 and NaN are told apart as the suite asks.
 *
 * @param wantDescribed - the suite's one-key object
 * @param got - what came back
 */
function same(wantDescribed, got) {
  if ('null' in wantDescribed) return got === null;
  if ('int' in wantDescribed) {
    if (typeof got !== 'bigint' && !(typeof got === 'number' && Number.isInteger(got))) return false;
    return BigInt(got) === BigInt(wantDescribed.int);
  }
  if ('real' in wantDescribed) return typeof got === 'number' && Object.is(got, valueOf(wantDescribed));
  if ('text' in wantDescribed) return got === wantDescribed.text;
  if ('blob' in wantDescribed) {
    return got instanceof Uint8Array && got.length === wantDescribed.blob.length && got.every((byte, at) => byte === wantDescribed.blob[at]);
  }
  return false;
}

/**
 * Runs one statement, answering either its result or the error it raised.
 *
 * @param engine - the engine
 * @param conn - the connection
 * @param sql - the statement
 * @param params - the values to bind
 * @param limit - the row limit, when the step names one
 */
function attempt(engine, conn, sql, params, limit) {
  try {
    return { ok: true, result: engine.execute(conn, sql, params, limit) };
  } catch (error) {
    if (error.statusName === undefined) throw error;
    return { ok: false, error };
  }
}

/**
 * Grades one step's result and returns what did not hold.
 *
 * @param name - the case's name
 * @param step - the step out of the suite
 * @param outcome - what `attempt` answered
 */
function gradeStep(name, step, outcome) {
  const wrong = [];
  const at = `${name}: \`${step.sql}\``;
  if (step.status !== undefined) {
    if (outcome.ok) return [`${at} succeeded and should have failed with \`${step.status}\``];
    if (outcome.error.statusName !== step.status) wrong.push(`${at} failed with \`${outcome.error.statusName}\` (${outcome.error.message}) and should have failed with \`${step.status}\``);
    if (step.message_contains && !outcome.error.message.includes(step.message_contains)) wrong.push(`${at} said \`${outcome.error.message}\`, which does not contain \`${step.message_contains}\``);
    if (step.feature_contains && !(outcome.error.feature ?? '').includes(step.feature_contains)) wrong.push(`${at} named the feature \`${outcome.error.feature}\`, which does not contain \`${step.feature_contains}\``);
    return wrong;
  }
  if (!outcome.ok) return [`${at} failed with \`${outcome.error.statusName}\`: ${outcome.error.message}`];
  const result = outcome.result;
  if (step.columns !== undefined && JSON.stringify(result.columns) !== JSON.stringify(step.columns)) {
    wrong.push(`${at} answered columns ${JSON.stringify(result.columns)}, wanted ${JSON.stringify(step.columns)}`);
  }
  if (step.rows !== undefined) {
    if (result.rows.length !== step.rows.length) wrong.push(`${at} answered ${result.rows.length} rows, wanted ${step.rows.length}`);
    else {
      step.rows.forEach((wantRow, row) => {
        if (result.rows[row].length !== wantRow.length) wrong.push(`${at}: row ${row} has ${result.rows[row].length} cells, wanted ${wantRow.length}`);
        wantRow.forEach((wantCell, column) => {
          const got = result.rows[row][column];
          if (!same(wantCell, got)) wrong.push(`${at}: row ${row} column ${column} is ${show(got)} and should be ${JSON.stringify(wantCell, (_k, v) => (typeof v === 'bigint' ? `${v}` : v))}`);
        });
      });
    }
  }
  if (step.types !== undefined && JSON.stringify(result.types) !== JSON.stringify(step.types)) {
    wrong.push(`${at} answered declared types ${JSON.stringify(result.types)}, wanted ${JSON.stringify(step.types)}`);
  }
  if (step.more !== undefined && result.more !== step.more) wrong.push(`${at} reported more=${result.more}, wanted ${step.more}`);
  if (step.total !== undefined && result.total !== Number(step.total)) wrong.push(`${at} reported total ${result.total}, wanted ${step.total}`);
  if (step.affected !== undefined && result.affected !== (step.affected === null ? null : Number(step.affected))) {
    wrong.push(`${at} reported ${result.affected} changed, wanted ${step.affected}`);
  }
  return wrong;
}

/**
 * Grades one case on its own database and returns what did not hold.
 *
 * @param engine - the engine
 * @param onecase - the case out of the suite
 * @param path - the case's database path
 */
function gradeCase(engine, onecase, path) {
  const db = engine.open(path);
  const held = engine.connect(db);
  const wrong = [];
  try {
    for (const statement of onecase.setup ?? []) {
      const outcome = attempt(engine, held, statement, [], undefined);
      if (!outcome.ok) return [`${onecase.name}: the setup \`${statement}\` failed: ${outcome.error.message}`];
    }
    for (const step of onecase.steps ?? []) {
      const params = (step.params ?? []).map(valueOf);
      const outcome = attempt(engine, held, step.sql, params, step.limit);
      wrong.push(...gradeStep(onecase.name, step, outcome));
    }
  } finally {
    engine.connectionFree(held);
    engine.close(db);
  }
  return wrong;
}

test('the shared conformance suite through the WebAssembly build', { skip: existsSync(wasm) ? false : 'no dist/inillucent.wasm (run node packages/web/build.mjs); skipping' }, async () => {
  const engine = await Engine.load({ wasm: readFileSync(wasm) });
  const ran = [];
  const skipped = [];
  const failures = [];
  for (const [index, onecase] of suite.cases.entries()) {
    const missing = (onecase.needs ?? []).filter((one) => lacks.has(one));
    if (missing.length > 0) {
      skipped.push({ name: onecase.name, group: onecase.group, needs: missing });
      continue;
    }
    failures.push(...gradeCase(engine, onecase, `/case-${index}.rdb`));
    ran.push(onecase.name);
  }
  const record = join(root, '_agent_output', 'conformance');
  mkdirSync(record, { recursive: true });
  writeFileSync(join(record, `${LANGUAGE}.json`), `${JSON.stringify({ language: LANGUAGE, lacks: [...lacks], ran, skipped, failures }, null, 2)}\n`);
  assert.equal(ran.length, suite.cases.length, `only ${ran.length} of ${suite.cases.length} cases ran`);
  assert.deepEqual(failures, [], `${failures.length} assertions did not hold through the WebAssembly build:\n  ${failures.join('\n  ')}`);
});
