# Changelog

## [Unreleased]

- Terminal previews declare their replay script hash at registration and report safe-mode fallback before writing any output.

- Setting or unsetting `acdc-terminal` in the body now controls subsequent
  source previews. Dimensions and theme also follow the current block’s scope.

- Extracted terminal previews and recorded-session replay into the native
  `acdc-extensions-terminal` extension. Select `--extension terminal` with the CLI's
  `html-terminal` build feature, or register and select the package through the
  library pipeline. Existing syntax, dimensions, themes, captions, replay formats,
  safe-mode fallback, and structured warnings are preserved. Asciidoctor continues
  to render these styles as raw listings or literal blocks.
