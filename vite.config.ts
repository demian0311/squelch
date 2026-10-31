import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  root: 'ui',
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5174, strictPort: true },
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022' },
  test: { root: '.', include: ['ui/src/**/*.test.ts'] },
});
