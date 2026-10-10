import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp, readFile, rm} from 'node:fs/promises';
import {createServer} from 'node:http';
import {tmpdir} from 'node:os';
import {dirname, join, resolve, sep} from 'node:path';
import {fileURLToPath} from 'node:url';

const crate = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const site = join(crate, 'www');
const profile = await mkdtemp(join(tmpdir(), 'acdc-preview-browser-'));
const blockedRequests = [];
const server = createServer(async (request, response) => {
  const pathname = new URL(request.url, 'http://localhost').pathname;
  if (pathname.startsWith('/should-not-load')) blockedRequests.push(pathname);
  const routes = {
    '/browser-tests.html': join(crate, 'tests/browser.html'),
    '/browser-tests.js': join(crate, 'tests/browser.js'),
    '/acdc_editor_wasm.js': join(crate, 'pkg/acdc_editor_wasm.js'),
    '/acdc_editor_wasm_bg.wasm': join(crate, 'pkg/acdc_editor_wasm_bg.wasm'),
    '/trusted-host.html': join(site, 'index.html'),
  };
  const path = routes[pathname] ?? resolve(site, `.${pathname}`);
  if (!routes[pathname] && !path.startsWith(site + sep)) {
    response.writeHead(403).end();
    return;
  }
  try {
    let body;
    body = await readFile(path);
    if (pathname === '/index.html' || pathname === '/trusted-host.html') {
      // Exercise optional copy support without changing the example page's controls.
      body = body.toString().replace('<footer class="site-footer">', '<button id="btn-copy">Copy HTML</button><footer class="site-footer">');
      body = body.replace(/<script data-goatcounter=.*?<\/script>/, '');
      // Keep the existing math hook offline without loading MathJax from its CDN.
      body = body.replace('         await init();', `
         MathJax.typesetClear = () => {};
         MathJax.typesetPromise = elements => {
           window.previewMathCalls = (window.previewMathCalls || 0) + 1;
           window.previewMathTarget = elements[0].id;
           return Promise.resolve();
         };
         await init();`);
      const install = 'window.sanitizePreviewHtml = sanitizePreviewHtml;';
      body = body.replace(install, pathname === '/trusted-host.html' ? '' : `
         window.sanitizePreviewHtml = html => {
           if (window.failPreviewSanitizer) throw new Error('Test sanitizer failure');
           return sanitizePreviewHtml(html);
         };`);
    }
    const type = pathname.endsWith('.wasm') ? 'application/wasm'
      : /\.(js|mjs)$/.test(pathname) ? 'text/javascript'
      : pathname.endsWith('.css') ? 'text/css'
      : pathname.endsWith('.png') ? 'image/png' : 'text/html';
    response.writeHead(200, {'Content-Type': type, 'Cache-Control': 'no-store'}).end(body);
  } catch {
    response.writeHead(404).end('Not found');
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const url = `http://127.0.0.1:${server.address().port}/browser-tests.html`;
const chromeBinary = process.env.CHROME_BIN ?? (process.platform === 'darwin'
  ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : 'google-chrome');
const chrome = spawn(chromeBinary, [
  '--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
  '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
], {stdio: ['ignore', 'ignore', 'pipe']});
let chromeErrors = '';
chrome.stderr.on('data', chunk => { chromeErrors = (chromeErrors + chunk).slice(-12000); });
let launchError;
chrome.on('error', error => { launchError = error; });
let socket;

try {
  const deadline = Date.now() + 60000;
  let port;
  while (!port) {
    if (launchError) throw launchError;
    assert(Date.now() < deadline, `Chrome did not start.\n${chromeErrors}`);
    try {
      port = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0];
    } catch {
      await new Promise(resolve => setTimeout(resolve, 50));
    }
  }
  const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const page = pages.find(page => page.type === 'page');
  assert(page, 'The browser test page was not opened');
  socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, {once: true});
    socket.addEventListener('error', reject, {once: true});
  });
  let nextId = 0;
  const pending = new Map();
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    const callback = pending.get(message.id);
    if (callback) {
      pending.delete(message.id);
      callback(message);
    }
  });
  const call = (method, params) => new Promise((resolve, reject) => {
    const id = ++nextId;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`Chrome command timed out: ${method}`));
    }, 5000);
    pending.set(id, message => {
      clearTimeout(timer);
      if (message.error) reject(new Error(JSON.stringify(message.error)));
      else resolve(message.result);
    });
    socket.send(JSON.stringify({id, method, params}));
  });
  const mockImage = '<svg xmlns="http://www.w3.org/2000/svg" width="512" height="600"><rect width="512" height="600" fill="black"/></svg>';
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.method !== 'Fetch.requestPaused') return;
    const {requestId, request} = message.params;
    const resource = new URL(request.url);
    const image = resource.href === 'https://upload.wikimedia.org/wikipedia/commons/3/35/Tux.svg';
    const action = image
      ? call('Fetch.fulfillRequest', {requestId, responseCode: 200,
        responseHeaders: [{name: 'Content-Type', value: 'image/svg+xml'}],
        body: Buffer.from(mockImage).toString('base64')})
      : call('Fetch.failRequest', {requestId, errorReason: 'BlockedByClient'});
    action.catch(error => { chromeErrors += `\n${error.message}`; });
  });
  // Check image rendering without contacting external services.
  await call('Fetch.enable', {patterns: [{urlPattern: 'https://*'}]});
  await call('Page.navigate', {url});
  let result;
  while (!result?.done) {
    assert(Date.now() < deadline, `Browser tests timed out.\n${chromeErrors}`);
    const evaluation = await call('Runtime.evaluate', {expression: 'window.browserTestResult', returnByValue: true});
    result = evaluation.result.value;
    if (!result?.done) await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert(result.passed, `${result.error}\nCompleted: ${result.checks.join('; ')}\n${chromeErrors}`);
  assert.deepEqual(blockedRequests, [], 'Blocked content made network requests');
  for (const check of result.checks) console.log(`PASS ${check}`);
  console.log('PASS Removed content made no local requests');
} finally {
  socket?.close();
  chrome.kill();
  await new Promise(resolve => {
    if (chrome.exitCode !== null || chrome.signalCode !== null || launchError) resolve();
    else chrome.once('exit', resolve);
  });
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  await rm(profile, {recursive: true, force: true});
}
