//! Non-fatal converter diagnostics.
//!
//! Backends recover from many conditions while still producing usable output
//! (deprecated roles, unsupported constructs, missing companion files). Those
//! recoveries become structured warnings rather than panics, log lines, or
//! silent fallbacks.
//!
//! Each conversion plumbs a [`Diagnostics`] handle: a borrowed
//! [`WarningSource`] tag bundled with a borrowed `&mut Vec<Warning>`. The
//! provider methods on the [`Converter`](crate::Converter) trait own both for
//! the duration of one conversion and consume the buffer into
//! [`ConversionResult`](crate::ConversionResult) at the end.
//!
//! There is no interior mutability and no per-emit allocation. Backends with
//! variants (html, markdown) tag warnings with `&'static str` variant strings,
//! so cloning a `WarningSource` is a pointer copy.

use std::{borrow::Cow, fmt};

use acdc_parser::SourceLocation;

/// Identifies the converter that emitted a warning.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    ///
    /// Pass a `&'static str` whenever possible so the resulting `WarningSource`
    /// stays in `Cow::Borrowed` land and clones cost a pointer copy.
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
///
/// Construct with [`Warning::new`], optionally attach advice via
/// [`Warning::with_advice`] and a source location via [`Warning::at`]:
///
/// ```
/// use acdc_converters_core::{Warning, WarningSource};
///
/// let w = Warning::new(WarningSource::new("html"), "missing alt text")
///     .with_advice("Add an `alt=...` attribute to the image.");
/// assert_eq!(w.advice(), Some("Add an `alt=...` attribute to the image."));
/// ```
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
    pub fn new(source: WarningSource, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            source,
            message: message.into(),
            advice: None,
            location: None,
        }
    }

    /// Attach advice/help text.
    #[must_use]
    pub fn with_advice(mut self, advice: impl Into<Cow<'static, str>>) -> Self {
        self.advice = Some(advice.into());
        self
    }

    /// Attach a source location.
    #[must_use]
    pub fn at(mut self, location: SourceLocation) -> Self {
        self.location = Some(location);
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
    /// Renders just `"{source} converter: {message}"`.
    ///
    /// Source-location formatting is left to the renderer (see `acdc-cli` for
    /// the miette-based presentation).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} converter: {}", self.source, self.message)
    }
}

/// Stack-only diagnostics handle threaded through a single conversion.
///
/// Holds a [`WarningSource`] borrow and a `&mut Vec<Warning>` borrow — no
/// owned state, no interior mutability, no allocations on construction.
/// Emission is a single push.
///
/// Pass it down to helpers via `&mut Diagnostics<'_>`; for sub-visitors that
/// take a `Diagnostics` by value, use [`Diagnostics::reborrow`] to obtain a
/// fresh handle without giving up the outer borrow.
pub struct Diagnostics<'a> {
    source: &'a WarningSource,
    warnings: &'a mut Vec<Warning>,
}

impl<'a> Diagnostics<'a> {
    /// Build a diagnostics handle.
    #[must_use]
    pub fn new(source: &'a WarningSource, warnings: &'a mut Vec<Warning>) -> Self {
        Self { source, warnings }
    }

    /// Tag attached to every warning emitted through this handle.
    #[must_use]
    pub fn source(&self) -> &WarningSource {
        self.source
    }

    /// Already-recorded warnings.
    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        self.warnings
    }

    /// Reborrow as a fresh handle with a shorter lifetime.
    ///
    /// Useful for handing the diagnostics to a sub-visitor by value while
    /// keeping the outer handle usable after the sub-visitor drops.
    pub fn reborrow(&mut self) -> Diagnostics<'_> {
        Diagnostics {
            source: self.source,
            warnings: &mut *self.warnings,
        }
    }

    /// Record a fully built warning (typically one with a `SourceLocation`).
    pub fn emit(&mut self, warning: Warning) {
        self.warnings.push(warning);
    }

    /// Record a warning with this handle's source tag.
    pub fn warn(&mut self, message: impl Into<Cow<'static, str>>) {
        self.warnings
            .push(Warning::new(self.source.clone(), message));
    }

    /// Record a warning with this handle's source tag and the given advice.
    pub fn warn_with_advice(
        &mut self,
        message: impl Into<Cow<'static, str>>,
        advice: impl Into<Cow<'static, str>>,
    ) {
        self.warnings
            .push(Warning::new(self.source.clone(), message).with_advice(advice));
    }
}
