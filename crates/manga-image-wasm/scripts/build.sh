#!/bin/sh
set -eu
: "${WASI_SYSROOT:?Set WASI_SYSROOT to the wasi-libc sysroot directory}"
: "${WASM_CLANG:?Set WASM_CLANG to a clang with wasm32 and wasm SjLj support}"
test -f "$WASI_SYSROOT/lib/wasm32-wasip1/libc.a"
test -f "$WASI_SYSROOT/lib/wasm32-wasip1/libsetjmp.a"
export CC_wasm32_unknown_unknown="$WASM_CLANG"
export CFLAGS_wasm32_unknown_unknown="--target=wasm32-wasip1 --sysroot=$WASI_SYSROOT -mllvm -wasm-enable-sjlj"
export RUSTFLAGS="${RUSTFLAGS:-} -L native=$WASI_SYSROOT/lib/wasm32-wasip1 -l static=c -l static=setjmp"
cargo build --locked -p manga-image-wasm --target wasm32-unknown-unknown --release
