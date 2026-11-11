/** Instantiate once per isolate. Network acquisition stays with the caller. */
export function createImageProcessor(module) {
  const imports = {};
  for (const entry of WebAssembly.Module.imports(module)) {
    if (entry.module !== 'wasi_snapshot_preview1' || entry.kind !== 'function') {
      throw new Error(`Unsupported Wasm import: ${entry.module}.${entry.name}`);
    }
    (imports[entry.module] ??= {})[entry.name] = () => {
      throw new Error(`WASI call unsupported: ${entry.name}`);
    };
  }
  const instance = new WebAssembly.Instance(module, imports);
  const e = instance.exports;
  let usable = true;
  function restore(bytes, metadata) {
    if (!usable) throw new Error('Wasm processor is unusable after a trap');
    if (!(bytes instanceof Uint8Array)) throw new TypeError('bytes must be Uint8Array');
    const meta = new TextEncoder().encode(JSON.stringify(metadata));
    let inputPtr = 0, metaPtr = 0;
    try {
      inputPtr = e.alloc(bytes.length);
      if (!inputPtr) throw new Error('Wasm input allocation failed');
      metaPtr = e.alloc(meta.length);
      if (!metaPtr) throw new Error('Wasm metadata allocation failed');
      new Uint8Array(e.memory.buffer, inputPtr, bytes.length).set(bytes);
      new Uint8Array(e.memory.buffer, metaPtr, meta.length).set(meta);
      const status = e.restore(inputPtr, bytes.length, metaPtr, meta.length);
      const result = new Uint8Array(e.memory.buffer, e.output_ptr(), e.output_len()).slice();
      if (status === 1) return { kind: 'unsupported', reason: new TextDecoder().decode(result) };
      if (status !== 0) {
        const error = new Error(new TextDecoder().decode(result));
        error.code = ({ 2: 'malformed', 3: 'limit_exceeded', 4: 'invalid_argument', 5: 'internal' })[status] ?? 'internal';
        throw error;
      }
      const contentType = ({ 1: 'image/jpeg', 2: 'image/png', 3: 'image/webp' })[e.output_format()];
      if (!contentType) throw new Error('Wasm returned an invalid output format');
      return { kind: 'ok', bytes: result, contentType };
    } catch (error) {
      // A trap can leave allocator or C state inconsistent. Retire this instance.
      if (error instanceof WebAssembly.RuntimeError || String(error?.message).startsWith('WASI call unsupported:')) usable = false;
      throw error;
    } finally {
      if (usable) {
        try {
          if (inputPtr) e.dealloc(inputPtr, bytes.length);
          if (metaPtr) e.dealloc(metaPtr, meta.length);
          e.clear_output();
        } catch (error) {
          usable = false;
          throw error;
        }
      }
    }
  }
  restore.memoryBytes = () => e.memory.buffer.byteLength;
  return restore;
}
