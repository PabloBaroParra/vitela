import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

// Same test setup the editor had inside apps/web (vite.config.ts `test`).
export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    restoreMocks: true,
    unstubGlobals: true,
  },
});
