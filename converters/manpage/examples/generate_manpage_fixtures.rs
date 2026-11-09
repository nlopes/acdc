//! Generate expected manpage fixture output with the test adapter.
//! Optional arguments select fixture stems; omit them to generate all fixtures.

use acdc_converters_dev::generate_fixtures::FixtureGenerator;

#[path = "../tests/support/mod.rs"]
mod fixture_support;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let names = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let generator = FixtureGenerator::new("manpage", "man");
    let generator = if names.is_empty() {
        generator
    } else {
        generator.with_fixtures(&names)
    };
    generator.generate(|subdir, path, output| {
        output.extend(fixture_support::render(path, subdir == Some("embedded"))?.output);
        Ok(())
    })
}
