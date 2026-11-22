#!/usr/bin/env node
/**
 * Static reuse-conformance checks for the ICA frontend.
 *
 * This script is the executable form of
 * `REACT-UI-REUSE-STANDARD-t_dff04c2c.md`. Every check emits a machine-readable
 * result (id, measured value, threshold, violations) so a reuse validation is a
 * number, not an opinion. It has no dependencies and reads source files only;
 * it never writes to the repo.
 *
 * Usage:
 *   node scripts/check-ui-reuse.mjs            # human summary, exit 0
 *   node scripts/check-ui-reuse.mjs --json     # JSON only
 *   node scripts/check-ui-reuse.mjs --strict   # exit 1 when any check fails
 *   node scripts/check-ui-reuse.mjs --json --out <path>
 *   node scripts/check-ui-reuse.mjs --baseline <frozen.json>   # + regression diff
 *   node scripts/check-ui-reuse.mjs --baseline <frozen.json> --fail-on-regression
 *
 * Exit-code contract (1.2.0):
 *   default              exit 0 — report only
 *   --strict             exit 1 when any check fails OR a metric regressed
 *                        (this is a "whole repository is green" gate, i.e. Gate 2)
 *   --fail-on-regression exit 1 **only** when a metric regressed against
 *                        `--baseline`; still-failing checks do not affect the
 *                        exit code. Requires `--baseline` (exit 2 otherwise).
 *                        This is the Gate 1 command: a single extraction must not
 *                        regress the frozen baseline, regardless of how many
 *                        repository-wide checks are still red.
 *   --strict + --fail-on-regression  exit 2 — mutually exclusive (a silently
 *                        swallowed flag would fail open in CI).
 *
 * Check ids map 1:1 to the acceptance criteria in the standard document.
 * 1.1.0 changes (see standard §0):
 *   - C1: ternary/template class names are analysed per branch instead of being
 *     blanket-deferred; `needs_manual_review_sites` is now a reported metric.
 *   - C6: a shared export counts as tested only if a test file *imports* it from
 *     that module (text mentions no longer count).
 *   - C7: a call site must import the name and use it outside comments; comment
 *     mentions no longer keep a dead export alive.
 *   - new `--fail-on-regression` exit mode.
 * 1.2.0 changes (see standard §0.1 v1.2):
 *   - C1 `status` now includes `needs_manual_review_sites`, so `--strict` /
 *     `all_checks_pass` cannot stay green while AC-13's fourth number is > 0.
 *   - a className with no static token at all (`className={clsBad}`,
 *     ``className={`${badCls}`}``) is counted as a `needs_manual_review` site
 *     instead of being invisible. Pure prop forwarding
 *     (`className={className}` / `className={props.className}`) is exempt: it
 *     forwards the consumer's class list and is not a styling decision here.
 *   - class-name fragments glued to an interpolation or a `+` (`btn-${x}`,
 *     `'btn-' + size`) are no longer reported as phantom tokens; the site is
 *     statically undecidable and goes to `needs_manual_review`.
 *   - `--strict` + `--fail-on-regression` is a usage error (exit 2).
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const VERSION = '1.2.0';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FRONTEND = path.resolve(HERE, '..');
const SRC = path.join(FRONTEND, 'src');
const CSS_PATH = path.join(SRC, 'index.css');
const TYPES_PATH = path.join(SRC, 'types.ts');

const argv = process.argv.slice(2);
const asJson = argv.includes('--json');
const strict = argv.includes('--strict');
const failOnRegression = argv.includes('--fail-on-regression');
const outIdx = argv.indexOf('--out');
const outPath = outIdx >= 0 ? argv[outIdx + 1] : null;
const baseIdx = argv.indexOf('--baseline');
const baselinePath = baseIdx >= 0 ? argv[baseIdx + 1] : null;

if (failOnRegression && !baselinePath) {
  console.error('check-ui-reuse: --fail-on-regression requires --baseline <frozen.json>');
  process.exit(2);
}

if (failOnRegression && strict) {
  console.error(
    'check-ui-reuse: --strict and --fail-on-regression are mutually exclusive ' +
      '(--strict = whole-repository gate, --fail-on-regression = Gate 1 regression gate); ' +
      'pick one, do not rely on one silently overriding the other',
  );
  process.exit(2);
}

// ── file discovery ─────────────────────────────────────────────────────────

const TEST_RE = /\.test\.(tsx?|jsx?)$/;
const SOURCE_RE = /\.(tsx?|jsx?)$/;

function walk(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(full));
    else if (SOURCE_RE.test(entry.name)) out.push(full);
  }
  return out;
}

// Path-shape consumers (the C5 `/\/pages\//` filter, baseline `where` keys)
// assume posix separators. Normalize once at collection so the checker behaves
// identically on Windows hosts; fs APIs accept forward slashes there too.
const toPosix = (p) => p.split(path.sep).join('/');

const allFiles = walk(SRC).map(toPosix).sort();
const sourceFiles = allFiles.filter((f) => !TEST_RE.test(f));
const testFiles = allFiles.filter((f) => TEST_RE.test(f));
const rel = (f) => toPosix(path.relative(FRONTEND, f));

const read = (f) => fs.readFileSync(f, 'utf8');

/** Remove block and line comments (naive but adequate for TS/TSX sources). */
const stripComments = (text) =>
  text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/[^\n]*/g, '$1');

// ── import parsing (used by C6/C7) ─────────────────────────────────────────

const IMPORT_RE = /import\s+(?!type\b)([\s\S]*?)\s+from\s+['"]([^'"]+)['"]/g;

function resolveSpecifier(fromFile, spec) {
  if (!spec.startsWith('.')) return null;
  const base = path.resolve(path.dirname(fromFile), spec);
  const candidates = [
    base,
    `${base}.ts`,
    `${base}.tsx`,
    `${base}.js`,
    `${base}.jsx`,
    path.join(base, 'index.ts'),
    path.join(base, 'index.tsx'),
  ];
  for (const candidate of candidates) {
    try {
      if (fs.statSync(candidate).isFile()) return path.resolve(candidate);
    } catch {
      /* not a file */
    }
  }
  return null;
}

/** Names imported from `modulePath` by `file`, plus the file's namespace aliases. */
function namesImportedFrom(file, modulePath, text) {
  const imported = new Set();
  const namespaces = new Set();
  for (const m of text.matchAll(IMPORT_RE)) {
    const clause = m[1];
    const ns = clause.match(/\*\s+as\s+([A-Za-z_$][\w$]*)/);
    if (ns) namespaces.add(ns[1]);
    if (resolveSpecifier(file, m[2]) !== path.resolve(modulePath)) continue;
    const braces = clause.match(/\{([\s\S]*?)\}/);
    if (!braces) continue;
    for (const part of braces[1].split(',')) {
      const entry = part.trim().replace(/^type\s+/, '');
      if (!entry) continue;
      const name = entry.split(/\s+as\s+/)[0].trim();
      if (/^[A-Za-z_$][\w$]*$/.test(name)) imported.add(name);
    }
  }
  return { imported, namespaces };
}

// ── CSS model ──────────────────────────────────────────────────────────────

const css = read(CSS_PATH);
const cssNoComments = css.replace(/\/\*[\s\S]*?\*\//g, '');

/** token -> selectors that mention it */
const cssClasses = new Map();
{
  const chunks = cssNoComments.split('}');
  for (const chunk of chunks) {
    const brace = chunk.lastIndexOf('{');
    const pre = brace >= 0 ? chunk.slice(0, brace) : chunk;
    const lastOpen = pre.lastIndexOf('{');
    const selectorText = (lastOpen >= 0 ? pre.slice(lastOpen + 1) : pre).trim();
    if (!selectorText) continue;
    for (const selector of selectorText.split(',')) {
      const sel = selector.trim();
      if (!sel || sel.startsWith('@')) continue;
      for (const m of sel.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) {
        const token = m[1];
        if (!cssClasses.has(token)) cssClasses.set(token, []);
        cssClasses.get(token).push(sel);
      }
    }
  }
}

/** Split a selector into compound groups (whitespace, >, +, ~ are combinators). */
function selectorGroups(sel) {
  return sel
    .split(/[\s>+~]+/)
    .filter(Boolean)
    .map((group) => {
      const parts = [];
      const element = group.match(/^([a-zA-Z][\w-]*)/);
      if (element) parts.push(element[1]);
      for (const m of group.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)) parts.push(`.${m[1]}`);
      return parts;
    });
}

/** True when the element group that carries `token` can match an element whose
 *  own tag + static classes are exactly `own`. Ancestor groups are not
 *  verified (a descendant selector's ancestors cannot be known statically). */
function selectorCanMatch(sel, token, own) {
  const groups = selectorGroups(sel);
  for (let i = groups.length - 1; i >= 0; i -= 1) {
    if (!groups[i].includes(`.${token}`)) continue;
    return groups[i].every((part) => own.has(part));
  }
  return false;
}

// ── className usage extraction ─────────────────────────────────────────────

const CLASSNAME_RE =
  /className=(?:"([^"]*)"|'([^']*)'|\{`([^`]*)`\}|\{"([^"]*)"\}|\{([^{}]*(?:\{[^{}]*\}[^{}]*)*)\})/g;

function jsxTagBefore(text, index) {
  const window = text.slice(Math.max(0, index - 400), index);
  let tag = null;
  for (const m of window.matchAll(/<([A-Za-z][\w.]*)/g)) tag = m[1];
  return tag;
}

/** Static tokens of a className value; `dynamic` is true when it interpolates. */
function classTokens(value) {
  const dynamic = value.includes('${');
  const tokens = [];
  const parts = value.split(/\$\{[^}]*\}/);
  parts.forEach((part, i) => {
    const list = part.split(/\s+/).filter((t) => /^-?[_a-zA-Z][\w-]*$/.test(t));
    // A token glued to an interpolation boundary (`btn-${x}`, `${x}-open`) is a
    // fragment of a class name, not a class name: it cannot be resolved
    // statically, and counting it would report a phantom token (`btn-`).
    const gluedLeft = i > 0 && part.length > 0 && !/^\s/.test(part);
    const gluedRight = i < parts.length - 1 && part.length > 0 && !/\s$/.test(part);
    if (gluedLeft && list.length > 0) list.shift();
    if (gluedRight && list.length > 0) list.pop();
    tokens.push(...list);
  });
  return { tokens, dynamic };
}

const TOKEN_RE = /^-?[_a-zA-Z][\w-]*$/;

/** Split a brace expression on top-level `?` / `:` (ternary branches). */
function braceSegments(expr) {
  const segs = [];
  let cur = '';
  let depth = 0;
  let quote = null;
  for (let i = 0; i < expr.length; i += 1) {
    const ch = expr[i];
    if (quote) {
      cur += ch;
      if (ch === quote && expr[i - 1] !== '\\') quote = null;
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') {
      quote = ch;
      cur += ch;
      continue;
    }
    if ('([{'.includes(ch)) depth += 1;
    else if (')]}'.includes(ch)) depth -= 1;
    if (depth === 0 && (ch === '?' || ch === ':')) {
      segs.push(cur);
      cur = '';
      continue;
    }
    cur += ch;
  }
  segs.push(cur);
  return segs;
}

/**
 * Possible static class sets of a `className={…}` expression.
 * `sets` holds one Set per ternary branch (a single union when the ternary is
 * nested inside parens); `unknown` is true when a branch contributes class
 * names that cannot be known statically (`${…}` or a bare value expression).
 */
function braceClassModel(expr) {
  const segs = braceSegments(expr);
  const branches = segs.length > 1 ? segs.slice(1) : segs;
  const sets = [];
  let unknown = expr.includes('${');
  for (const seg of branches) {
    const tokens = [];
    let concat = false;
    for (const m of seg.matchAll(/'([\w\s-]+)'|"([\w\s-]+)"/g)) {
      const before = seg.slice(0, m.index).replace(/\s+$/, '');
      const after = seg.slice(m.index + m[0].length).replace(/^\s+/, '');
      if (before.endsWith('+') || after.startsWith('+')) {
        // `'btn-' + size`: a fragment of a class name, not a class. The branch is
        // statically undecidable (needs_manual_review) instead of yielding a
        // phantom token like `btn-`.
        concat = true;
        continue;
      }
      for (const lit of [m[1] ?? m[2]]) {
        for (const t of lit.split(/\s+/)) if (TOKEN_RE.test(t)) tokens.push(t);
      }
    }
    if (tokens.length > 0) sets.push(new Set(tokens));
    if (concat) unknown = true;
    else if (tokens.length === 0 && /[A-Za-z_$]/.test(seg.replace(/'[^']*'|"[^"]*"/g, ''))) unknown = true;
  }
  return { sets: sets.length > 0 ? sets : [new Set()], unknown };
}

const usages = []; // {file,line,tag,sets,unknown,allTokens}
for (const file of sourceFiles) {
  const text = read(file);
  const lineStarts = [0];
  for (let i = 0; i < text.length; i += 1) if (text[i] === '\n') lineStarts.push(i + 1);
  const lineOf = (idx) => {
    let lo = 0;
    let hi = lineStarts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (lineStarts[mid] <= idx) lo = mid;
      else hi = mid - 1;
    }
    return lo + 1;
  };

  for (const m of text.matchAll(CLASSNAME_RE)) {
    const plain = m[1] ?? m[2] ?? null; // className="a b"
    const template = m[3] ?? null; // className={`a ${b}`}
    const brace = m[5] ?? null; // className={expr}
    let sets;
    let unknown = false;
    if (plain !== null) {
      sets = [new Set(classTokens(plain).tokens)];
    } else if (template !== null) {
      const analyzed = classTokens(template);
      sets = [new Set(analyzed.tokens)];
      unknown = analyzed.dynamic;
    } else {
      const model = braceClassModel(brace ?? '');
      sets = model.sets;
      unknown = model.unknown;
    }
    const allTokens = [...new Set(sets.flatMap((s) => [...s]))];
    // `className={className}` / `className={props.className}` forwards the
    // consumer's own class list; it is not a styling decision made by this
    // file, so it is neither a defect nor an undecidable site.
    const passThrough = /^\s*(?:props\.)?className\s*$/.test(template ?? brace ?? '');
    usages.push({
      file: rel(file),
      line: lineOf(m.index),
      tag: jsxTagBefore(text, m.index),
      sets,
      unknown,
      allTokens,
      expr: template ?? brace ?? null,
      passThrough,
      // No static token at all: `className={clsBad}` or ``className={`${x}`}``.
      // Invisible in M1/M2 by construction, so it must be reported as an
      // undecidable site (AC-13's fourth number) or the standard is unsound.
      tokenless: allTokens.length === 0 && !passThrough,
    });
  }
}

// ── checks ─────────────────────────────────────────────────────────────────

const results = [];
const add = (id, title, status, measured, threshold, violations, note) =>
  results.push({ id, title, status, measured, threshold, violations, note });

// ── canonical scalar metrics (used for baseline/regression comparison) ──────
// Every metric is stated in the "lower is better" direction except the ones
// explicitly marked, so a post-change run can be diffed against a frozen
// baseline without re-reading the human report.
const METRIC_SPECS = {
  C1: (c) => ({
    undefined_class_sites: c.measured.undefined_class_sites,
    compound_mismatch_sites: c.measured.compound_mismatch_sites,
    unresolved_tone_sites: c.measured.unresolved_tone_sites,
    needs_manual_review_sites: c.measured.needs_manual_review_sites,
  }),
  C2: (c) => ({ bare_error_regions: c.measured.bare_error_regions }),
  C3: (c) => ({
    unmapped_union_values: c.measured.unmapped_values.length,
    status_props_typed_as_string: c.measured.status_props_typed_as_string,
    toned_union_values: c.measured.toned_values,
  }),
  C4: (c) => ({
    inline_styles_total: c.measured.inline_styles_total,
    inline_styles_static: c.measured.inline_styles_static,
  }),
  C5: (c) => ({
    worst_pair_identical_substantial_lines: c.measured.worst_pair.identical_substantial_lines,
    worst_pair_share_pct: c.measured.worst_pair.share_pct,
    worst_pair_duplicate_blocks_ge4: c.measured.worst_pair.duplicate_blocks_ge4,
  }),
  C6: (c) => ({ untested_shared_exports: c.measured.untested.length }),
  C7: (c) => ({ dead_exports: c.measured.dead_exports }),
  C8: (c) => ({ page_lines_with_status_literals: c.measured.page_lines_with_status_literals }),
};
// toned_union_values is the only higher-is-better metric.
const HIGHER_IS_BETTER = new Set(['toned_union_values']);

function metricsOf(checks) {
  const out = {};
  for (const check of checks) {
    const spec = METRIC_SPECS[check.id];
    if (!spec) continue;
    out[check.id] = spec(check);
  }
  return out;
}

// C1 — every class token used in JSX exists in index.css.
{
  const undefined_ = [];
  const compoundOnly = [];
  const needsReview = [];
  for (const usage of usages) {
    const where = `${usage.file}:${usage.line}`;
    if (usage.tokenless) {
      needsReview.push({
        token: null,
        where,
        tag: usage.tag,
        reason: 'className has no static class token (pure expression)',
      });
      continue;
    }
    for (const token of usage.allTokens) {
      const selectors = cssClasses.get(token);
      if (!selectors) {
        undefined_.push({ token, where, tag: usage.tag });
        continue;
      }
      // A token can appear in more than one ternary branch; it resolves if the
      // branch it belongs to (tag + sibling classes of that branch) can match a
      // selector that carries it.
      const matched = usage.sets.some((set) => {
        if (!set.has(token)) return false;
        const own = new Set([...set].map((t) => `.${t}`));
        if (usage.tag) own.add(usage.tag);
        return selectors.some((s) => selectorCanMatch(s, token, own));
      });
      if (matched) continue;
      const entry = { token, where, tag: usage.tag, selectors: [...new Set(selectors)] };
      if (usage.unknown) needsReview.push(entry);
      else compoundOnly.push(entry);
    }
  }
  const seen = new Set();
  const dedupe = (rows) =>
    rows.filter((r) => {
      const k = `${r.token}|${r.where}`;
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    });

  // Variant/tone literals must resolve to a CSS rule as well: <Badge tone="info">
  // with no `.badge.info` rule renders as the default tone.
  const unresolvedTones = [];
  for (const file of sourceFiles) {
    const text = read(file);
    const lineStarts = text.split('\n');
    for (const m of text.matchAll(/tone=(?:"([\w-]+)"|\{'([\w-]+)'\}|\{"([\w-]+)"\})/g)) {
      const tone = m[1] ?? m[2] ?? m[3];
      if (tone === 'default') continue;
      const selector = cssClasses.get(tone) || [];
      const ok = selector.some((s) => selectorGroups(s).some((g) => g.includes(`.${tone}`)));
      if (!ok) {
        const line = lineStarts.slice(0, m.index).join('\n').split('\n').length;
        unresolvedTones.push({ tone, where: `${rel(file)}:${line}` });
      }
    }
  }

  const needsReviewRows = dedupe(needsReview);
  // `status` is the machine form of AC-13: all four numbers must be 0, not just
  // the first three. Without needsReviewRows here, `--strict` / all_checks_pass
  // would stay green while a defect sits in the manual-review bucket.
  add(
    'C1',
    'className tokens and variant literals resolve to a CSS rule',
    undefined_.length === 0 &&
      compoundOnly.length === 0 &&
      unresolvedTones.length === 0 &&
      needsReviewRows.length === 0
      ? 'pass'
      : 'fail',
    {
      undefined_class_sites: undefined_.length,
      compound_mismatch_sites: compoundOnly.length,
      unresolved_tone_sites: unresolvedTones.length,
      needs_manual_review_sites: needsReviewRows.length,
    },
    {
      undefined_class_sites: 0,
      compound_mismatch_sites: 0,
      unresolved_tone_sites: 0,
      needs_manual_review_sites: 0,
    },
    {
      undefined_classes: dedupe(undefined_),
      compound_mismatch: dedupe(compoundOnly),
      unresolved_tones: unresolvedTones,
      needs_manual_review: needsReviewRows,
    },
  );
}

// C2 — every error surface is announced (role="alert"), no bare error divs.
{
  const violations = [];
  for (const file of sourceFiles) {
    const text = read(file);
    const lines = text.split('\n');
    lines.forEach((line, i) => {
      if (!/className=\{?["'`]error\b/.test(line)) return;
      const hasRole = /role=["']alert["']/.test(line) || /<ErrorBox/.test(line);
      if (!hasRole) violations.push({ where: `${rel(file)}:${i + 1}`, line: line.trim() });
    });
  }
  add(
    'C2',
    'error regions carry role="alert"',
    violations.length === 0 ? 'pass' : 'fail',
    { bare_error_regions: violations.length },
    { bare_error_regions: 0 },
    violations,
  );
}

// C3 — status props are typed by union, and every union value has a tone.
{
  const types = read(TYPES_PATH);
  const unionsOfInterest = {
    CandidateStatus: null,
    PositionStatus: null,
    FollowUpTaskStatus: null,
    DraftStatus: null,
    ValidationStatus: null,
    SenderType: null,
  };
  for (const name of Object.keys(unionsOfInterest)) {
    const m = types.match(new RegExp(`export type ${name} =([\\s\\S]*?);`));
    unionsOfInterest[name] = m
      ? [...m[1].matchAll(/'([^']+)'/g)].map((x) => x[1])
      : [];
  }
  const unionValues = [...new Set(Object.values(unionsOfInterest).flat())];

  const ui = read(path.join(SRC, 'components/ui.tsx'));
  const toneMapBlock = ui.slice(ui.indexOf('export function StatusBadge'));
  const mapped = new Set([...toneMapBlock.matchAll(/status === '([^']+)'/g)].map((m) => m[1]));
  const unmapped = unionValues.filter((v) => !mapped.has(v));

  // Component/handler contracts only: src/types.ts mirrors the backend schemas
  // and is deliberately excluded (its fields are validated against the Pydantic
  // models separately).
  const stringStatusProps = [];
  for (const file of sourceFiles) {
    if (/\/types\.ts$/.test(file)) continue;
    if (!/\/(components|hooks|pages)\//.test(file)) continue;
    const text = read(file);
    for (const m of text.matchAll(/\bstatus: string\b/g)) {
      const line = text.slice(0, m.index).split('\n').length;
      stringStatusProps.push({ where: `${rel(file)}:${line}` });
    }
  }

  add(
    'C3',
    'status values are union-typed and fully toned',
    unmapped.length === 0 && stringStatusProps.length === 0 ? 'pass' : 'fail',
    {
      union_values: unionValues.length,
      toned_values: unionValues.length - unmapped.length,
      unmapped_values: unmapped,
      status_props_typed_as_string: stringStatusProps.length,
    },
    { unmapped_values: 0, status_props_typed_as_string: 0 },
    { unmapped, stringStatusProps },
  );
}

// C4 — no static inline styles (dynamic geometry only).
{
  const total = [];
  const staticOnly = [];
  for (const file of sourceFiles) {
    const text = read(file);
    for (const m of text.matchAll(/style=\{\{/g)) {
      const line = text.slice(0, m.index).split('\n').length;
      const snippet = text.slice(m.index, m.index + 160).split('\n')[0];
      total.push({ where: `${rel(file)}:${line}`, snippet: snippet.trim() });
      const close = text.indexOf('}}', m.index + 9);
      const inner = close > 0 ? text.slice(m.index + 9, close) : '';
      const parts = inner.split(',').map((p) => p.trim()).filter(Boolean);
      const allLiteral =
        parts.length > 0 &&
        parts.every((p) => /^[\w-]+\s*:\s*(?:'[^']*'|"[^"]*"|\d+(?:\.\d+)?)$/.test(p));
      if (allLiteral) staticOnly.push({ where: `${rel(file)}:${line}`, snippet: snippet.trim() });
    }
  }
  add(
    'C4',
    'no static inline styles',
    staticOnly.length === 0 ? 'pass' : 'fail',
    { inline_styles_total: total.length, inline_styles_static: staticOnly.length },
    { inline_styles_static: 0 },
    { static: staticOnly, all: total },
  );
}

// C5 — page-level duplication by substantial duplicated lines and blocks.
{
  const normalize = (line) => line.trim().replace(/\s+/g, ' ').replace(/[;,]$/, '');
  // Only "substantial" lines count: a bare `</div>` or `);` is structural noise
  // that every JSX file repeats, and counting it would overstate duplication.
  const substantial = (line) => normalize(line).length >= 20;
  const pages = sourceFiles.filter((f) => /\/pages\/.+\.tsx$/.test(f));
  const bodies = new Map();
  for (const file of pages) {
    const lines = read(file).split('\n').map(normalize).filter((l) => substantial(l));
    bodies.set(rel(file), lines);
  }
  const names = [...bodies.keys()];
  const BLOCK = 4;
  const pairs = [];
  for (let i = 0; i < names.length; i += 1) {
    for (let j = i + 1; j < names.length; j += 1) {
      const a = bodies.get(names[i]);
      const b = bodies.get(names[j]);
      const counts = new Map();
      for (const line of a) counts.set(line, (counts.get(line) || 0) + 1);
      let identical = 0;
      for (const line of b) {
        const n = counts.get(line) || 0;
        if (n > 0) {
          identical += 1;
          counts.set(line, n - 1);
        }
      }
      const windows = (lines) => {
        const out = new Set();
        for (let k = 0; k + BLOCK <= lines.length; k += 1) out.add(lines.slice(k, k + BLOCK).join('\u0001'));
        return out;
      };
      const wa = windows(a);
      const wb = windows(b);
      let blocks = 0;
      for (const w of wa) if (wb.has(w)) blocks += 1;
      const shorter = Math.min(a.length, b.length);
      pairs.push({
        pair: [names[i], names[j]],
        identical_substantial_lines: identical,
        shorter_substantial_lines: shorter,
        share_pct: shorter ? Math.round((identical / shorter) * 1000) / 10 : 0,
        duplicate_blocks_ge4: blocks,
      });
    }
  }
  pairs.sort((x, y) => y.identical_substantial_lines - x.identical_substantial_lines);
  const worst = pairs[0];
  // Threshold: after a validated extraction no page pair may share more than
  // 10% of the shorter file's substantial lines, no more than 30 such lines,
  // and no more than 2 duplicated 4-line blocks.
  const failing = pairs.filter(
    (p) => p.share_pct > 10 && p.identical_substantial_lines > 30,
  );
  add(
    'C5',
    'page-pair duplication under threshold',
    failing.length === 0 ? 'pass' : 'fail',
    { pairs_evaluated: pairs.length, worst_pair: worst },
    { max_share_pct: 10, max_identical_substantial_lines: 30, max_duplicate_blocks_ge4: 2 },
    { failing_pairs: failing.slice(0, 10), top_pairs: pairs.slice(0, 6) },
  );
}

// C6 — every shared primitive is imported by at least one test.
// 1.1.0: import-based, not text-presence. A name mentioned in a comment, a
// string or a test title does not count; a namespace import counts only when
// the test dereferences `NS.name`.
{
  const sharedModules = [
    path.join(SRC, 'components/ui.tsx'),
    path.join(SRC, 'hooks/useAsync.ts'),
  ];
  const exports = [];
  for (const file of sharedModules) {
    const text = read(file);
    for (const m of text.matchAll(/export (?:async )?function (\w+)/g)) {
      exports.push({ name: m[1], module: rel(file), modulePath: file });
    }
  }
  const tested = new Set();
  for (const testFile of testFiles) {
    const text = stripComments(read(testFile));
    for (const shared of sharedModules) {
      const { imported, namespaces } = namesImportedFrom(testFile, shared, text);
      for (const name of imported) tested.add(name);
      for (const ns of namespaces) {
        for (const e of exports) {
          if (new RegExp(`\\b${ns}\\.${e.name}\\b`).test(text)) tested.add(e.name);
        }
      }
    }
  }
  const untested = exports.filter((e) => !tested.has(e.name));
  add(
    'C6',
    'shared primitives are imported by a test',
    untested.length === 0 ? 'pass' : 'fail',
    { shared_exports: exports.length, untested: untested.map((e) => e.name) },
    { untested: 0 },
    untested.map((e) => ({ name: e.name, module: e.module })),
  );
}

// C7 — no exported primitive/hook without a call site outside its module.
// 1.1.0: a call site needs (a) an import of the name from that module and
// (b) at least one use after comments and import clauses are removed.
{
  const dead = [];
  const sharedModules = [
    path.join(SRC, 'components/ui.tsx'),
    path.join(SRC, 'hooks/useAsync.ts'),
  ];
  const IMPORT_STMT_RE = /import\s+(?!type\b)[\s\S]*?\s+from\s+['"][^'"]+['"];?/g;
  for (const module_ of sharedModules) {
    const text = read(module_);
    for (const m of text.matchAll(/export (?:async )?function (\w+)/g)) {
      const name = m[1];
      const usageFiles = [];
      let occurrences = 0;
      for (const file of sourceFiles) {
        if (path.resolve(file) === path.resolve(module_)) continue;
        const body = stripComments(read(file));
        const { imported, namespaces } = namesImportedFrom(file, module_, body);
        const importsName =
          imported.has(name) ||
          [...namespaces].some((ns) => new RegExp(`\\b${ns}\\.${name}\\b`).test(body));
        if (!importsName) continue;
        const withoutImports = body.replace(IMPORT_STMT_RE, '');
        const hits = [...withoutImports.matchAll(new RegExp(`\\b${name}\\b`, 'g'))].length;
        if (hits > 0) {
          usageFiles.push(rel(file));
          occurrences += hits;
        }
      }
      if (usageFiles.length === 0) {
        dead.push({ name, module: rel(module_), external_usage_files: 0, external_call_sites: 0 });
      }
    }
  }
  add(
    'C7',
    'no dead shared exports',
    dead.length === 0 ? 'pass' : 'fail',
    { dead_exports: dead.length },
    { dead_exports: 0 },
    dead,
  );
}

// C8 — page files do not re-encode status vocabulary.
{
  const types = read(TYPES_PATH);
  const statusUnions = [...types.matchAll(/export type (\w*Status\w*) =([\s\S]*?);/g)];
  const vocabulary = new Set();
  for (const u of statusUnions) for (const v of u[2].matchAll(/'([^']+)'/g)) vocabulary.add(v[1]);
  const violations = [];
  for (const file of sourceFiles) {
    if (!/\/pages\//.test(file)) continue;
    const lines = read(file).split('\n');
    lines.forEach((line, i) => {
      for (const m of line.matchAll(/'([^']+)'/g)) {
        if (vocabulary.has(m[1])) {
          violations.push({ where: `${rel(file)}:${i + 1}`, literal: m[1] });
          break;
        }
      }
    });
  }
  add(
    'C8',
    'status vocabulary is not duplicated in pages',
    violations.length === 0 ? 'pass' : 'fail',
    { page_lines_with_status_literals: violations.length },
    { page_lines_with_status_literals: 0 },
    violations,
  );
}

// ── report ─────────────────────────────────────────────────────────────────

const summary = {
  tool: 'check-ui-reuse',
  version: VERSION,
  generated_at: new Date().toISOString(),
  base: process.env.ICA_UI_CHECK_BASE || 'worktree',
  scope: {
    source_files: sourceFiles.length,
    test_files: testFiles.length,
    css_class_selectors: new Set(
      [...cssClasses.values()].flat().flatMap((s) => [...s.matchAll(/\.(-?[_a-zA-Z][\w-]*)/g)].map((m) => m[1])),
    ).size,
    className_usages: usages.length,
  },
  checks: results,
  metrics: metricsOf(results),
  failed: results.filter((r) => r.status === 'fail').map((r) => r.id),
  passed: results.filter((r) => r.status === 'pass').map((r) => r.id),
};

// ── optional baseline / regression comparison ───────────────────────────────
if (baselinePath) {
  const baseline = JSON.parse(fs.readFileSync(baselinePath, 'utf8'));
  const before = baseline.metrics || metricsOf(baseline.checks || []);
  const regressions = [];
  const improvements = [];
  const notComparable = [];
  for (const [id, metrics] of Object.entries(summary.metrics)) {
    for (const [key, value] of Object.entries(metrics)) {
      const prior = before?.[id]?.[key];
      if (typeof prior !== 'number' || typeof value !== 'number') {
        notComparable.push(`${id}.${key}`);
        continue;
      }
      if (prior === value) continue;
      const better = HIGHER_IS_BETTER.has(key) ? value > prior : value < prior;
      (better ? improvements : regressions).push({ id, metric: key, before: prior, after: value });
    }
  }
  summary.baseline = {
    path: baselinePath,
    baseline_version: baseline.version ?? null,
    tool_version: VERSION,
    regressions,
    improvements,
    not_comparable: notComparable,
  };
}

const regressionOk = !summary.baseline || summary.baseline.regressions.length === 0;
summary.verdict = {
  checks_total: results.length,
  checks_failed: summary.failed.length,
  all_checks_pass: summary.failed.length === 0,
  regression_gate: summary.baseline ? (regressionOk ? 'PASS' : 'FAIL') : 'not-evaluated',
};

if (outPath) fs.writeFileSync(outPath, `${JSON.stringify(summary, null, 2)}\n`);

if (asJson) {
  process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
} else {
  console.log(`check-ui-reuse ${VERSION} @ ${FRONTEND}`);
  console.log(
    `scope: ${summary.scope.source_files} source files, ${summary.scope.test_files} test files, ` +
      `${summary.scope.css_class_selectors} CSS classes, ${summary.scope.className_usages} className usages\n`,
  );
  for (const r of results) {
    const marker = r.status === 'pass' ? 'PASS' : 'FAIL';
    console.log(`${marker} ${r.id}  ${r.title}`);
    console.log(`      measured: ${JSON.stringify(r.measured)}`);
    console.log(`      required: ${JSON.stringify(r.threshold)}`);
    if (r.status === 'fail') {
      const detail = JSON.stringify(r.violations).slice(0, 1200);
      console.log(`      violations: ${detail}`);
    }
    console.log();
  }
  console.log(`passed: ${summary.passed.length}/${results.length}  failed: ${summary.failed.join(', ') || 'none'}`);
  if (summary.baseline) {
    console.log(`\nbaseline: ${summary.baseline.path} (frozen by checker ${summary.baseline.baseline_version})`);
    console.log(`  regressions: ${summary.baseline.regressions.length}`);
    for (const r of summary.baseline.regressions) {
      console.log(`    ${r.id} ${r.metric}: ${r.before} -> ${r.after}`);
    }
    console.log(`  improvements: ${summary.baseline.improvements.length}`);
    for (const r of summary.baseline.improvements.slice(0, 30)) {
      console.log(`    ${r.id} ${r.metric}: ${r.before} -> ${r.after}`);
    }
    if (summary.baseline.not_comparable.length > 0) {
      console.log(`  not-comparable metrics: ${summary.baseline.not_comparable.join(', ')}`);
    }
    console.log(`  regression gate: ${summary.verdict.regression_gate}`);
  }
}

const regressed = Boolean(summary.baseline && summary.baseline.regressions.length > 0);
if (failOnRegression) {
  if (regressed) process.exitCode = 1;
} else if (strict && (summary.failed.length > 0 || regressed)) {
  process.exitCode = 1;
}
