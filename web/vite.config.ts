import { defineConfig } from 'vite';
// Cross-origin isolation enables SharedArrayBuffer, which the parallel line-recognition pool
// needs. Without it the app still works (single-threaded recognition fallback).
const isolation = { 'Cross-Origin-Opener-Policy': 'same-origin', 'Cross-Origin-Embedder-Policy': 'require-corp' };
export default defineConfig({
  base: process.env.BASE_PATH || '/DeadlockCoach/',
  worker: { format: 'es' },
  build: { target: 'es2022', assetsInlineLimit: 0 },
  server: { port: 5173, strictPort: true, headers: isolation },
  preview: { port: 4173, strictPort: true, headers: isolation },
});
