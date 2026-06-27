/// <reference types="vitest" />

import { defineConfig } from 'vitest/config';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import { playwright } from '@vitest/browser-playwright';

export default defineConfig({
  plugins: [wasm(), topLevelAwait()],
  test: {
    globalSetup: './tests/global-setup.ts',
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
    exclude: ['modbus-rs-wasm'],
  },
});