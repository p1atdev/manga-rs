// Local-only benchmark: Comici JPEG -> WebP fallback inside a local Miniflare/workerd.
// Run from anywhere: node crates/manga-image-wasm/scripts/benchmark-workerd.mjs
// Optional: IMAGE_WASM_PATH (wasm override), MINIFLARE_ROOT (node_modules dir containing miniflare).
// Output: one JSON document on stdout. It measures local workerd only; it says nothing about Cloudflare's edge.
import { createRequire } from 'node:module';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const { Miniflare } = process.env.MINIFLARE_ROOT ? require(`${process.env.MINIFLARE_ROOT}/miniflare`) : require('miniflare');
const sharp = process.env.MINIFLARE_ROOT ? require(`${process.env.MINIFLARE_ROOT}/sharp`) : require('sharp');

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const fixtureBase = path.join(root, 'crates/cli/tests/assets/comici-youngchampion-public-20261111');
const wasmPath = process.env.IMAGE_WASM_PATH
  ? path.resolve(process.env.IMAGE_WASM_PATH)
  : path.join(root, 'target/wasm32-unknown-unknown/release/manga_image_wasm.wasm');

const WARMUP = 5;
const SAMPLES = 100;
const T_CRIT_95_DF99 = 1.984; // two-sided 95%, df = SAMPLES - 1

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

const input = await readFile(`${fixtureBase}.jpg`);
const permutation = JSON.parse(await readFile(`${fixtureBase}.permutation.json`, 'utf8'));
if (!Array.isArray(permutation.tile_sources)) throw new Error('permutation.json has no tile_sources array');
const wasm = await readFile(wasmPath);
const wrapper = await readFile(path.join(root, 'crates/manga-image-wasm/js/index.mjs'), 'utf8');

const modes = [
  { name: 'lossless_q75', fallback: { format: 'webp', mode: 'lossless', quality: 75 }, chunk: 'VP8L' },
  { name: 'lossy_q95', fallback: { format: 'webp', mode: 'lossy', quality: 95 }, chunk: 'VP8 ' },
];

const worker = `import module from './image.wasm'; import {createImageProcessor} from './processor.mjs'; const restore=createImageProcessor(module); export default {async fetch(req) {try {const result=restore(new Uint8Array(await req.arrayBuffer()), JSON.parse(req.headers.get('metadata'))); if(result.kind==='unsupported') return new Response(result.reason,{status:422}); return new Response(result.bytes,{headers:{'content-type':result.contentType}});} catch(e) {return new Response(e.code || String(e),{status:400});}}}`;

const mf = new Miniflare({
  cf: false,
  compatibilityDate: '2026-07-01',
  modules: [
    { type: 'ESModule', path: 'worker.mjs', contents: worker },
    { type: 'ESModule', path: 'processor.mjs', contents: wrapper },
    { type: 'CompiledWasm', path: 'image.wasm', contents: wasm },
  ],
});

// Timed region: dispatch + full body consumption. Validation happens after the clock stops.
async function invoke(mode) {
  const metadata = JSON.stringify({ kind: 'comici', scramble: permutation.tile_sources, fallback: mode.fallback });
  const start = performance.now();
  const response = await mf.dispatchFetch('http://localhost/', { method: 'POST', headers: { metadata }, body: input });
  const body = Buffer.from(await response.arrayBuffer());
  const ms = performance.now() - start;
  const contentType = response.headers.get('content-type');
  if (response.status !== 200) throw new Error(`${mode.name}: expected 200, got ${response.status}: ${body.toString('utf8', 0, 200)}`);
  if (contentType !== 'image/webp') throw new Error(`${mode.name}: expected content-type image/webp, got ${contentType}`);
  if (body.length < 16 || body.toString('latin1', 0, 4) !== 'RIFF' || body.toString('latin1', 8, 12) !== 'WEBP') {
    throw new Error(`${mode.name}: body is not a WebP (direct JPEG passthrough?)`);
  }
  if (body[0] === 0xff && body[1] === 0xd8) throw new Error(`${mode.name}: body is JPEG`);
  if (body.toString('latin1', 12, 16) !== mode.chunk && !body.toString('latin1').includes(mode.chunk)) {
    throw new Error(`${mode.name}: expected ${mode.chunk} bitstream`);
  }
  return { ms, body };
}

function stats(samples) {
  const n = samples.length;
  const mean = samples.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(samples.reduce((a, b) => a + (b - mean) ** 2, 0) / (n - 1));
  const se = sd / Math.sqrt(n);
  const sorted = [...samples].sort((a, b) => a - b);
  const median = n % 2 ? sorted[(n - 1) / 2] : (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
  const half = T_CRIT_95_DF99 * se;
  return { n, mean_ms: mean, sample_stddev_ms: sd, standard_error_ms: se, t_critical_95_df99: T_CRIT_95_DF99, ci95_ms: [mean - half, mean + half], ci95_half_width_ms: half, median_ms: median, samples_ms: samples };
}

try {
  const inputMeta = await sharp(input).metadata();
  const results = Object.fromEntries(modes.map((m) => [m.name, { samples: [], sizes: new Set(), hashes: new Set(), meta: null }]));

  // Alternate which mode goes first each round to spread drift across both modes.
  const order = (i) => (i % 2 === 0 ? modes : [...modes].reverse());
  for (let i = 0; i < WARMUP; i++) for (const mode of order(i)) await invoke(mode);
  for (let i = 0; i < SAMPLES; i++) {
    for (const mode of order(i)) {
      const { ms, body } = await invoke(mode);
      const r = results[mode.name];
      r.samples.push(ms);
      r.sizes.add(body.length);
      r.hashes.add(sha256(body));
      r.last ??= body;
    }
  }

  const output = {};
  for (const mode of modes) {
    const r = results[mode.name];
    const meta = await sharp(r.last).metadata();
    output[mode.name] = {
      fallback: mode.fallback,
      output_bytes: [...r.sizes].length === 1 ? [...r.sizes][0] : null,
      output_bytes_distinct: [...r.sizes],
      output_sha256_distinct: [...r.hashes],
      output_width: meta.width,
      output_height: meta.height,
      ...stats(r.samples),
    };
  }

  let workerdVersion = null;
  try {
    const miniflareDir = path.dirname(require.resolve('miniflare/package.json', process.env.MINIFLARE_ROOT ? { paths: [process.env.MINIFLARE_ROOT] } : undefined));
    workerdVersion = createRequire(path.join(miniflareDir, 'package.json'))('workerd/package.json').version;
  } catch {}
  let miniflareVersion = null;
  try { miniflareVersion = require('miniflare/package.json').version; } catch {}

  console.log(JSON.stringify({
    label: 'LOCAL-ONLY: local Miniflare/workerd measurement; not Cloudflare edge, no equivalence claimed',
    wall_scope: 'performance.now() around mf.dispatchFetch (request dispatch) plus full response body consumption (arrayBuffer); includes local Miniflare/workerd request overhead and Wasm processing; excludes startup, validation, hashing',
    warmup_per_mode: WARMUP,
    measured_per_mode: SAMPLES,
    order: 'alternating per round: even rounds lossless then lossy, odd rounds lossy then lossless',
    fixture: {
      path: path.relative(root, `${fixtureBase}.jpg`),
      bytes: input.length,
      sha256: sha256(input),
      width: inputMeta.width,
      height: inputMeta.height,
      tile_sources: permutation.tile_sources,
    },
    wasm: { path: process.env.IMAGE_WASM_PATH ? '(IMAGE_WASM_PATH override)' : path.relative(root, wasmPath), bytes: wasm.length, sha256: sha256(wasm) },
    environment: {
      node: process.version,
      miniflare: miniflareVersion,
      workerd: workerdVersion,
      platform: `${os.platform()} ${os.release()} ${os.arch()}`,
      cpu_model: os.cpus()[0]?.model ?? null,
      cpu_count: os.cpus().length,
    },
    modes: output,
  }, null, 2));
} finally {
  await mf.dispose();
}
