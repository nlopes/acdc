import DOMPurify from './vendor/purify.es.mjs';

const purifier = DOMPurify(window);
const idPrefix = 'user-content-';
// Keep source highlighting and table sizing without letting CSS cover the editor.
const styleProperties = new Set([
  'color', 'background-color', 'font-weight', 'font-style', 'font-family', 'font-size',
  'text-decoration', 'text-align', 'vertical-align', 'white-space', 'width', 'height',
]);
let adjusted;

function isPlayerUrl(source) {
  let url;
  try {
    url = new URL(source);
  } catch {
    return false;
  }
  if (url.username || url.password || url.port || url.protocol !== 'https:') return false;
  return (['www.youtube.com', 'www.youtube-nocookie.com'].includes(url.hostname)
    && /^\/embed\/(?:[A-Za-z0-9_-]{11}|videoseries)$/.test(url.pathname))
    || (url.hostname === 'player.vimeo.com' && /^\/video\/[0-9]+$/.test(url.pathname));
}

purifier.addHook('uponSanitizeElement', node => {
  if (node.localName === 'iframe'
    && (!isPlayerUrl(node.getAttribute('src')) || node.hasAttribute('srcdoc'))) {
    node.remove();
    adjusted = true;
  }
});

purifier.addHook('uponSanitizeAttribute', (node, attribute) => {
  if (attribute.attrName === 'href' && attribute.attrValue.startsWith('#')
    && !attribute.attrValue.startsWith('#' + idPrefix)) {
    attribute.attrValue = '#' + idPrefix + attribute.attrValue.slice(1);
  }
  if (attribute.attrName !== 'style') return;
  const source = document.createElement('span').style;
  const clean = document.createElement('span').style;
  source.cssText = attribute.attrValue;
  for (const property of source) {
    if (styleProperties.has(property)) clean.setProperty(property, source.getPropertyValue(property));
    else adjusted = true;
  }
  attribute.attrValue = clean.cssText;
});

purifier.addHook('afterSanitizeAttributes', node => {
  if (node.localName !== 'iframe') return;
  node.setAttribute('sandbox', 'allow-scripts allow-same-origin allow-presentation');
  node.setAttribute('allow', 'autoplay; encrypted-media; fullscreen; picture-in-picture');
  // YouTube requires a Referer. Send the origin without the editor's path or query.
  node.setAttribute('referrerpolicy', 'strict-origin-when-cross-origin');
});

export function sanitizePreviewHtml(html) {
  if (!purifier.isSupported) throw new Error('This browser does not support the preview sanitizer.');
  adjusted = false;
  const clean = purifier.sanitize(html, {
    ADD_TAGS: ['iframe'],
    ADD_ATTR: ['allowfullscreen'],
    FORBID_TAGS: ['style', 'form'],
    // Document IDs must not replace editor controls or named window properties.
    SANITIZE_NAMED_PROPS: true,
  });
  return [clean, adjusted || purifier.removed.length > 0];
}
