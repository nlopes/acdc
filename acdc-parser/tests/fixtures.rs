#[allow(dead_code)]
#[path = "support/fixtures.rs"]
mod support;

use std::process::ExitCode;

use libtest_mimic::{Arguments, Trial};
use support::{Case, Error, discover_sources, fixture_root};

fn trials() -> Result<Vec<Trial>, Error> {
    let root = fixture_root();
    let mut tests = Vec::new();
    for source in discover_sources(&root)? {
        let case = Case::load(&root, &source, false)?;
        for expected in &case.expectations {
            for mode in expected.variant.modes() {
                let name = format!("{}::{}", case.name, format!("{mode:?}").to_lowercase());
                let ignored = case.unavailable_feature().is_some();
                let case = case.clone();
                let expected = expected.clone();
                tests.push(
                    Trial::test(name, move || {
                        case.verify_mode(&expected, *mode)
                            .map_err(|error| error.to_string().into())
                    })
                    .with_ignored_flag(ignored),
                );
            }
        }
    }
    Ok(tests)
}

fn main() -> ExitCode {
    let args = Arguments::from_args();
    match trials() {
        Ok(tests) => libtest_mimic::run(&args, tests).exit_code(),
        Err(error) => {
            eprintln!("Invalid parser fixture layout: {error}");
            ExitCode::FAILURE
        }
    }
}
