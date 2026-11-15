import { mkdir, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import { preview } from 'vite';
import { chromium } from 'playwright';

// Reuse the same trusted, repository-owned smoke functions as the CLI workflow.
// Each suite gets an ephemeral context: IPC fixtures cannot leak into another.
const root = fileURLToPath(new URL('../', import.meta.url));
process.chdir(root);
const suites = [
  'ui-smoke.js', 'scroll-layout-smoke.js', 'header-theme-smoke.js',
  'active-window-smoke.js', 'stability-smoke.js', 'dom-safety-smoke.js',
];
const channel = process.env.FISHCREEN_BROWSER_CHANNEL;
if (channel && !['chrome', 'msedge'].includes(channel)) {
  throw new Error('FISHCREEN_BROWSER_CHANNEL must be chrome or msedge, or unset for bundled Chromium.');
}
await mkdir(new URL('../output/playwright/', import.meta.url), { recursive: true });
await mkdir(new URL('../.impeccable/review/', import.meta.url), { recursive: true });
let server;
let browser;
try {
  server = await preview({ root, preview: { host: '127.0.0.1', port: 0, strictPort: true, open: false } });
  const address = server.httpServer.address();
  if (!address || typeof address === 'string') throw new Error('Unable to resolve local preview port.');
  const url = `http://127.0.0.1:${address.port}/`;
  try {
    browser = await chromium.launch({ headless: true, ...(channel ? { channel } : {}) });
  } catch (error) {
    throw new Error('Chromium could not start. Run npx playwright install chromium, or set FISHCREEN_BROWSER_CHANNEL=chrome/msedge.', { cause: error });
  }
  let assertions = 0;
  for (const name of suites) {
    const source = await readFile(new URL(name, import.meta.url), 'utf8');
    const run = new vm.Script(`(${source}\n)`, { filename: name }).runInThisContext();
    if (typeof run !== 'function') throw new Error(`Invalid repository smoke function: ${name}`);
    const context = await browser.newContext({ viewport: { width: 760, height: 880 } });
    const page = await context.newPage();
    const pageErrors = [];
    page.on('pageerror', error => pageErrors.push(error.message));
    page.setDefaultTimeout(15000);
    page.setDefaultNavigationTimeout(15000);
    const timeout = setTimeout(() => void context.close().catch(() => {}), 90000);
    try {
      await page.goto(url);
      await page.locator('#version').filter({ hasText: /^\d+\.\d+\.\d+$/ }).waitFor();
      const result = await run(page);
      if (pageErrors.length) throw new Error(`${name}: ${pageErrors.join('; ')}`);
      if (!Number.isInteger(result?.count) || result.count < 1) throw new Error(`No assertions reported by ${name}`);
      assertions += result.count;
      console.log(`[ui] ${name}: ${result.count} assertions passed`);
    } finally {
      clearTimeout(timeout);
      await context.close();
    }
  }
  console.log(`[ui] ${assertions} assertions passed across ${suites.length} isolated Chromium suites`);
} finally {
  try {
    if (browser) await browser.close();
  } finally {
    if (server) {
      server.httpServer.closeAllConnections?.();
      await new Promise((resolve, reject) => server.httpServer.close(error => error ? reject(error) : resolve()));
    }
  }
}
