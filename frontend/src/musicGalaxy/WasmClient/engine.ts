// Shim module that exports stub functions when WASM engine is not built
// This file is committed to the repo and allows builds to succeed without the WASM module
// When the WASM module is built, this file is replaced by the generated engine.js

export * from './engine.stub';
