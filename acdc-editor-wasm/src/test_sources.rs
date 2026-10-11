use std::{collections::HashMap, error::Error, path::PathBuf, sync::Arc};

use acdc_parser::{
    IncludeLoader, IncludeSource, IncludeSourceError, IncludeSourceErrorKind,
    IncludeSourceProvider, IncludeSourceTarget, Options, SafeMode,
};

struct Sources(HashMap<PathBuf, Vec<u8>>);

impl IncludeSourceProvider for Sources {
    fn open(&self, target: &IncludeSourceTarget) -> Result<IncludeSource, IncludeSourceError> {
        if let IncludeSourceTarget::File(path) = target
            && let Some(bytes) = self.0.get(path)
        {
            return Ok(IncludeSource::from_bytes(bytes.clone()));
        }
        Err(IncludeSourceError::new(
            IncludeSourceErrorKind::NotFound,
            "unknown test source",
        ))
    }
}

pub(super) fn options(sources: &[(&str, &str)]) -> Result<Options<'static>, Box<dyn Error>> {
    let base = std::env::temp_dir().canonicalize()?;
    let files = sources
        .iter()
        .map(|(name, text)| (base.join(name), text.as_bytes().to_vec()))
        .collect();
    Ok(Options::builder()
        .with_safe_mode(SafeMode::Server)
        .with_base_dir(base)
        .with_include_loader(IncludeLoader::Custom(Arc::new(Sources(files))))
        .build()?)
}
