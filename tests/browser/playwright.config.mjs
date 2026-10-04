import {defineConfig} from '@playwright/test';
import {join} from 'node:path';

export default defineConfig({
  testDir: '.', testMatch: '*.spec.mjs', timeout: 30000,
  fullyParallel: false, workers: 1, retries: process.env.CI ? 1 : 0,
  reporter: [['list'], ['html', {open: 'never'}], ['json', {outputFile: process.env.CLASHTUI_REPORT_DIR ? join(process.env.CLASHTUI_REPORT_DIR,'browser-report.json') : 'playwright-report/results.json'}]],
  use: {browserName: 'chromium', serviceWorkers: 'block', trace: 'retain-on-failure', screenshot: 'only-on-failure',
    launchOptions: process.env.CLASHTUI_CHROMIUM_PATH ? {executablePath: process.env.CLASHTUI_CHROMIUM_PATH} : {}},
});
