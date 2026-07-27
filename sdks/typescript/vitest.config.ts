import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    // The live integration suites (client / multi-database / external-id)
    // all drive one shared server + default database, so running the test
    // files in parallel lets them clobber each other's data. Serialise
    // file execution; within a file vitest already runs sequentially.
    fileParallelism: false,
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json', 'html'],
      exclude: ['**/*.test.ts', '**/examples/**', '**/dist/**'],
    },
  },
});

