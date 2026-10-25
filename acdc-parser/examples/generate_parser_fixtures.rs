//! Generate selected parser expectations using the fixture runner's rules.
//!
//! Pass exact paths relative to fixtures/tests. With no paths, process all
//! fixtures available in this build. Use --pair with exact paths to generate
//! both mode expectations, or --mode to update one existing partner.

#[allow(dead_code)]
#[path = "../tests/support/fixtures.rs"]
mod support;

fn main() -> Result<(), support::Error> {
    let request = support::Generation::from_args(std::env::args_os().skip(1))?;
    let count = request.generate(&support::fixture_root())?;
    println!(
        "Generated expectations for {count} fixture(s). Review each change before committing."
    );
    Ok(())
}
