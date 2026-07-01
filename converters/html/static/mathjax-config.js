// MathJax configuration acdc emits when `:stem:` is set. Loaded before the
// MathJax bundle so it is picked up at startup.
MathJax = {
  loader: {load: ['input/asciimath']},
  tex: {
    processEscapes: false,
    inlineMath: [['\\(', '\\)']],
    displayMath: [['\\[', '\\]']]
  },
  asciimath: {
    delimiters: {'[+]': [['\\$', '\\$']]},
    displaystyle: false
  },
  options: {
    ignoreHtmlClass: 'tex2jax_ignore|nostem|nolatexmath|noasciimath',
    processHtmlClass: 'tex2jax_process'
  },
  startup: {
    ready() {
      MathJax.startup.defaultReady();
      MathJax.startup.promise.then(() => {
        const asciimath = MathJax._.input.asciimath.AsciiMath;
        if (asciimath) {
          const originalCompile = asciimath.compile;
          asciimath.compile = function(math, display) {
            const node = math.math;
            if (node && node.parentElement && node.parentElement.parentElement &&
              node.parentElement.parentElement.classList.contains('stemblock')) {
              display = true;
            }
            return originalCompile.call(this, math, display);
          };
        }
      });
    }
  }
};
