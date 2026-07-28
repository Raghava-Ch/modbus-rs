/// <reference types="vitest" />

import { defineConfig } from 'vitest/config';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import { playwright } from '@vitest/browser-playwright';
import path from 'path';

export default defineConfig({
  plugins: [wasm(), topLevelAwait()],
  resolve: {
    alias: {
      'modbus-rs/web': path.resolve(__dirname, './dist/index.web.js'),
      'modbus-rs': path.resolve(__dirname, './dist/index.browser.js'),
    },
  },
  test: {
    globalSetup: './__test__/wasm/tests/global-setup.ts',
    include: ['__test__/wasm/tests/**/*.spec.ts'],
    browser: {
      enabled: true,
      headless: true,
      provider: playwright(),
      instances: [
        { browser: 'chromium' }
      ],
    },
  },
  optimizeDeps: {
    exclude: ['modbus-rs', 'modbus-rs-wasm'],
  },
});
