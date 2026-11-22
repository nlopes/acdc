// Terminal-replay player: a tiny, self-contained vanilla-JS player shared by
// every replay block on the page. It reads each block's JSON payload, swaps the
// visible rows on a clock (only touching rows that actually change), and honours
// `prefers-reduced-motion` by leaving the server-rendered final frame in place.
// Defining `__acdcReplayInit` is idempotent, so emitting this once per block is
// safe; every call initialises any not-yet-initialised player on the page.
(function () {
  function init(el) {
    el.setAttribute('data-acdc-ready', '1');

    var dataEl = el.querySelector('script.terminal-view__data');
    if (!dataEl) return;

    var d;
    try {
      d = JSON.parse(dataEl.textContent);
    } catch (e) {
      return;
    }

    var stream = el.querySelector('.terminal-view__stream');
    if (!stream) return;

    // Reduced motion: leave the server-rendered final frame as-is.
    if (window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

    // The server renders only the final frame; build the remaining row slots.
    while (stream.children.length < d.rows) {
      var r = document.createElement('div');
      r.className = 'terminal-view__row';
      stream.appendChild(r);
    }

    var slots = stream.querySelectorAll('.terminal-view__row');

    function show(s, on) {
      slots[s].style.display = on ? '' : 'none';
    }

    // Every row is shown during playback; at rest only the final frame's rows.
    function fill() {
      for (var s = d.finalRows; s < slots.length; s++) show(s, true);
    }
    function trim() {
      for (var s = d.finalRows; s < slots.length; s++) show(s, false);
    }

    // Render frame `f`, touching only the rows whose HTML actually changed.
    function apply(f) {
      var idx = d.frames[f];
      for (var s = 0; s < idx.length; s++) {
        var h = d.pool[idx[s]];
        if (slots[s].__h !== h) {
          slots[s].innerHTML = h;
          slots[s].__h = h;
        }
      }
    }

    fill();
    apply(0);

    var n = d.frames.length;
    var i = 0;
    var start = null;

    function step(ts) {
      if (start === null) start = ts;
      var t = ts - start;
      while (i < n - 1 && d.times[i + 1] <= t) i++;
      apply(i);
      if (i < n - 1) {
        requestAnimationFrame(step);
      } else {
        apply(n - 1);
        trim();
      }
    }

    requestAnimationFrame(step);
  }

  window.__acdcReplayInit = function () {
    var els = document.querySelectorAll('.terminal-view--replay:not([data-acdc-ready])');
    for (var i = 0; i < els.length; i++) init(els[i]);
  };
  window.__acdcReplayInit();
})();
