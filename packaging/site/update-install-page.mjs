/**
 * Rewrites the release on the site's install page, `public/install.md`.
 *
 * **The page an agent follows named a release three behind.** `update-downloads.mjs` rewrote the
 * home page's download list on every release, and nothing rewrote this page, so in October 2026
 * inillucent.com/install.md still said "You are installing Inillucent 1.0.29", told the reader that
 * `inillucent --version` "must print `inillucent 1.0.29`", and listed archives and digests that
 * SHA256SUMS no longer named. Anyone following it, person or agent, was told a correct 2.0.3
 * install had failed.
 *
 * So this runs beside `update-downloads.mjs` in `publish-site.ps1 -Link`, and `Test-SiteVersion`
 * in `ship.ps1` fetches the live page and refuses a release whose install page names another
 * version. Every version number on the page is the one it was last written with, so each one is
 * replaced by the new version, and the archive list is written again from the artifacts that were
 * staged. A page that no longer has the line naming its version, or the archive section, is an
 * error rather than a page left stale.
 *
 * Usage: node update-install-page.mjs <install.md> '<json array of entries>' <version>
 * Each entry is { platform, detail, href, sha256 }, the shape update-downloads.mjs takes.
 */
import { readFileSync, writeFileSync } from 'node:fs';

/** The heading the archive list sits under. */
const ARCHIVES_HEADING = '## The archives, and their checksums';

/** The paragraph that follows the archive list. */
const AFTER_ARCHIVES = 'Both install scripts check the archive';

/**
 * Finds the version the page was last written for.
 * @param source - the page
 */
function writtenVersion(source) {
  const found = source.match(/You are installing \*\*Inillucent (\d+\.\d+\.\d+)\*\*/);
  if (!found) throw new Error('install.md has no "You are installing **Inillucent <version>**" line');
  return found[1];
}

/**
 * Renders one archive as the page's list item.
 * @param entry - platform, detail, href and sha256 for one artifact
 */
function renderArchive(entry) {
  const file = entry.href.split('/').pop();
  const digest = entry.sha256 ? `, SHA-256 \`${entry.sha256}\`` : '';
  return `- **${entry.platform}** — \`${file}\` (${entry.detail})${digest}`;
}

/**
 * Replaces the archive list between its heading and the paragraph after it.
 * @param source - the page
 * @param entries - the staged artifacts
 */
function replaceArchives(source, entries) {
  const start = source.indexOf(ARCHIVES_HEADING);
  if (start === -1) throw new Error(`install.md has no "${ARCHIVES_HEADING}" section`);
  const end = source.indexOf(AFTER_ARCHIVES, start);
  if (end === -1) throw new Error(`install.md has no "${AFTER_ARCHIVES}" paragraph after the archive list`);
  const list = entries.map(renderArchive).join('\n');
  return `${source.slice(0, start)}${ARCHIVES_HEADING}\n\n${list}\n\n${source.slice(end)}`;
}

/**
 * Rewrites the page for one release.
 * @param source - the page as it is
 * @param entries - the staged artifacts
 * @param version - the release
 */
export function rewrite(source, entries, version) {
  const previous = writtenVersion(source);
  const escaped = previous.replace(/\./g, '\\.');
  // Not preceded by a digit or a dot, and not followed by a digit or a dot and a digit, so
  // `inillucent-1.0.29.pkg` is a match and `1.0.290` is not.
  const replaced = source.replace(new RegExp(`(?<![\\d.])${escaped}(?!\\.?\\d)`, 'g'), version);
  return replaceArchives(replaced, entries);
}

/**
 * Runs from the command line, as publish-site.ps1 calls it.
 */
function main() {
  const [path, json, version] = process.argv.slice(2);
  if (!path || !json || !version) {
    throw new Error("usage: node update-install-page.mjs <install.md> '<json entries>' <version>");
  }
  const parsed = JSON.parse(json);
  const entries = Array.isArray(parsed) ? parsed : [parsed];
  const source = readFileSync(path, 'utf8').replace(/\r\n/g, '\n');
  const written = rewrite(source, entries, version);
  const stale = written.match(/inillucent[-_ ](\d+\.\d+\.\d+)/gi)?.filter((name) => !name.includes(version)) ?? [];
  if (stale.length > 0) throw new Error(`install.md still names another release: ${stale.join(', ')}`);
  writeFileSync(path, written);
  console.log(`install.md now describes ${version}, with ${entries.length} archive(s)`);
}

if (process.argv[1] && import.meta.url.endsWith(process.argv[1].replace(/\\/g, '/').split('/').pop())) {
  main();
}
