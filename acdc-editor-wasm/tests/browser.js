const app = document.getElementById('app');
const checks = [];

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function until(predicate, label) {
  const started = performance.now();
  while (!predicate()) {
    if (performance.now() - started > 20000) throw new Error(`Timed out: ${label}`);
    await new Promise(resolve => setTimeout(resolve, 20));
  }
}

try {
  await until(() => app.contentDocument?.getElementById('editor')?.value, 'editor initialization');
  const host = app.contentWindow;
  const doc = host.document;
  const editor = doc.getElementById('editor');
  const preview = doc.getElementById('preview');
  const originalDisplay = host.getComputedStyle(editor).display;
  const status = doc.getElementById('parse-status');

  async function render(source, marker) {
    editor.value = source;
    editor.dispatchEvent(new host.Event('input', {bubbles: true}));
    await until(() => preview.querySelector('#user-content-' + marker), 'preview ' + marker);
  }

  assert(preview.tagName === 'DIV', 'The preview must use a div');
  assert(new URL(preview.querySelector('iframe').src).pathname === '/embed/RvRhUHTV_8k', 'The default video was lost');
  assert(host.previewMathCalls > 0, 'The existing math hook was not called');

  await render(`= Normal content
:toc: macro
:stem: latexmath

toc::[]

[[destination]]
== Destination

<<destination,Jump>> and footnote:[A note.] and stem:[x^2].

pass:[<details id="normal"><summary>Details</summary>Body</details>]

[source,rust]
----
fn main() { println!("hello"); }
----

video::M7lc1UVf-VE[youtube,width=480,height=270,options="nofullscreen"]

video::76979871[vimeo]

image::https://upload.wikimedia.org/wikipedia/commons/3/35/Tux.svg[Tux,250,350]
`, 'normal');
  preview.querySelector('summary').click();
  assert(preview.querySelector('#user-content-normal').open, 'Raw details stopped working');
  assert(preview.querySelector('code span[style]'), 'Source highlighting was lost');
  assert(host.previewMathCalls > 1 && host.previewMathTarget === 'preview', 'Math missed the preview pane');
  assert(preview.textContent.includes('x^2'), 'Math source was changed');
  const players = [...preview.querySelectorAll('iframe')];
  assert(players.length === 2, 'A supported video was lost');
  const youtube = players.find(player => new URL(player.src).hostname === 'www.youtube.com');
  const vimeo = players.find(player => new URL(player.src).hostname === 'player.vimeo.com');
  assert(youtube?.width === '480' && youtube.height === '270' && !youtube.allowFullscreen && vimeo, 'Video attributes changed');
  assert(players.every(player => player.referrerPolicy === 'strict-origin-when-cross-origin'), 'Players lost their origin referrer');
  const image = preview.querySelector('img[alt="Tux"]');
  await until(() => image?.naturalWidth > 0, 'SVG image loading');
  assert(image.getAttribute('width') === '250' && image.getAttribute('height') === '350', 'Image dimensions changed');
  const links = [...preview.querySelectorAll('a[href="#user-content-destination"]')];
  assert(links.length >= 2, 'TOC or cross-reference links were lost');
  links[0].click();
  assert(host.location.hash === '#user-content-destination', 'An internal link missed its target');
  const footnote = preview.querySelector('a[role="doc-noteref"]');
  assert(footnote && preview.querySelector(footnote.getAttribute('href')), 'A footnote target was lost');
  assert(!status.textContent.includes('Some HTML'), 'Normal content reports removed HTML');
  checks.push('Details, highlighting, math hooks, provider embeds, SVG images, and internal links remain available');

  const hostile = `++++
<p id="hostile">Pasted HTML</p>
<style>#editor { display: none !important; }</style>
<input id="editor" name="MathJax">
<script>window.hostileRan = true;</script>
<img src="/missing.png" onerror="window.hostileRan = true">
<form action="/should-not-load-form"><button>Submit</button></form>
<object data="/should-not-load-object"></object>
<script src="/should-not-load-script"></script>
<iframe src="/should-not-load-frame"></iframe>
<iframe src="https://www.youtube.com.evil.invalid/embed/M7lc1UVf-VE"></iframe>
<iframe src="https://evil.invalid@www.youtube.com/embed/M7lc1UVf-VE"></iframe>
<iframe src="https://www.youtube.com/watch?v=M7lc1UVf-VE"></iframe>
<iframe src="https://www.youtube.com/embed/M7lc1UVf-VE" srcdoc="<p>Other content</p>"></iframe>
<a id="active" href="  JaVa&#10;ScRiPt:window.hostileRan=true">Active link</a>
<span style="position:fixed;inset:0;color:red">Styled text</span>
++++

Inline pass:[<button id="handler" onclick="window.hostileRan=true">Click</button>].
`;
  await render(hostile, 'hostile');
  preview.querySelector('#user-content-active').click();
  preview.querySelector('#user-content-handler').click();
  await new Promise(resolve => setTimeout(resolve, 100));
  assert(!host.hostileRan, 'Document code ran');
  assert(host.getComputedStyle(editor).display === originalDisplay, 'Document CSS hid the editor');
  assert(doc.querySelectorAll('#editor').length === 1 && host.MathJax.typesetPromise, 'Document names replaced editor controls');
  assert(!preview.querySelector('script, style, form, object, iframe, [onclick], [onerror]'), 'Active content survived');
  const styled = preview.querySelector('span[style]').style;
  assert(styled.color === 'red' && !styled.position, 'Inline style filtering failed');
  assert(status.textContent.includes('Some HTML was removed'), 'The removal warning is missing');
  checks.push('Pasted handlers, scripts, global CSS, conflicting names, and unsupported frames are removed');

  let copied;
  Object.defineProperty(host.navigator, 'clipboard', {value: {writeText: async html => { copied = html; }}});
  doc.getElementById('btn-copy').click();
  await until(() => copied !== undefined, 'Copy HTML');
  assert(copied.includes('<script>window.hostileRan') && copied.includes('<p id="hostile">'), 'Copy HTML changed converter output');
  checks.push('Copy HTML returns the original converter output');

  delete host.sanitizePreviewHtml;
  await render('pass:[<p id="cached" onclick="window.hostileRan=true">Cached sanitizer</p>]', 'cached');
  assert(!preview.querySelector('[onclick]'), 'Deleting the global disabled sanitization');
  const previous = preview.innerHTML;
  host.failPreviewSanitizer = true;
  editor.value = 'pass:[<p id="failed" onclick="window.hostileRan=true">Failed sanitizer</p>]';
  editor.dispatchEvent(new host.Event('input', {bubbles: true}));
  await until(() => status.textContent.includes('Cannot sanitize'), 'sanitizer failure');
  assert(preview.innerHTML === previous && preview.parentElement.classList.contains('preview-stale'), 'Sanitizer failure displayed raw HTML');
  host.failPreviewSanitizer = false;
  await render('pass:[<p id="clean">Clean preview</p>]', 'clean');
  assert(!preview.parentElement.classList.contains('preview-stale'), 'Recovery left the preview stale');
  checks.push('Enabled sanitization survives global changes and preserves the last preview on failure');

  app.src = '/trusted-host.html';
  await until(() => app.contentWindow.location.pathname === '/trusted-host.html'
    && app.contentDocument?.getElementById('editor')?.value, 'host without sanitizer');
  const legacy = app.contentDocument;
  const legacyEditor = legacy.getElementById('editor');
  legacyEditor.value = '= Trusted HTML\n:toc: macro\n\ntoc::[]\n\n== Section\n\npass:[<b id="legacy" onclick="window.example=true">Trusted HTML</b>]';
  legacyEditor.dispatchEvent(new app.contentWindow.Event('input', {bubbles: true}));
  await until(() => legacy.getElementById('legacy'), 'trusted HTML');
  assert(legacy.getElementById('legacy').hasAttribute('onclick'), 'A host without a sanitizer changed behavior');
  assert(app.contentWindow.getComputedStyle(legacy.getElementById('toc')).borderBottomStyle === 'solid', 'Original document IDs lost their styles');
  checks.push('Existing hosts can display trusted HTML without installing a sanitizer');
  window.browserTestResult = {done: true, passed: true, checks};
} catch (error) {
  window.browserTestResult = {done: true, passed: false, error: error.stack, checks};
}
document.getElementById('result').textContent = JSON.stringify(window.browserTestResult, null, 2);
