//! Non-fatal converter diagnostics.
//!
//! Converter warnings are backend-owned messages about conditions a converter
//! recovered from while producing output. Core intentionally keeps the payload
//! generic so backend-specific warning categories stay in backend crates.

use std::{borrow::Cow, cell::RefCell, fmt, rc::Rc};

use acdc_parser::SourceLocation;

/// Identifies the converter that emitted a warning.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WarningSource {
    /// Converter name, for example `html`, `markdown`, or `terminal`.
    pub converter: Cow<'static, str>,
    /// Optional backend variant, for example `semantic`.
    pub variant: Option<Cow<'static, str>>,
}

impl WarningSource {
    /// Construct a warning source.
    #[must_use]
    pub fn new(converter: impl Into<Cow<'static, str>>) -> Self {
        Self {
            converter: converter.into(),
            variant: None,
        }
    }

    /// Attach a converter variant.
    #[must_use]
    pub fn with_variant(mut self, variant: impl Into<Cow<'static, str>>) -> Self {
        self.variant = Some(variant.into());
        self
    }
}

impl fmt::Display for WarningSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(variant) = &self.variant {
            write!(f, "{} ({variant})", self.converter)
        } else {
            f.write_str(&self.converter)
        }
    }
}

/// A non-fatal condition detected during conversion.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub struct Warning {
    /// Converter that emitted this warning.
    pub source: WarningSource,
    /// User-facing warning message.
    pub message: Cow<'static, str>,
    /// Optional advice/help text.
    pub advice: Option<Cow<'static, str>>,
    /// Where the condition was detected, when known.
    pub location: Option<SourceLocation>,
}

impl Warning {
    /// Construct a converter warning.
    #[must_use]
    pub fn new(
        source: WarningSource,
        message: impl Into<Cow<'static, str>>,
        location: Option<SourceLocation>,
    ) -> Self {
        Self {
            source,
            message: message.into(),
            advice: None,
            location,
        }
    }

    /// Attach advice/help text.
    #[must_use]
    pub fn with_advice(mut self, advice: impl Into<Cow<'static, str>>) -> Self {
        self.advice = Some(advice.into());
        self
    }

    /// Source location for this warning, when available.
    #[must_use]
    pub fn source_location(&self) -> Option<&SourceLocation> {
        self.location.as_ref()
    }

    /// Advice text for this warning, when available.
    #[must_use]
    pub fn advice(&self) -> Option<&str> {
        self.advice.as_deref()
    }
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(loc) = &self.location else {
            return write!(f, "{} converter: {}", self.source, self.message);
        };
        if let Some(name) = loc
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
        {
            write!(
                f,
                "{name}: {}: {} converter: {}",
                loc.positioning, self.source, self.message
            )
        } else {
            write!(
                f,
                "{}: {} converter: {}",
                loc.positioning, self.source, self.message
            )
        }
    }
}

/// Shared collector for converter warnings.
#[derive(Clone, Debug, Default)]
pub struct WarningSink {
    inner: Rc<RefCell<Vec<Warning>>>,
}

impl WarningSink {
    /// Record a warning.
    pub fn emit(&self, warning: Warning) {
        self.inner.borrow_mut().push(warning);
    }

    /// Borrow collected warnings.
    #[must_use]
    pub fn warnings(&self) -> std::cell::Ref<'_, Vec<Warning>> {
        self.inner.borrow()
    }

    /// Drain collected warnings.
    #[must_use]
    pub fn take_warnings(&self) -> Vec<Warning> {
        std::mem::take(&mut *self.inner.borrow_mut())
    }

    /// Remove all collected warnings.
    pub fn clear(&self) {
        self.inner.borrow_mut().clear();
    }
}
