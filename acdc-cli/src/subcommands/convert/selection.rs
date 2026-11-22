//! Bind statically linked converter choices to the input and output routes of this CLI.
use std::sync::OnceLock;

use acdc_converters_core::registry::{ConverterRegistry, SelectionError};
use clap::builder::{
    PossibleValue, PossibleValuesParser, StringValueParser, TypedValueParser as _, ValueParser,
};

use super::{
    Args, ConversionSummary, Extensions, MietteResult, Options, OptionsBuilder, error,
    run_converter,
};

type Run = dyn Fn(
        &Args,
        &Options,
        OptionsBuilder<'static>,
        Option<&Extensions>,
    ) -> MietteResult<ConversionSummary>
    + Send
    + Sync;

pub(super) struct Runner {
    pub(super) run: Box<Run>,
    #[cfg(feature = "pdf")]
    pub(super) pdf_options: bool,
}

impl Runner {
    fn new(
        run: impl Fn(
            &Args,
            &Options,
            OptionsBuilder<'static>,
            Option<&Extensions>,
        ) -> MietteResult<ConversionSummary>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            run: Box::new(run),
            #[cfg(feature = "pdf")]
            pdf_options: false,
        }
    }

    /// Adapt a static converter factory once, retaining its typed pipeline for each input.
    pub(super) fn converter<P, F>(factory: F) -> Self
    where
        P: acdc_converters_core::Converter<'static>,
        P::Error: Send + 'static,
        F: Fn(Options, OptionsBuilder<'static>) -> Result<P, P::Error> + Send + Sync + 'static,
    {
        Self::new(move |args, opts, parser, extensions| {
            run_converter(args, opts, parser, extensions, &factory)
        })
    }
}

pub(super) fn builtins() -> Result<ConverterRegistry<Runner>, SelectionError> {
    #[cfg(any(feature = "manpage", feature = "pdf", feature = "markdown"))]
    use acdc_converters_core::Converter as _;

    let mut backends = ConverterRegistry::default();
    #[cfg(feature = "html")]
    backends.register(acdc_converters_html::REGISTRATION, |variant| {
        Runner::converter(move |opts, parser| {
            acdc_converters_html::Converter::new_with_variant(opts, parser, variant)
        })
    })?;
    #[cfg(feature = "markdown")]
    backends.register(acdc_converters_markdown::REGISTRATION, |variant| {
        Runner::converter(move |opts, parser| {
            acdc_converters_markdown::Converter::new(opts, parser)
                .map(|converter| converter.with_variant(variant))
        })
    })?;
    #[cfg(feature = "manpage")]
    backends.register(acdc_converters_manpage::REGISTRATION, |()| {
        Runner::converter(acdc_converters_manpage::Converter::new)
    })?;
    #[cfg(feature = "terminal")]
    backends.register(acdc_converters_terminal::REGISTRATION, |()| {
        Runner::new(|args, options, parser, extensions| {
            super::run_terminal_with_pager(args, options, parser, extensions)
                .map_err(|e| error::display(&e))
        })
    })?;
    #[cfg(feature = "pdf")]
    backends.register(acdc_converters_pdf::REGISTRATION, |()| {
        let run = |args: &Args,
                   options: &Options,
                   parser: OptionsBuilder<'static>,
                   extensions: Option<&Extensions>| {
            let pdf_options = super::pdf_options_from_args(args);
            run_converter(args, options, parser, extensions, move |opts, attrs| {
                acdc_converters_pdf::Converter::new(opts, attrs)
                    .map(|converter| converter.with_pdf_options(pdf_options.clone()))
            })
        };
        Runner {
            run: Box::new(run),
            pdf_options: true,
        }
    })?;
    Ok(backends)
}

pub(super) fn installed() -> MietteResult<&'static ConverterRegistry<Runner>> {
    static INSTALLED: OnceLock<Result<ConverterRegistry<Runner>, SelectionError>> = OnceLock::new();
    INSTALLED
        .get_or_init(builtins)
        .as_ref()
        .map_err(error::display)
}

pub(super) fn values(spellings: impl Iterator<Item = (&'static str, bool)>) -> ValueParser {
    PossibleValuesParser::new(
        spellings.map(|(name, visible)| PossibleValue::new(name).hide(!visible)),
    )
    .map(String::into_boxed_str)
    .into()
}

pub(super) fn backend_parser() -> ValueParser {
    installed().map_or_else(
        |_| StringValueParser::new().map(String::into_boxed_str).into(),
        |catalog| values(catalog.backend_spellings()),
    )
}

pub(super) fn variant_parser() -> ValueParser {
    installed().map_or_else(
        |_| StringValueParser::new().map(String::into_boxed_str).into(),
        |catalog| values(catalog.variant_spellings()),
    )
}
