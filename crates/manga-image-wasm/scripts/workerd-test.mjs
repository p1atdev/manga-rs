import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
const require = createRequire(import.meta.url);
const { Miniflare } = process.env.MINIFLARE_ROOT ? require(`${process.env.MINIFLARE_ROOT}/miniflare`) : require('miniflare');
const input = await readFile('crates/manga/tests/assets/giga-tonari-yj-public-20261111.jpg');
const webp = await readFile('crates/manga/tests/assets/kadocomi-decrypted.webp');
const encryptedWebp = await readFile('crates/manga/tests/assets/kadocomi-encrypted.webp');
const encryptedJpeg = await readFile('crates/manga/tests/assets/fuz-encrypted.jpeg');
const progressive = await readFile('crates/manga/tests/assets/giga-public-progressive.jpg');
const wasm = await readFile(process.env.IMAGE_WASM_PATH ?? 'target/wasm32-unknown-unknown/release/manga_image_wasm.wasm');
const wrapper = await readFile('crates/manga-image-wasm/js/index.mjs', 'utf8');
const worker = `import module from './image.wasm'; import {createImageProcessor} from './processor.mjs'; const restore=createImageProcessor(module); export default {async fetch(req) {try {const result=restore(new Uint8Array(await req.arrayBuffer()), JSON.parse(req.headers.get('metadata'))); if(result.kind==='unsupported') return new Response(result.reason,{status:422}); return new Response(result.bytes,{headers:{'content-type':result.contentType,'x-memory-bytes':String(restore.memoryBytes())}});} catch(e) {return new Response(e.code || String(e),{status:400});}}}`;
const mf = new Miniflare({ cf: false, compatibilityDate: '2026-07-01', modules: [
  { type: 'ESModule', path: 'worker.mjs', contents: worker },
  { type: 'ESModule', path: 'processor.mjs', contents: wrapper },
  { type: 'CompiledWasm', path: 'image.wasm', contents: wasm },
] });
async function run(bytes, metadata) { const r=await mf.dispatchFetch('http://localhost/',{method:'POST',headers:{metadata:JSON.stringify(metadata)},body:bytes}); return [r, Buffer.from(await r.arrayBuffer())]; }
try {
  const [ok, output] = await run(input,{kind:'giga'});
  if(ok.status!==200 || ok.headers.get('content-type')!=='image/jpeg' || output[0]!==0xff) throw Error(`giga failed: ${ok.status} ${output}`);
  if (process.env.WORKERD_OUTPUT) await writeFile(process.env.WORKERD_OUTPUT, output);
  const [twice, back] = await run(output,{kind:'giga'});
  if(twice.status!==200) throw Error('second Giga transform failed');
  if (process.env.WORKERD_TWICE_OUTPUT) await writeFile(process.env.WORKERD_TWICE_OUTPUT, back);
  const [raw, preserved] = await run(webp,{kind:'kadokomi_raw'});
  if(raw.status!==200 || raw.headers.get('content-type')!=='image/webp' || !webp.equals(preserved)) throw Error(`WebP preserve failed: ${raw.status} ${preserved}`);
  const [xor, decryptedWebp] = await run(encryptedWebp,{kind:'kadokomi_xor',keyHex:'0123456789abcdef'});
  if(xor.status!==200 || !decryptedWebp.equals(webp)) throw Error('Kadokomi XOR failed');
  const { createDecipheriv } = await import('node:crypto');
  const key = Buffer.from('2e009856520e10917accae78097a2e13d9dd7a97d3a5ea293527ec9d0132bba3','hex');
  const iv = Buffer.from('e8c7e042d6ba9fb85c128d5ceb64b82f','hex');
  const decipher = createDecipheriv('aes-256-cbc',key,iv);
  const plainJpeg = Buffer.concat([decipher.update(encryptedJpeg),decipher.final()]);
  const [fuz, decryptedJpeg] = await run(encryptedJpeg,{kind:'fuz_aes_cbc',keyHex:key.toString('hex'),ivHex:iv.toString('hex')});
  if(fuz.status!==200 || fuz.headers.get('content-type')!=='image/jpeg' || !decryptedJpeg.equals(plainJpeg)) throw Error(`FUZ AES failed: ${fuz.status}`);
  const [badDimensions, dimensionError] = await run(input,{kind:'giga',width:1,height:1});
  if(badDimensions.status!==400 || dimensionError.toString()!=='malformed') throw Error('dimension mismatch not rejected');
  const [badScramble, scrambleError] = await run(input,{kind:'comici',scramble:Array(16).fill(0)});
  if(badScramble.status!==400 || scrambleError.toString()!=='invalid_argument') throw Error('invalid permutation not rejected');
  const [badWebp] = await run(webp.subarray(0,Math.floor(webp.length/2)),{kind:'kadokomi_raw'});
  if(badWebp.status!==400) throw Error('corrupt WebP accepted');
  const [progressiveResponse, progressiveOutput] = await run(progressive, {kind:'fuz_raw'});
  if(progressiveResponse.status!==200 || !progressive.equals(progressiveOutput)) throw Error('progressive EXIF JPEG not preserved');
  for (const [name, image] of [['baseline', input], ['progressive', progressive]]) {
    for (const percent of [10,25,50,75,90,99]) {
      const truncated = Buffer.concat([image.subarray(0, Math.floor(image.length * percent / 100)), Buffer.from([0xff, 0xd9])]);
      const [response, message] = await run(truncated, {kind:'fuz_raw'});
      if(response.status!==400 || message.toString()!=='malformed') throw Error(`${name} forged EOI accepted at ${percent}%: ${response.status} ${message}`);
    }
  }
  const [bad, reason] = await run(input.subarray(0,Math.floor(input.length/2)),{kind:'giga'});
  if(bad.status!==400 || reason.toString()!=='malformed') throw Error(`truncated classification: ${bad.status} ${reason}`);
  const [held] = await run(input,{kind:'piccoma',scrambled:true});
  if(held.status!==422) throw Error(`unsupported classification: ${held.status}`);
  const [again] = await run(input,{kind:'giga'});
  if(again.status!==200) throw Error(`after errors: ${again.status}`);
  // One processor/instance serves all requests. Alternate content and run overlapping fetches;
  // each response must match its own baseline, including immediately after classified errors.
  let memoryAfterWarmup = 0;
  let memoryFinal = 0;
  for (let batch = 0; batch < 12; batch++) {
    const results = await Promise.all(Array.from({length: 10}, (_, index) => {
      const isGiga = (batch + index) % 2 === 0;
      return run(isGiga ? input : webp, isGiga ? {kind:'giga'} : {kind:'kadokomi_raw'});
    }));
    for (let index = 0; index < results.length; index++) {
      const [response, actual] = results[index];
      const isGiga = (batch + index) % 2 === 0;
      if (response.status !== 200 || !actual.equals(isGiga ? output : webp)) throw Error(`cross-request corruption at ${batch}:${index}`);
      memoryFinal = Math.max(memoryFinal, Number(response.headers.get('x-memory-bytes')));
    }
    if (batch === 5) memoryAfterWarmup = memoryFinal;
  }
  if (memoryFinal > memoryAfterWarmup + 65536) throw Error(`memory grew after warmup: ${memoryAfterWarmup} -> ${memoryFinal}`);
  // WebP fallback integration. Failures are collected and reported with bounded, byte-free messages.
  const sharp = require('sharp');
  // Deterministic in-memory RGBA PNG (no fixture file): gradients and a hash-like term in RGB, with
  // alpha varying and fully transparent (alpha=0) pixels that still carry non-zero RGB.
  const syntheticWidth = 173;
  const syntheticHeight = 137;
  const syntheticRaw = Buffer.alloc(syntheticWidth * syntheticHeight * 4);
  for (let y = 0; y < syntheticHeight; y++) {
    for (let x = 0; x < syntheticWidth; x++) {
      const offset = (y * syntheticWidth + x) * 4;
      const mix = Math.imul(x * 73856093 ^ y * 19349663, 2654435761) >>> 24;
      syntheticRaw[offset] = (x * 255 / (syntheticWidth - 1) + mix) & 0xff;
      syntheticRaw[offset + 1] = (y * 255 / (syntheticHeight - 1) + (mix >> 1)) & 0xff;
      syntheticRaw[offset + 2] = (x * 7 + y * 13 + mix) & 0xff;
      syntheticRaw[offset + 3] = (x + y) % 11 === 0 ? 0 : (x ^ y) % 7 === 0 ? 128 : 255 - ((x * 3 + y) % 200);
      if (syntheticRaw[offset + 3] === 0 && syntheticRaw[offset] === 0 && syntheticRaw[offset + 1] === 0 && syntheticRaw[offset + 2] === 0) syntheticRaw[offset] = 1;
    }
  }
  const metadataPng = await sharp(syntheticRaw, {raw: {width: syntheticWidth, height: syntheticHeight, channels: 4}}).png({compressionLevel: 9}).toBuffer();
  // Walk PNG chunks with strict boundary validation: signature, every chunk fully inside the
  // buffer, and IEND as the exact last chunk. CRCs are not interpreted; chunks are copied verbatim.
  const pngChunks = (bytes) => {
    if (bytes.length < 8 || !bytes.subarray(0, 8).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))) throw Error('not a PNG');
    const chunks = [];
    let offset = 8;
    while (offset < bytes.length) {
      if (offset + 12 > bytes.length) throw Error('PNG chunk header truncated');
      const end = offset + 12 + bytes.readUInt32BE(offset);
      if (end > bytes.length) throw Error('PNG chunk overruns buffer');
      chunks.push({type: bytes.toString('latin1', offset + 4, offset + 8), start: offset, end});
      offset = end;
    }
    if (chunks.length === 0 || chunks[0].type !== 'IHDR' || chunks[chunks.length - 1].type !== 'IEND') throw Error('PNG must start with IHDR and end with IEND');
    return chunks;
  };
  const metadataChunks = pngChunks(metadataPng);
  if (!metadataChunks.some((chunk) => chunk.type === 'pHYs')) throw Error('sharp PNG has no pHYs chunk to strip');
  const syntheticPng = Buffer.concat([
    metadataPng.subarray(0, 8),
    ...metadataChunks.filter((chunk) => chunk.type !== 'pHYs').map((chunk) => metadataPng.subarray(chunk.start, chunk.end)),
  ]);
  const strippedTypes = pngChunks(syntheticPng).map((chunk) => chunk.type);
  if (strippedTypes[0] !== 'IHDR' || strippedTypes[strippedTypes.length - 1] !== 'IEND' || strippedTypes.some((type) => type !== 'IHDR' && type !== 'IDAT' && type !== 'IEND')) throw Error(`stripped PNG has unexpected chunks: ${strippedTypes.join(',')}`);
  const fallbackFailures = [];
  const bound = (value) => String(value).replace(/[^\x20-\x7e]/g, '?').slice(0, 80);
  const lossless75 = {format:'webp',mode:'lossless',quality:75};
  const lossy95 = {format:'webp',mode:'lossy',quality:95};
  const check = async (name, fn) => {
    try { await fn(); } catch (e) { fallbackFailures.push(`${name}: ${bound(e?.message ?? e)}`); }
  };
  const expect = (condition, message) => { if (!condition) throw Error(message); };
  // Top-level RIFF/WebP chunk fourccs, to tell VP8 (lossy) from VP8L (lossless) without trusting sharp.
  const webpChunks = (bytes) => {
    expect(bytes.length >= 12 && bytes.toString('latin1', 0, 4) === 'RIFF' && bytes.toString('latin1', 8, 12) === 'WEBP', 'not a RIFF/WebP');
    const chunks = [];
    for (let offset = 12; offset + 8 <= bytes.length;) {
      chunks.push(bytes.toString('latin1', offset, offset + 4));
      offset += 8 + bytes.readUInt32LE(offset + 4) + (bytes.readUInt32LE(offset + 4) & 1);
    }
    return chunks;
  };
  const decodeRgba = async (bytes) => {
    const {data, info} = await sharp(bytes).ensureAlpha().raw().toBuffer({resolveWithObject: true});
    return {data, width: info.width, height: info.height};
  };
  const expected = await decodeRgba(syntheticPng);
  // Guard the fixture itself: the PNG must round-trip the generated pixels exactly, including
  // RGB under alpha=0, and must actually contain such pixels.
  if (expected.width !== syntheticWidth || expected.height !== syntheticHeight || !expected.data.equals(syntheticRaw)) throw Error('synthetic PNG does not round-trip its source pixels');
  let hiddenRgb = 0;
  for (let offset = 0; offset < syntheticRaw.length; offset += 4) if (syntheticRaw[offset + 3] === 0 && (syntheticRaw[offset] | syntheticRaw[offset + 1] | syntheticRaw[offset + 2])) hiddenRgb++;
  if (hiddenRgb === 0) throw Error('synthetic PNG has no alpha=0 pixels with RGB');
  const decodeWebp = async (response, bytes) => {
    expect(response.status === 200, `status ${response.status}`);
    expect(response.headers.get('content-type') === 'image/webp', `content-type ${bound(response.headers.get('content-type'))}`);
    const decoded = await decodeRgba(bytes);
    expect(decoded.width === expected.width && decoded.height === expected.height, `dimensions ${decoded.width}x${decoded.height}`);
    return decoded;
  };
  await check('lossless75 PNG', async () => {
    const [response, bytes] = await run(syntheticPng, {kind:'fuz_raw', fallback: lossless75});
    const decoded = await decodeWebp(response, bytes);
    expect(webpChunks(bytes).includes('VP8L'), 'no VP8L chunk');
    expect(decoded.data.equals(expected.data), 'decoded RGBA differs from source');
  });
  await check('lossy95 PNG', async () => {
    const [response, bytes] = await run(syntheticPng, {kind:'fuz_raw', fallback: lossy95});
    await decodeWebp(response, bytes);
    const chunks = webpChunks(bytes);
    expect(chunks.includes('VP8 ') && !chunks.includes('VP8L'), `not lossy: ${chunks.slice(0, 4).join(',')}`);
  });
  await check('source WebP identical with fallback', async () => {
    for (const fallback of [lossless75, lossy95]) {
      const [response, bytes] = await run(webp, {kind:'kadokomi_raw', fallback});
      expect(response.status === 200 && response.headers.get('content-type') === 'image/webp', `status ${response.status}`);
      expect(webp.equals(bytes), `bytes changed (${fallback.mode})`);
    }
  });
  await check('raw direct JPEG identical with fallback', async () => {
    for (const [name, image] of [['baseline', input], ['progressive', progressive]]) {
      for (const fallback of [lossless75, lossy95]) {
        const [response, bytes] = await run(image, {kind:'fuz_raw', fallback});
        expect(response.status === 200 && response.headers.get('content-type') === 'image/jpeg', `${name} status ${response.status}`);
        expect(image.equals(bytes), `${name} bytes changed (${fallback.mode})`);
      }
    }
  });
  await check('Giga direct JPEG identical with fallback', async () => {
    for (const fallback of [lossless75, lossy95]) {
      const [response, bytes] = await run(input, {kind:'giga', fallback});
      expect(response.status === 200 && response.headers.get('content-type') === 'image/jpeg', `status ${response.status} (${fallback.mode})`);
      expect(output.equals(bytes), `bytes changed (${fallback.mode})`);
    }
  });
  // A PNG carrying ancillary metadata (pHYs) cannot be converted without discarding it, so core
  // must hold it as unsupported rather than silently converting.
  await check('metadata-bearing PNG held as unsupported', async () => {
    const decodedMetadata = await decodeRgba(metadataPng);
    expect(decodedMetadata.data.equals(expected.data), 'metadata PNG pixels differ from stripped PNG');
    for (const fallback of [lossless75, lossy95]) {
      const [response, body] = await run(metadataPng, {kind:'fuz_raw', fallback});
      expect(response.status === 422, `status ${response.status} (${fallback.mode})`);
      expect(response.headers.get('content-type') !== 'image/webp' && !(body.length >= 12 && body.toString('latin1', 0, 4) === 'RIFF'), `converted to WebP (${fallback.mode})`);
    }
  });
  await check('invalid fallback options', async () => {
    const invalid = [
      {format:'png',mode:'lossless',quality:75},
      {format:'webp',mode:'fast',quality:75},
      {format:'webp',mode:'lossy',quality:101},
      {format:'webp',mode:'lossy',quality:-1},
      {format:'webp',mode:'lossy',quality:'75'},
      {format:'webp',mode:'lossy',quality:75,extra:1},
      {},
    ];
    for (const [index, fallback] of invalid.entries()) {
      const [response, body] = await run(syntheticPng, {kind:'fuz_raw', fallback});
      expect(response.status === 400 && body.toString() === 'invalid_argument', `case ${index}: ${response.status} ${bound(body.toString())}`);
    }
  });
  await check('malformed image classified as malformed', async () => {
    const cases = [
      ['truncated PNG', syntheticPng.subarray(0, Math.floor(syntheticPng.length / 2))],
      ['truncated PNG header', syntheticPng.subarray(0, 40)],
      ['PNG signature with garbage', Buffer.concat([syntheticPng.subarray(0, 8), Buffer.from('definitely not a png')])],
    ];
    for (const [name, image] of cases) {
      for (const fallback of [lossless75, lossy95]) {
        const [response, body] = await run(image, {kind:'fuz_raw', fallback});
        expect(response.status === 400 && body.toString() === 'malformed', `${name} (${fallback.mode}): ${response.status} ${bound(body.toString())}`);
      }
    }
  });
  // Bytes with no recognizable image signature are not malformed images: core's format sniffing
  // (`source_format`) classifies them as `unsupported` (wasm status 1 -> HTTP 422), deliberately
  // distinct from `malformed`. The zero-byte body is the same case, not `invalid_argument`.
  await check('unrecognized input classified as unsupported', async () => {
    const cases = [['garbage', Buffer.from('definitely not an image')], ['zero-byte', Buffer.alloc(0)]];
    for (const [name, image] of cases) {
      for (const fallback of [lossless75, lossy95]) {
        const [response] = await run(image, {kind:'fuz_raw', fallback});
        expect(response.status === 422, `${name} (${fallback.mode}): ${response.status}`);
      }
    }
  });
  await check('scrambled Piccoma without seed unsupported', async () => {
    for (const fallback of [lossless75, lossy95]) {
      const [response] = await run(input, {kind:'piccoma', scrambled:true, fallback});
      expect(response.status === 422, `status ${response.status} (${fallback.mode})`);
    }
  });
  // Piccoma seeded restoration. The golden order is precomputed from crates/manga/tests/piccoma.rs
  // (shuffle_order(12, "EXAMPLESEED")); destination tile d is restored from source tile order[d].
  await check('Piccoma seeded restoration matches golden order', async () => {
    const order = [3, 7, 6, 5, 11, 1, 8, 2, 9, 10, 0, 4];
    const width = 200, height = 150, tile = 50, columns = 4;
    // Tile i is uniquely colored (red = tile id), with green/blue encoding the in-tile position so
    // misplaced or shifted pixels are detected, not just wrong tiles.
    const tilePixel = (i, lx, ly) => [20 * i + 5, lx * 5, ly * 5, 255];
    const scrambledRaw = Buffer.alloc(width * height * 4);
    const expectedRaw = Buffer.alloc(width * height * 4);
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width; x++) {
        const offset = (y * width + x) * 4;
        const destination = Math.floor(y / tile) * columns + Math.floor(x / tile);
        const [lx, ly] = [x % tile, y % tile];
        Buffer.from(tilePixel(destination, lx, ly)).copy(scrambledRaw, offset);
        Buffer.from(tilePixel(order[destination], lx, ly)).copy(expectedRaw, offset);
      }
    }
    const withPhys = await sharp(scrambledRaw, {raw: {width, height, channels: 4}}).png({compressionLevel: 9}).toBuffer();
    const chunks = pngChunks(withPhys);
    expect(chunks.some((chunk) => chunk.type === 'pHYs'), 'sharp PNG has no pHYs chunk to strip');
    const scrambledPng = Buffer.concat([
      withPhys.subarray(0, 8),
      ...chunks.filter((chunk) => chunk.type !== 'pHYs').map((chunk) => withPhys.subarray(chunk.start, chunk.end)),
    ]);
    const source = await decodeRgba(scrambledPng);
    expect(source.data.equals(scrambledRaw), 'scrambled PNG does not round-trip its source pixels');
    expect(!scrambledRaw.equals(expectedRaw), 'fixture is not actually scrambled');
    const matches = (actual) => actual.width === width && actual.height === height && actual.data.equals(expectedRaw);
    const [response, bytes] = await run(scrambledPng, {kind:'piccoma', scrambled:true, seed:'EXAMPLESEED', fallback: lossless75});
    expect(response.status === 200, `status ${response.status} ${bound(bytes.toString())}`);
    expect(response.headers.get('content-type') === 'image/webp', `content-type ${bound(response.headers.get('content-type'))}`);
    const restored = await decodeRgba(bytes);
    expect(restored.width === width && restored.height === height, `dimensions ${restored.width}x${restored.height}`);
    let differing = 0;
    for (let offset = 0; offset < expectedRaw.length; offset += 4) {
      if (!restored.data.subarray(offset, offset + 4).equals(expectedRaw.subarray(offset, offset + 4))) differing++;
    }
    expect(differing === 0, `${differing} restored pixels differ from golden`);
    // A wrong or missing seed must either be rejected or yield something other than the golden image.
    for (const [name, metadata] of [
      ['wrong seed', {kind:'piccoma', scrambled:true, seed:'WRONGSEED', fallback: lossless75}],
      ['missing seed', {kind:'piccoma', scrambled:true, fallback: lossless75}],
    ]) {
      const [other, otherBytes] = await run(scrambledPng, metadata);
      if (other.status === 200) expect(!matches(await decodeRgba(otherBytes)), `${name} produced the golden restoration`);
      else expect(other.status === 400 || other.status === 422, `${name}: unexpected status ${other.status}`);
    }
  });
  // ICC preservation through the real wasm. The Piccoma seed is synthetic, so this checks only that
  // the reassembled ICC profile survives into the WebP (ICCP chunk), not restored pixel correctness;
  // the Piccoma golden test above covers pixels.
  await check('ICC profile reassembled into exactly one ICCP chunk', async () => {
    const icc = await readFile('crates/manga/tests/assets/generated-srgb.icc');
    const jpeg = await readFile('crates/manga/tests/assets/piccoma-public-20261111.jpg');
    expect(jpeg[0] === 0xff && jpeg[1] === 0xd8, 'fixture is not a JPEG');
    expect(icc.length >= 2, 'ICC fixture too small to split');
    // Inject APP2 ICC_PROFILE\0 segments right after SOI. sequences: [1-based index, ICC part] in
    // injection order; total is the declared count.
    const withIcc = (segments, total) => {
      const parts = [];
      for (const [sequence, part] of segments) {
        const header = Buffer.alloc(4);
        header.writeUInt16BE(0xffe2, 0);
        header.writeUInt16BE(2 + 12 + 2 + part.length, 2);
        parts.push(header, Buffer.from('ICC_PROFILE\0', 'latin1'), Buffer.from([sequence, total]), part);
      }
      return Buffer.concat([jpeg.subarray(0, 2), ...parts, jpeg.subarray(2)]);
    };
    const half = Math.floor(icc.length / 2);
    const first = icc.subarray(0, half);
    const second = icc.subarray(half);
    const metadata = {kind:'piccoma', scrambled:true, seed:'example', fallback: lossless75};
    // Reverse order: sequence 2 precedes sequence 1.
    const [response, bytes] = await run(withIcc([[2, second], [1, first]], 2), metadata);
    expect(response.status === 200, `status ${response.status} ${bound(bytes.subarray(0, 40).toString('latin1'))}`);
    expect(response.headers.get('content-type') === 'image/webp', `content-type ${bound(response.headers.get('content-type'))}`);
    // Parse RIFF with explicit offsets and lengths; every chunk must lie inside the declared RIFF.
    expect(bytes.length >= 12 && bytes.toString('latin1', 0, 4) === 'RIFF' && bytes.toString('latin1', 8, 12) === 'WEBP', 'not a RIFF/WebP');
    const riffEnd = 8 + bytes.readUInt32LE(4);
    expect(riffEnd === bytes.length, `RIFF size ${riffEnd} != length ${bytes.length}`);
    const chunks = [];
    for (let offset = 12; offset < riffEnd;) {
      expect(offset + 8 <= riffEnd, `chunk header truncated at ${offset}`);
      const length = bytes.readUInt32LE(offset + 4);
      const end = offset + 8 + length + (length & 1);
      expect(end <= riffEnd, `chunk at ${offset} overruns RIFF`);
      chunks.push({type: bytes.toString('latin1', offset, offset + 4), offset, length, data: bytes.subarray(offset + 8, offset + 8 + length)});
      offset = end;
    }
    const iccp = chunks.filter((chunk) => chunk.type === 'ICCP');
    expect(iccp.length === 1, `ICCP chunk count ${iccp.length}`);
    expect(iccp[0].length === icc.length && iccp[0].data.equals(icc), 'ICCP bytes differ from original profile');
    const decoded = await sharp(bytes).metadata();
    expect(decoded.format === 'webp' && decoded.width > 0 && decoded.height > 0, 'WebP metadata invalid');
    const {info} = await sharp(bytes).raw().toBuffer({resolveWithObject: true});
    expect(info.width === decoded.width && info.height === decoded.height, `decoded dimensions ${info.width}x${info.height}`);
    // Invalid ICC segment sets must be held unsupported, never converted.
    for (const [name, image] of [
      ['duplicate sequence', withIcc([[1, first], [1, second]], 2)],
      ['incomplete', withIcc([[1, first]], 2)],
    ]) {
      const [held, body] = await run(image, metadata);
      expect(held.status === 422, `${name}: status ${held.status} ${bound(body.subarray(0, 40).toString('latin1'))}`);
      expect(held.headers.get('content-type') !== 'image/webp' && !(body.length >= 4 && body.toString('latin1', 0, 4) === 'RIFF'), `${name}: converted to WebP`);
    }
  });
  // YanMaga type-F restoration, independently constructed from the golden case in
  // crates/manga/tests/yanmaga.rs: a 328x328 gradient is split into 8x8 tiles of 41x41, each
  // placed in a 392x392 magenta canvas at (4+col*49, 4+row*49). Restoration must strip the padding.
  await check('YanMaga type-F restoration matches golden pixels', async () => {
    const size = 328, padded = 392, tiles = 8, tileSize = 41, pitch = 49, margin = 4;
    const expectedRaw = Buffer.alloc(size * size * 4);
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) Buffer.from([x % 256, y % 256, (x + y) % 256, 255]).copy(expectedRaw, (y * size + x) * 4);
    }
    const paddedRaw = Buffer.alloc(padded * padded * 4);
    for (let offset = 0; offset < paddedRaw.length; offset += 4) paddedRaw.set([255, 0, 255, 255], offset);
    for (let row = 0; row < tiles; row++) {
      for (let col = 0; col < tiles; col++) {
        for (let y = 0; y < tileSize; y++) {
          for (let x = 0; x < tileSize; x++) {
            const from = ((row * tileSize + y) * size + col * tileSize + x) * 4;
            const to = ((margin + row * pitch + y) * padded + margin + col * pitch + x) * 4;
            expectedRaw.copy(paddedRaw, to, from, from + 4);
          }
        }
      }
    }
    const withPhys = await sharp(paddedRaw, {raw: {width: padded, height: padded, channels: 4}}).png({compressionLevel: 9}).toBuffer();
    const chunks = pngChunks(withPhys);
    expect(chunks.some((chunk) => chunk.type === 'pHYs'), 'sharp PNG has no pHYs chunk to strip');
    const paddedPng = Buffer.concat([
      withPhys.subarray(0, 8),
      ...chunks.filter((chunk) => chunk.type !== 'pHYs').map((chunk) => withPhys.subarray(chunk.start, chunk.end)),
    ]);
    const roundTrip = await decodeRgba(paddedPng);
    expect(roundTrip.width === padded && roundTrip.height === padded && roundTrip.data.equals(paddedRaw), 'padded PNG does not round-trip its source pixels');
    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';
    const data = `HHHHHHHHHHHHHHHH${alphabet}`;
    const source = `=8-8-4-${data}`;
    const destination = `=8-8+4-${data}`;
    const [response, bytes] = await run(paddedPng, {kind:'yanmaga', source, destination, fallback: lossless75});
    expect(response.status === 200, `status ${response.status} ${bound(bytes.subarray(0, 40).toString('latin1'))}`);
    expect(response.headers.get('content-type') === 'image/webp', `content-type ${bound(response.headers.get('content-type'))}`);
    const restored = await decodeRgba(bytes);
    expect(restored.width === size && restored.height === size, `dimensions ${restored.width}x${restored.height}`);
    let differing = 0;
    for (let offset = 0; offset < expectedRaw.length; offset += 4) {
      if (!restored.data.subarray(offset, offset + 4).equals(expectedRaw.subarray(offset, offset + 4))) differing++;
    }
    expect(differing === 0, `${differing} restored pixels differ from golden`);
    // Rotated destination suffix (as in matching_type_f_permutations_*): destination tile j takes
    // source tile (j+63)%64, so a restorer that ignores source or destination cannot pass.
    const rotatedExpected = Buffer.alloc(size * size * 4);
    for (let j = 0; j < tiles * tiles; j++) {
      const s = (j + 63) % 64;
      const dstRow = Math.floor(j / tiles), dstCol = j % tiles, srcRow = Math.floor(s / tiles), srcCol = s % tiles;
      for (let y = 0; y < tileSize; y++) {
        const from = ((srcRow * tileSize + y) * size + srcCol * tileSize) * 4;
        expectedRaw.copy(rotatedExpected, ((dstRow * tileSize + y) * size + dstCol * tileSize) * 4, from, from + tileSize * 4);
      }
    }
    const rotatedDestination = `=8-8+4-HHHHHHHHHHHHHHHH${alphabet.slice(1) + alphabet[0]}`;
    const [rotatedResponse, rotatedBytes] = await run(paddedPng, {kind:'yanmaga', source, destination: rotatedDestination, fallback: lossless75});
    expect(rotatedResponse.status === 200, `rotated: status ${rotatedResponse.status} ${bound(rotatedBytes.subarray(0, 40).toString('latin1'))}`);
    expect(rotatedResponse.headers.get('content-type') === 'image/webp', `rotated: content-type ${bound(rotatedResponse.headers.get('content-type'))}`);
    const rotated = await decodeRgba(rotatedBytes);
    expect(rotated.width === size && rotated.height === size, `rotated: dimensions ${rotated.width}x${rotated.height}`);
    let rotatedDiffering = 0;
    for (let offset = 0; offset < rotatedExpected.length; offset += 4) {
      if (!rotated.data.subarray(offset, offset + 4).equals(rotatedExpected.subarray(offset, offset + 4))) rotatedDiffering++;
    }
    expect(rotatedDiffering === 0, `rotated: ${rotatedDiffering} restored pixels differ from golden`);
    // Malformed tables (either side) must be rejected or held, never converted successfully.
    for (const [name, metadata] of [
      ['malformed destination', {kind:'yanmaga', source, destination:'invalid', fallback: lossless75}],
      ['malformed source', {kind:'yanmaga', source:'invalid', destination, fallback: lossless75}],
      ['truncated destination table', {kind:'yanmaga', source, destination: destination.slice(0, -1), fallback: lossless75}],
    ]) {
      const [bad, body] = await run(paddedPng, metadata);
      expect(bad.status === 400 || bad.status === 422, `${name}: status ${bad.status} ${bound(body.subarray(0, 40).toString('latin1'))}`);
    }
  });
  if (fallbackFailures.length) throw Error(`WebP fallback tests failed (${fallbackFailures.length}): ${fallbackFailures.join(' | ').slice(0, 1500)}`);
  console.log(JSON.stringify({giga:ok.status,bytes:output.length,webp:raw.status,xor:xor.status,fuz:fuz.status,progressive:progressiveResponse.status,forgedEoiRejected:12,malformed:bad.status,unsupported:held.status,afterErrors:again.status,concurrentRequests:120,memoryAfterWarmup,memoryFinal}));
} finally { await mf.dispose(); }
