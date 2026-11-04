use std::{
    io::{self, Write},
    path::Path,
};

use acdc_parser::{InlineNode, Warning, ast::DocumentRef};
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Parse(#[from] acdc_parser::Error),
    #[cfg(feature = "asg")]
    #[error(transparent)]
    Asg(#[from] acdc_parser::asg::Error),
    #[cfg(feature = "tck")]
    #[error("unsupported TCK type `{0}`; expected `block` or `inline`")]
    UnsupportedType(String),
    #[error("ASG output requires a build with the `asg` feature")]
    UnavailableFormat,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Format {
    NativeJson,
    UpstreamAsg,
    AcdcAsgV1,
}

impl Format {
    pub fn check_available(self) -> Result<(), Error> {
        if !cfg!(feature = "asg") && !matches!(self, Self::NativeJson) {
            return Err(Error::UnavailableFormat);
        }
        Ok(())
    }
}

pub fn document(value: DocumentRef<'_>, format: Format) -> Result<Value, Error> {
    match format {
        Format::NativeJson => Ok(serde_json::to_value(value)?),
        #[cfg(feature = "asg")]
        Format::UpstreamAsg => Ok(acdc_parser::asg::document(value)?),
        #[cfg(feature = "asg")]
        Format::AcdcAsgV1 => Ok(acdc_parser::asg::document_with_profile(
            value,
            acdc_parser::asg::Profile::Acdc,
        )?),
        #[cfg(not(feature = "asg"))]
        Format::UpstreamAsg | Format::AcdcAsgV1 => Err(Error::UnavailableFormat),
    }
}

pub fn inlines(value: &[InlineNode<'_>], format: Format) -> Result<Value, Error> {
    match format {
        Format::NativeJson => Ok(serde_json::to_value(value)?),
        #[cfg(feature = "asg")]
        Format::UpstreamAsg => Ok(acdc_parser::asg::inlines(value)?),
        #[cfg(feature = "asg")]
        Format::AcdcAsgV1 => Ok(acdc_parser::asg::inlines_with_profile(
            value,
            acdc_parser::asg::Profile::Acdc,
        )?),
        #[cfg(not(feature = "asg"))]
        Format::UpstreamAsg | Format::AcdcAsgV1 => Err(Error::UnavailableFormat),
    }
}

pub fn warnings(warnings: &[Warning]) -> Result<(), Error> {
    let mut stderr = io::stderr().lock();
    for warning in warnings {
        let value = json!({"severity":"warning","kind":"parser-warning","message":warning.to_string(),
            "location":warning.source_location().map(|source| &source.location),"advice":warning.advice()});
        serde_json::to_writer(&mut stderr, &value)?;
        writeln!(stderr)?;
    }
    Ok(())
}

pub fn write(value: &Value, output: &Path) -> Result<(), Error> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    if output == Path::new("-") {
        io::stdout().lock().write_all(&bytes)?;
    } else {
        std::fs::write(output, bytes)?;
    }
    Ok(())
}

pub fn finish(result: Result<(), Error>) {
    if let Err(error) = result {
        let value = diagnostic(&error);
        let mut stderr = io::stderr().lock();
        let _ = serde_json::to_writer(&mut stderr, &value);
        let _ = writeln!(stderr);
        std::process::exit(1);
    }
}

fn diagnostic(error: &Error) -> Value {
    if let Error::Parse(error) = error {
        return json!({"severity":"error","kind":"parse","message":error.to_string(),
            "location":error.source_location().map(|source| &source.location),"advice":error.advice()});
    }
    #[cfg(feature = "asg")]
    if let Error::Asg(error) = error {
        use acdc_parser::asg::Error;
        return match error {
            Error::Unsupported {
                path,
                construct,
                location,
                message,
            } => {
                json!({"severity":"error","kind":"unsupported-asg","path":path,"construct":construct,"location":location,"message":message})
            }
            Error::InvalidLocation {
                path,
                construct,
                location,
                message,
            } => {
                json!({"severity":"error","kind":"invalid-asg-location","path":path,"construct":construct,"location":location,"message":message})
            }
            Error::Projection { path, message } => {
                json!({"severity":"error","kind":"invalid-asg-projection","path":path,"message":message})
            }
            Error::Schema(message) => {
                json!({"severity":"error","kind":"asg-schema","message":message})
            }
            _ => json!({"severity":"error","kind":"asg","message":error.to_string()}),
        };
    }
    let kind = match error {
        Error::Io(_) => "io",
        Error::Json(_) => "invalid-json",
        Error::Parse(_) => "parse",
        #[cfg(feature = "asg")]
        Error::Asg(_) => "asg",
        #[cfg(feature = "tck")]
        Error::UnsupportedType(_) => "unsupported-type",
        Error::UnavailableFormat => "unavailable-format",
    };
    json!({"severity":"error","kind":kind,"message":error.to_string()})
}
