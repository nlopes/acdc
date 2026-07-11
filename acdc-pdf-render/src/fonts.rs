use std::{path::Path, sync::OnceLock};

use typst::{foundations::Bytes, text::Font};

use crate::error::Error;

/// Font file extensions Typst can read (sfnt-based; woff2 is not supported).
const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc"];

/// Parsed bundled fonts are process-wide. `Font` clones share their parsed
/// representation and `Bytes::new` can directly retain a static asset slice, so
/// subsequent renders neither copy nor reparse the 27 MB bundle.
static BUNDLED_FONTS: OnceLock<Vec<Font>> = OnceLock::new();

/// Collect all fonts to register with the engine: every font found in the
/// caller-supplied directories, followed by the bundled fallback fonts.
///
/// Directory fonts come first so a brand face supplied at runtime is available
/// alongside the bundled fallback; family resolution (which face actually wins)
/// is driven by the font stacks the emitter writes into the markup.
pub(crate) fn load(font_dirs: &[impl AsRef<Path>]) -> Result<Vec<Font>, Error> {
    let mut fonts = Vec::new();
    for dir in font_dirs {
        collect_dir(dir.as_ref(), &mut fonts)?;
    }
    fonts.extend_from_slice(bundled());
    Ok(fonts)
}

fn bundled() -> &'static [Font] {
    BUNDLED_FONTS.get_or_init(|| {
        acdc_pdf_theme::embedded_fonts()
            .iter()
            .flat_map(|bytes| Font::iter(Bytes::new(*bytes)))
            .collect()
    })
}

fn collect_dir(dir: &Path, fonts: &mut Vec<Font>) -> Result<(), Error> {
    // The explicitly configured root may itself be a symlink, but directory
    // entries use `file_type`, which does not follow symlinks. This keeps a
    // symlink cycle inside the tree from becoming an unbounded walk.
    let mut pending = vec![(dir.to_path_buf(), true)];
    while let Some((path, is_dir)) = pending.pop() {
        if is_dir {
            let entries = std::fs::read_dir(&path).map_err(|source| font_error(&path, source))?;
            let mut entries = entries
                .map(|entry| {
                    let entry = entry.map_err(|source| font_error(&path, source))?;
                    let file_type = entry
                        .file_type()
                        .map_err(|source| font_error(&entry.path(), source))?;
                    Ok((entry.path(), file_type.is_dir()))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            pending.extend(entries.into_iter().rev());
            continue;
        }
        if is_font_file(&path) {
            let bytes = std::fs::read(&path).map_err(|source| font_error(&path, source))?;
            fonts.extend(Font::iter(Bytes::new(bytes)));
        }
    }
    Ok(())
}

fn font_error(path: &Path, source: std::io::Error) -> Error {
    Error::FontDir {
        path: path.display().to_string(),
        source,
    }
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|ext| FONT_EXTENSIONS.contains(&ext.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn embedded_font(index: usize) -> Result<&'static [u8], std::io::Error> {
        acdc_pdf_theme::embedded_fonts()
            .get(index)
            .copied()
            .ok_or_else(|| std::io::Error::other(format!("missing embedded font {index}")))
    }

    #[test]
    fn bundled_fonts_reuse_static_bytes_and_parsed_faces() -> Result<(), Error> {
        let first = load(&[] as &[&Path])?;
        let second = load(&[] as &[&Path])?;

        assert_eq!(first.len(), acdc_pdf_theme::embedded_fonts().len());
        assert_eq!(second.len(), first.len());
        for ((first_font, second_font), bytes) in first
            .iter()
            .zip(second.iter())
            .zip(acdc_pdf_theme::embedded_fonts())
        {
            assert_eq!(first_font.data().as_ptr(), bytes.as_ptr());
            assert_eq!(second_font.data().as_ptr(), first_font.data().as_ptr());
        }
        Ok(())
    }

    #[test]
    fn custom_fonts_load_in_stable_path_order() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let nested = dir.path().join("a");
        std::fs::create_dir(&nested)?;
        std::fs::write(nested.join("sans.ttf"), embedded_font(0)?)?;
        std::fs::write(dir.path().join("z-serif.ttf"), embedded_font(5)?)?;

        let fonts = load(&[dir.path()])?;
        let [first, second, ..] = fonts.as_slice() else {
            return Err(std::io::Error::other("expected two custom fonts").into());
        };

        assert_eq!(first.info().family, "IBM Plex Sans");
        assert_eq!(second.info().family, "IBM Plex Serif");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn directory_symlink_cycles_are_not_followed() -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("font.ttf"), embedded_font(0)?)?;
        symlink(dir.path(), dir.path().join("cycle"))?;

        let fonts = load(&[dir.path()])?;

        assert_eq!(fonts.len(), acdc_pdf_theme::embedded_fonts().len() + 1);
        Ok(())
    }
}
