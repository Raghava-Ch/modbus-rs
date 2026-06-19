import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // All tests run inside a real Chromium browser page.
    browser: {
      enabled: true,
      provider: 'playwright',
      name: 'chromium',
      headless: true,         // false locally for debugging
    },
    globalSetup: './tests/global-setup.ts',  // starts ws server
    testTimeout: 15_000,
    hookTimeout: 10_000,
  },
});
