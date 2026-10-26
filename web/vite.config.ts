import { defineConfig } from 'vite';
export default defineConfig({
  base: process.env.BASE_PATH || '/DeadlockCoach/',
  worker: { format: 'es' },
  build: { target: 'es2022', assetsInlineLimit: 0 },
  server: { port: 5173, strictPort: true },
  preview: { port: 4173, strictPort: true },
});
