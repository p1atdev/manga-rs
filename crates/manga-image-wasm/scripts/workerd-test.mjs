import { createRequire } from 'node:module';
import { readFile, writeFile } from 'node:fs/promises';
const require = createRequire(import.meta.url);
const { Miniflare } = process.env.MINIFLARE_ROOT ? require(`${process.env.MINIFLARE_ROOT}/miniflare`) : require('miniflare');
const input = await readFile('crates/manga/tests/assets/giga-tonari-yj-public-20261111.jpg');
const webp = await readFile('crates/manga/tests/assets/kadocomi-decrypted.webp');
const encryptedWebp = await readFile('crates/manga/tests/assets/kadocomi-encrypted.webp');
const encryptedJpeg = await readFile('crates/manga/tests/assets/fuz-encrypted.jpeg');
const progressive = await readFile('crates/manga/tests/assets/giga-public-progressive.jpg');
const wasm = await readFile('target/wasm32-unknown-unknown/release/manga_image_wasm.wasm');
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
  console.log(JSON.stringify({giga:ok.status,bytes:output.length,webp:raw.status,xor:xor.status,fuz:fuz.status,progressive:progressiveResponse.status,forgedEoiRejected:12,malformed:bad.status,unsupported:held.status,afterErrors:again.status,concurrentRequests:120,memoryAfterWarmup,memoryFinal}));
} finally { await mf.dispose(); }
