// Each player owns its clock and reconstructed task state. Only escaped screen
// rows use innerHTML; recorded labels are always assigned through textContent.
(function () {
  function upperBound(items, value, get) {
    var lo = 0, hi = items.length;
    while (lo < hi) {
      var mid = Math.floor((lo + hi) / 2);
      if (get(items[mid]) <= value) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }
  function elapsed(ms) { return (ms / 1000).toFixed(1) + ' s'; }
  function text(parent, tag, value, cls) {
    var node = document.createElement(tag);
    if (cls) node.className = cls;
    node.textContent = value; parent.appendChild(node);
    return node;
  }
  function taskState(r) {
    if (r.state === 'error') return 'Reported error';
    if (r.state === 'blocked') {
      return {permission: 'Permission requested', auth: 'Authentication requested', question: 'Question'}[r.kind] || 'Input requested';
    }
    return {working: 'Working', done: 'Done', idle: 'Idle'}[r.state] || r.state;
  }
  function init(el) {
    var dataEl = el.querySelector('script.terminal-view__data'), d;
    if (!dataEl) return;
    try { d = JSON.parse(dataEl.textContent); } catch (e) { return; }
    var stream = el.querySelector('.terminal-view__stream');
    var slider = el.querySelector('.terminal-view__seek input');
    var playButton = el.querySelector('[data-replay-play]');
    if (!stream || !slider || !playButton || !d.frames.length) return;
    var a = d.activity, duration = d.durationMs, position = duration;
    var running = false, raf = null, origin = null, originAt = 0;
    var lastFrame = null, taskEnd = -1, records = new Map();
    var clock = el.querySelector('.terminal-view__clock');
    var announcement = el.querySelector('.terminal-view__announcement');
    var filter = el.querySelector('.terminal-view__events select');
    var taskLists = new Map(), stepNodes = new Map();
    el.querySelectorAll('[data-task-scope]').forEach(function (node) { taskLists.set(node.dataset.taskScope, node); });
    el.querySelectorAll('.terminal-view__step').forEach(function (node) { stepNodes.set(node.dataset.scope, node); });
    while (stream.children.length < d.rows) text(stream, 'div', '', 'terminal-view__row');
    var slots = stream.querySelectorAll('.terminal-view__row');
    function screen(t) {
      var frame = upperBound(d.times, t, function (x) { return x; }) - 1;
      if (frame === lastFrame) return;
      lastFrame = frame;
      var indices = frame < 0 ? [] : d.frames[frame];
      for (var row = 0; row < slots.length; row++) {
        var html = row < indices.length ? d.pool[indices[row]] : '';
        if (slots[row].__h !== html) { slots[row].innerHTML = html; slots[row].__h = html; }
      }
    }
    function taskRow(change) {
      var r = change.record, li = document.createElement('li');
      li.dataset.depth = Math.min(4, (r.id || '').split('/').length - 1);
      var button = text(li, 'button', r.label);
      button.type = 'button'; button.dataset.seek = change.at; button.dataset.taskKey = change.key;
      var badge = text(li, 'span', taskState(r), 'terminal-view__badge');
      badge.dataset.state = r.state;
      if (r.id !== null) text(li, 'span', r.id, 'terminal-view__task-id');
      if (r.app !== null) text(li, 'span', r.app, 'terminal-view__task-app');
      if (r.progress !== null) {
        var progress = text(li, 'progress', r.progress + '%');
        progress.setAttribute('aria-label', 'Task progress');
        progress.max = 100; progress.value = r.progress;
        text(li, 'span', r.progress + '%', 'terminal-view__progress-value');
      }
      if (r.message !== null) text(li, 'div', r.message, 'terminal-view__task-message');
      return li;
    }
    function tasks(t) {
      var end = upperBound(a.changes, t, function (change) { return change.at; });
      if (end === taskEnd) return;
      var dirty = new Set(), next = Math.max(0, taskEnd);
      if (taskEnd < 0 || end < taskEnd) {
        records.clear(); next = 0;
        taskLists.forEach(function (_, scope) { dirty.add(scope); });
        var cp = upperBound(a.checkpoints, end, function (checkpoint) { return checkpoint.next; }) - 1;
        if (cp >= 0) {
          next = a.checkpoints[cp].next;
          a.checkpoints[cp].indices.forEach(function (index) {
            var change = a.changes[index]; records.set(change.key, change);
          });
        }
      }
      for (var i = next; i < end; i++) {
        var change = a.changes[i], previous = records.get(change.key);
        if (previous) dirty.add(previous.record.scope);
        if (change.record) { records.set(change.key, change); dirty.add(change.record.scope); }
        else records.delete(change.key);
      }
      taskEnd = end;
      dirty.forEach(function (scope) {
        var list = taskLists.get(scope);
        if (!list) return;
        var entries = [];
        records.forEach(function (change) { if (change.record.scope === scope) entries.push(change); });
        entries.sort(function (left, right) {
          var x = left.record.id, y = right.record.id;
          return x === y ? 0 : x === null ? -1 : y === null ? 1 : x < y ? -1 : 1;
        });
        list.replaceChildren.apply(list, entries.map(taskRow));
        if (!entries.length) text(list, 'li', 'No reported tasks at this position', 'terminal-view__empty');
      });
    }
    function steps(t) {
      a.steps.forEach(function (step) {
        var node = stepNodes.get(step.scope);
        if (!node) return;
        var finished = step.finish && step.finish.at <= t;
        var state = finished ? step.state : step.start && step.start.at <= t ? 'running' : 'pending';
        if (node.dataset.state === state) return;
        node.dataset.state = state;
        var badge = node.querySelector('.terminal-view__badge');
        badge.textContent = state; badge.dataset.state = state;
        node.querySelector('.terminal-view__step-detail').textContent = finished ? step.detail : state === 'running' && !step.finish ? 'Outcome not captured' : '';
        node.querySelector('.terminal-view__step-duration').textContent = finished && step.elapsedMs !== null ? elapsed(step.elapsedMs) + ' recorded' : '';
      });
    }
    function stepTimes() { return a.steps.map(function (s) { return s.start || s.finish; }).filter(Boolean); }
    function eventTimes() { return a.events.filter(function (event) { return !filter || filter.value === 'all' || filter.value === event.category; }); }
    function neighbour(entries, direction) {
      if (direction > 0) return entries.find(function (entry) { return entry.at > position; });
      for (var i = entries.length - 1; i >= 0; i--) if (entries[i].at < position) return entries[i];
      return null;
    }
    function navigation() {
      [['step', stepTimes()], ['event', eventTimes()]].forEach(function (group) {
        ['prev', 'next'].forEach(function (name) {
          var button = el.querySelector('[data-replay-' + name + '-' + group[0] + ']');
          if (button) button.disabled = !neighbour(group[1], name === 'next' ? 1 : -1);
        });
      });
    }
    function draw(t) {
      position = Math.max(0, Math.min(duration, t));
      screen(position); tasks(position); steps(position);
      slider.value = position;
      slider.setAttribute('aria-valuetext', elapsed(position) + ' of ' + elapsed(duration));
      clock.textContent = elapsed(position) + ' / ' + elapsed(duration);
      navigation();
    }
    function announce(value) { announcement.textContent = value; }
    function pause(silent) {
      running = false;
      if (raf !== null) cancelAnimationFrame(raf);
      raf = null; playButton.textContent = 'Play'; el.dataset.playing = 'false';
      if (!silent) announce('Paused at ' + elapsed(position));
    }
    function seek(at, label) {
      pause(true); draw(at);
      announce((label ? label + '. ' : '') + 'Playback position ' + elapsed(position));
    }
    function tick(ts) {
      if (!running) return;
      if (origin === null) origin = ts;
      draw(originAt + ts - origin);
      if (position < duration) raf = requestAnimationFrame(tick);
      else { pause(true); announce('Replay complete'); }
    }
    function play() {
      if (position >= duration) draw(0);
      running = true; origin = null; originAt = position;
      playButton.textContent = 'Pause'; el.dataset.playing = 'true';
      raf = requestAnimationFrame(tick);
    }
    el.querySelectorAll('button[data-seek], .terminal-view__controls button, .terminal-view__seek input, .terminal-view__events select').forEach(function (node) { node.disabled = false; });
    playButton.addEventListener('click', function () { if (running) pause(false); else play(); });
    slider.addEventListener('input', function () { pause(true); draw(Number(slider.value)); });
    slider.addEventListener('change', function () { announce('Playback position ' + elapsed(position)); });
    el.addEventListener('click', function (event) {
      var button = event.target.closest('button[data-seek]');
      if (!button || !el.contains(button)) return;
      var taskKey = button.dataset.taskKey;
      seek(Number(button.dataset.seek), button.textContent);
      if (taskKey !== undefined) {
        var replacement = el.querySelector('button[data-task-key="' + taskKey + '"]');
        if (replacement) replacement.focus();
      }
    });
    [['step', stepTimes], ['event', eventTimes]].forEach(function (group) {
      ['prev', 'next'].forEach(function (name) {
        var button = el.querySelector('[data-replay-' + name + '-' + group[0] + ']');
        if (button) button.addEventListener('click', function () {
          var target = neighbour(group[1](), name === 'next' ? 1 : -1);
          if (target) seek(target.at, target.label || (name === 'next' ? 'Next step' : 'Previous step'));
        });
      });
    });
    if (filter) filter.addEventListener('change', function () {
      el.querySelectorAll('.terminal-view__events li[data-category]').forEach(function (li) { li.hidden = filter.value !== 'all' && li.dataset.category !== filter.value; });
      navigation();
    });
    document.addEventListener('visibilitychange', function () { if (document.hidden && running) pause(true); });
    var motion = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)');
    if (motion && motion.addEventListener) motion.addEventListener('change', function () { if (motion.matches) pause(true); });
    el.setAttribute('data-acdc-ready', '1');
    draw(duration); pause(true);
    if (!motion || !motion.matches) play();
  }
  window.__acdcReplayInit = function () {
    document.querySelectorAll('.terminal-view--replay:not([data-acdc-ready])').forEach(init);
  };
  window.__acdcReplayInit();
})();
