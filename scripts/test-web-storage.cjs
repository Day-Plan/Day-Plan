// Copyright © The Daybrite Project
// SPDX-License-Identifier: GPL-2.0-or-later
// Exercise the production web bundle on the same headerless subpath as GitHub Pages.
// The task and note below are synthetic fixtures in a disposable browser profile.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const playwright = require(require.resolve('playwright', {
  paths: [process.env.DAY_WEB_DRIVER_PLAYWRIGHT || process.cwd()],
}));

const root = path.resolve(process.argv[2]);
const browser = playwright[process.argv[3] || 'chromium'];
const base = '/Day-Plan/webapp/';
const types = {
  '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm',
  '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml',
  '.webmanifest': 'application/manifest+json',
};

async function main() {
  const profile = await fs.mkdtemp(path.join(os.tmpdir(), 'day-plan-web-'));
  const server = http.createServer(async (req, res) => {
    const pathname = new URL(req.url, 'http://localhost').pathname;
    const file = path.resolve(root, pathname.slice(base.length) || 'index.html');
    if (!pathname.startsWith(base) || !file.startsWith(root + path.sep)) {
      res.writeHead(404).end();
      return;
    }
    try {
      const data = await fs.readFile(file);
      res.writeHead(200, {'Content-Type': types[path.extname(file)] || 'application/octet-stream'}).end(data);
    } catch { res.writeHead(404).end(); }
  });
  let context;
  try {
    await new Promise((resolve, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolve));
    const url = `http://127.0.0.1:${server.address().port}${base}?locale=en`;
    const task = 'Static hosting fixture';
    const note = 'This note survives an offline restart.';
    for (const firstVisit of [true, false]) {
      context = await browser.launchPersistentContext(profile, {viewport: {width: 1280, height: 800}});
      if (!firstVisit) await context.setOffline(true);
      const page = context.pages()[0] || await context.newPage();
      await page.goto(url);
      await page.locator('#quick_capture').waitFor({timeout: 60000});
      assert.equal(new URL(page.url()).search, '?locale=en');
      assert.equal(await page.evaluate(() => crossOriginIsolated), true);
      if (firstVisit) {
        await page.locator('#quick_capture').fill(task);
        await page.locator('#quick_capture').press('Enter');
        await page.locator('#item_body').fill(note);
        await page.locator('#item_body').blur();
        // OPFS gives one tab exclusive access. Retry must boot a new SQLite worker.
        const other = await context.newPage();
        await other.goto(url);
        const retry = other.locator('#retry_storage');
        const capture = other.locator('#quick_capture');
        await retry.or(capture).waitFor({timeout: 45000});
        await page.close();
        if (await retry.isVisible()) await retry.click();
        await capture.waitFor({timeout: 60000});
        await other.getByRole('option').filter({hasText: task}).click();
        assert.equal(await other.locator('#item_body').inputValue(), note);
      } else {
        await page.getByRole('option').filter({hasText: task}).click();
        assert.equal(await page.locator('#item_body').inputValue(), note);
      }
      await context.close();
      context = undefined;
    }
  } finally {
    if (context) await context.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    await fs.rm(profile, {recursive: true, force: true});
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
