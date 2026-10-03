#!/usr/bin/env node
// Scores a performance hill climb step from the samples files `inillucent-fullgate --samples` writes.
//
// usage:
//   node tools/perf-hillclimb/score.mjs <samples>...                 score one candidate
//   node tools/perf-hillclimb/score.mjs --base <samples>... --cand <samples>...
//                                                                    compare a candidate with a base
//
// Each samples file holds lines `round <tab> elapsed <tab> arm <tab> workload <tab> nanos`, one per
// arm per workload per round. A workload's score in one round is log(SQLite's time / ours), so SQLite
// is the yardstick that moves with the machine, and two builds measured an hour apart can be compared.
// A workload is in the test set when FNV-1a of its name is 0 mod 3, the rule in
// `crates/inillucent-compat/src/perf/hillclimb.rs`. Everything else is train.
//
// The acceptance rule of the TDD (tasks/task-2175-performance-hillclimb-tdd.md, section 4):
// a candidate is kept when the train geomean and the test geomean both rise, the lower end of the
// test delta's 95% interval is above -1%, and no single workload loses more than 5% with its whole
// interval below zero.

import { existsSync, readFileSync } from 'node:fs';

/**
 * Reads the child process figures the gate prints beside a samples file, from `<name>.txt`.
 * @param path - the samples file; its report is the same name ending in `.txt`
 */
function readChild(path) {
  const report = path.replace(/\.samples$/, '.txt');
  if (!existsSync(report)) return null;
  const text = readFileSync(report, 'utf8');
  const peak = /peak resident set\s+([\d.]+) MiB\s+([\d.]+) MiB/.exec(text);
  const cpu = /processor time\s+([\d.]+) ms\s+([\d.]+) ms/.exec(text);
  if (!peak || !cpu) return null;
  return { peak: Number(peak[1]), theirPeak: Number(peak[2]), cpu: Number(cpu[1]), theirCpu: Number(cpu[2]) };
}

/**
 * Returns whether a workload is held out, by the same FNV-1a rule as the Rust plan.
 * @param name - the workload's name
 */
function isTest(name) {
  let hash = 0xcbf29ce484222325n;
  const mask = (1n << 64n) - 1n;
  for (const byte of Buffer.from(name, 'utf8')) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & mask;
  }
  return hash % 3n === 0n;
}

/**
 * Reads samples files into per workload, per round log ratios and per round costs.
 * @param paths - the samples files; rounds of several files are pooled
 */
function readSamples(paths) {
  const workloads = new Map();
  const costs = { ours: [], theirs: [] };
  paths.forEach((path, fileIndex) => {
    const rounds = new Map();
    for (const line of readFileSync(path, 'utf8').split(/\r?\n/)) {
      const fields = line.split('\t');
      if (fields.length < 5) continue;
      const [round, , arm, workload, value] = fields;
      if (workload === '(faults)') continue;
      if (workload === '(cost)') {
        const m = /user (\d+) kernel (\d+) faults (\d+) peak (\d+)/.exec(value);
        if (m) costs[arm].push({ cpu: (Number(m[1]) + Number(m[2])) / 1e6, peak: Number(m[4]) / 1048576 });
        continue;
      }
      const key = `${fileIndex}:${round}`;
      if (!rounds.has(key)) rounds.set(key, new Map());
      const entry = rounds.get(key);
      if (!entry.has(workload)) entry.set(workload, {});
      entry.get(workload)[arm] = Number(value);
    }
    for (const entry of rounds.values()) {
      for (const [workload, arms] of entry) {
        if (!(arms.ours > 0 && arms.theirs > 0)) continue;
        if (!workloads.has(workload)) workloads.set(workload, []);
        workloads.get(workload).push(Math.log(arms.theirs / arms.ours));
      }
    }
  });
  const child = paths.map(readChild).filter(Boolean);
  return { workloads, costs, child };
}

/**
 * Returns the median of a list of numbers.
 * @param values - the numbers
 */
function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

/**
 * Returns a seeded random number generator, so an interval is reproducible.
 * @param seed - the starting state
 */
function rng(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * Returns the 95% bootstrap interval of a statistic over resampled lists.
 * @param lists - one list of samples per workload
 * @param statistic - maps one resampled list per workload to a number
 */
function bootstrap(lists, statistic) {
  const random = rng(0x5eed1833);
  const results = [];
  for (let i = 0; i < 2000; i++) {
    const resampled = lists.map((list) => list.map(() => list[Math.floor(random() * list.length)]));
    results.push(statistic(resampled));
  }
  results.sort((a, b) => a - b);
  return [results[Math.floor(0.025 * results.length)], results[Math.floor(0.975 * results.length)]];
}

/**
 * Returns the geometric mean ratio of a set of workloads, from their median log ratios.
 * @param lists - one list of log ratios per workload
 */
function geomean(lists) {
  if (lists.length === 0) return NaN;
  return Math.exp(lists.reduce((sum, list) => sum + median(list), 0) / lists.length);
}

/**
 * Formats a ratio as a fixed width figure.
 * @param value - the ratio
 */
function fmt(value) {
  return Number.isFinite(value) ? value.toFixed(3).padStart(8) : '     n/a';
}

/**
 * Prints one run's per workload ratios and its train and test scores.
 * @param run - what readSamples returned
 * @param title - the heading
 */
function scoreOne(run, title) {
  console.log(`## ${title}`);
  const names = [...run.workloads.keys()].sort();
  for (const name of names) {
    const list = run.workloads.get(name);
    console.log(`  ${isTest(name) ? 'test ' : 'train'} ${name.padEnd(30)} ${fmt(Math.exp(median(list)))}x  n=${list.length}`);
  }
  for (const set of ['train', 'test']) {
    const lists = names.filter((n) => (set === 'test') === isTest(n)).map((n) => run.workloads.get(n));
    const [low, high] = bootstrap(lists, geomean);
    console.log(`  ${set} geomean ${fmt(geomean(lists))}x  95% ${fmt(low)} to ${fmt(high)}  (${lists.length} workloads)`);
  }
  for (const c of run.child) {
    console.log(`  child: peak ${c.peak.toFixed(2)} MiB against ${c.theirPeak.toFixed(2)} (${(c.peak / c.theirPeak).toFixed(2)}x), cpu ${c.cpu.toFixed(0)} ms against ${c.theirCpu.toFixed(0)} (${(c.cpu / c.theirCpu).toFixed(2)}x)`);
  }
}

/**
 * Prints the candidate against the base and the verdict of the acceptance rule.
 * @param base - the base run
 * @param cand - the candidate run
 */
function compare(base, cand) {
  const names = [...base.workloads.keys()].filter((n) => cand.workloads.has(n)).sort();
  const deltas = new Map();
  let worst = null;
  console.log('## candidate against base (ratio of ratios; above 1 is faster)');
  for (const name of names) {
    const b = base.workloads.get(name);
    const c = cand.workloads.get(name);
    const delta = Math.exp(median(c) - median(b));
    const [low, high] = bootstrap([b, c], ([rb, rc]) => Math.exp(median(rc) - median(rb)));
    deltas.set(name, { delta, low, high });
    const flag = delta < 0.95 && high < 1 ? '  REGRESSION' : delta > 1.05 && low > 1 ? '  better' : '';
    if (flag.includes('REGRESSION') && (!worst || delta < worst.delta)) worst = { name, delta };
    console.log(`  ${isTest(name) ? 'test ' : 'train'} ${name.padEnd(30)} ${fmt(delta)}  95% ${fmt(low)} to ${fmt(high)}${flag}`);
  }
  const verdicts = {};
  for (const set of ['train', 'test']) {
    const chosen = names.filter((n) => (set === 'test') === isTest(n));
    const bl = chosen.map((n) => base.workloads.get(n));
    const cl = chosen.map((n) => cand.workloads.get(n));
    const centre = geomean(cl) / geomean(bl);
    const [low, high] = bootstrap([...bl, ...cl], (all) => geomean(all.slice(bl.length)) / geomean(all.slice(0, bl.length)));
    verdicts[set] = { centre, low, high };
    console.log(`  ${set} geomean change ${fmt(centre)}  95% ${fmt(low)} to ${fmt(high)}`);
  }
  for (const key of ['cpu', 'peak']) {
    const b = base.child.map((x) => x[key]);
    const c = cand.child.map((x) => x[key]);
    if (b.length && c.length) console.log(`  child ${key}: base ${median(b).toFixed(2)}, candidate ${median(c).toFixed(2)} (${((median(c) / median(b) - 1) * 100).toFixed(1)}%)`);
  }
  const keep = verdicts.train.centre > 1 && verdicts.test.centre > 1 && verdicts.test.low > 0.99 && !worst;
  console.log(`  verdict: ${keep ? 'KEEP' : 'REJECT'}${worst ? ` (regression in ${worst.name}, ${worst.delta.toFixed(3)})` : ''}`);
}

/**
 * Reads the command line and runs the score or the comparison.
 * @param argv - the arguments after the script name
 */
function main(argv) {
  const at = (flag) => argv.indexOf(flag);
  if (at('--base') >= 0 && at('--cand') >= 0) {
    const base = argv.slice(at('--base') + 1, at('--cand'));
    const cand = argv.slice(at('--cand') + 1);
    const b = readSamples(base);
    const c = readSamples(cand);
    scoreOne(b, 'base');
    scoreOne(c, 'candidate');
    compare(b, c);
    return;
  }
  scoreOne(readSamples(argv), 'run');
}

main(process.argv.slice(2));
