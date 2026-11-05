//! Prepared atom edits retain text, interpretation flags, source spans and lifetimes.

use std::{alloc::System, hint::black_box, sync::Arc};

use acdc_parser::{
    CharacterReference, CharacterReferenceKind, CompatibilityMode, LineBreak, Location, Options,
    Plain, Position, StandaloneCurvedApostrophe, Stem, StemContent, StemNotation, Symbol,
    SymbolKind, Verbatim,
    ast::{
        BlockKind, CharacterReferenceRef, LineBreakRef, PlainRef, StandaloneCurvedApostropheRef,
        StemRef, SymbolRef, VerbatimRef,
    },
    parse,
};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

type Error = Box<dyn std::error::Error>;

fn location() -> Location {
    let file = Some(Arc::new(vec![String::from("includes/λ.adoc")]));
    let mut start = Position::new(2, 2);
    start.file.clone_from(&file);
    let mut end = Position::new(2, 6);
    end.file = file;
    let mut location = Location::point(start);
    location.end = end;
    location.absolute_start = 5;
    location.absolute_end = 9;
    location
}

macro_rules! copied {
    ($value:expr, $owned:ident, $read:ident) => {{
        let original = $value;
        let before = Location::from($read::from(&original).location());
        let copy = $owned::from($read::from(&original));
        assert_eq!(copy, original);
        assert_eq!(Location::from($read::from(&copy).location()), before);
    }};
}

#[test]
fn selected_atom_copies_retain_every_value_and_include_location() -> Result<(), Error> {
    copied!(
        Plain::new("λ x", location()).with_escaped(true),
        Plain,
        PlainRef
    );
    copied!(Verbatim::new("λ x", location()), Verbatim, VerbatimRef);
    copied!(
        Symbol::new(SymbolKind::Copyright, location()),
        Symbol,
        SymbolRef
    );
    copied!(
        CharacterReference::new(CharacterReferenceKind::Named("Unknown"), location())
            .ok_or("reference")?,
        CharacterReference,
        CharacterReferenceRef
    );
    copied!(LineBreak::new(location()), LineBreak, LineBreakRef);
    copied!(
        StandaloneCurvedApostrophe::new(location()),
        StandaloneCurvedApostrophe,
        StandaloneCurvedApostropheRef
    );
    copied!(
        Stem::new("λ x", StemNotation::Latexmath, location()),
        Stem,
        StemRef
    );
    Ok(())
}

#[test]
fn plain_text_transfers_keep_the_escape_decision_and_source_lifetime() {
    let source = String::from("λ *literal*");
    let replacement = String::from("_prepared_");
    let transferred = {
        let mut plain = Plain::new(&source, location()).with_escaped(true);
        let before = Location::from(PlainRef::from(&plain).location());
        let old = plain.replace_text(&replacement);
        assert_eq!(PlainRef::from(&plain).text(), replacement);
        assert!(PlainRef::from(&plain).is_escaped());
        assert_eq!(Location::from(PlainRef::from(&plain).location()), before);
        plain.set_escaped(false);
        assert!(!PlainRef::from(&plain).is_escaped());
        old
    };
    assert_eq!(transferred, source);
    assert_eq!(transferred.as_ptr(), source.as_ptr());
}

#[test]
fn verbatim_transfers_keep_empty_prepared_text_and_source_lifetime() {
    let source = String::from("λ\tcode\n");
    let old = {
        let mut text = Verbatim::new(&source, location());
        let old = text.replace_text("");
        assert_eq!(VerbatimRef::from(&text).text(), "");
        old
    };
    assert_eq!(old, source);
    assert_eq!(old.as_ptr(), source.as_ptr());
}

#[test]
fn empty_plain_and_formula_values_stay_present_atoms() {
    let plain = Plain::new("", location());
    assert_eq!(PlainRef::from(&plain).text(), "");
    assert!(!PlainRef::from(&plain).is_escaped());
    let stem = Stem::new("", StemNotation::Asciimath, location());
    assert_eq!(StemRef::from(&stem).text(), "");
    assert_eq!(StemRef::from(&stem).notation(), StemNotation::Asciimath);
    let content = StemContent::new("", StemNotation::Latexmath);
    assert_eq!(content.text(), "");
    assert_eq!(content.notation(), StemNotation::Latexmath);
}

#[test]
fn formula_edits_do_not_translate_or_reparse_the_formula() {
    let source = String::from("λ *x*");
    let (inline_text, block_text) = {
        let mut inline = Stem::new(&source, StemNotation::Latexmath, location());
        let mut block = StemContent::new(&source, StemNotation::Latexmath);
        assert_eq!(
            inline.replace_notation(StemNotation::Asciimath),
            StemNotation::Latexmath
        );
        assert_eq!(
            block.replace_notation(StemNotation::Asciimath),
            StemNotation::Latexmath
        );
        assert_eq!(StemRef::from(&inline).text(), source);
        assert_eq!(block.text(), source);
        assert_eq!(StemRef::from(&inline).notation(), StemNotation::Asciimath);
        assert_eq!(block.notation(), StemNotation::Asciimath);
        (inline.replace_text(""), block.replace_text(""))
    };
    assert_eq!(inline_text.as_ptr(), source.as_ptr());
    assert_eq!(block_text.as_ptr(), source.as_ptr());
}

#[test]
fn changing_a_symbol_keeps_its_source_span() {
    let mut symbol = Symbol::new(SymbolKind::Copyright, location());
    let before = Location::from(SymbolRef::from(&symbol).location());
    symbol.set_kind(SymbolKind::RightDoubleArrow);
    assert_eq!(
        SymbolRef::from(&symbol).kind(),
        SymbolKind::RightDoubleArrow
    );
    assert_eq!(SymbolRef::from(&symbol).text(), "⇒");
    assert_eq!(Location::from(SymbolRef::from(&symbol).location()), before);
}

#[test]
fn character_reference_construction_keeps_validation_and_recovered_values() -> Result<(), Error> {
    for name in ["", "bad name", "1bad"] {
        assert!(CharacterReference::new(CharacterReferenceKind::Named(name), location()).is_none());
    }
    for kind in [
        CharacterReferenceKind::Named("Unknown"),
        CharacterReferenceKind::Named("λ:name"),
        CharacterReferenceKind::Numeric(0),
        CharacterReferenceKind::Numeric(u32::MAX),
    ] {
        let mut value = CharacterReference::new(kind, location()).ok_or("reference")?;
        value.set_location(Location::default());
        assert_eq!(CharacterReferenceRef::from(&value).kind(), kind);
        assert_eq!(
            Location::from(CharacterReferenceRef::from(&value).location()),
            Location::default()
        );
    }
    Ok(())
}

macro_rules! moved {
    ($value:expr, $expected:expr, $read:ident) => {{
        let mut value = $value;
        value.set_location(Location::default());
        assert_eq!(value, $expected);
        assert_eq!(
            Location::from($read::from(&value).location()),
            Location::default()
        );
    }};
}

#[test]
fn source_span_edits_leave_all_other_atom_values_unchanged() {
    moved!(
        Plain::new("λ x", location()).with_escaped(true),
        Plain::new("λ x", Location::default()).with_escaped(true),
        PlainRef
    );
    moved!(
        Verbatim::new("λ x", location()),
        Verbatim::new("λ x", Location::default()),
        VerbatimRef
    );
    moved!(
        Symbol::new(SymbolKind::Ellipsis, location()),
        Symbol::new(SymbolKind::Ellipsis, Location::default()),
        SymbolRef
    );
    moved!(
        LineBreak::new(location()),
        LineBreak::new(Location::default()),
        LineBreakRef
    );
    moved!(
        StandaloneCurvedApostrophe::new(location()),
        StandaloneCurvedApostrophe::new(Location::default()),
        StandaloneCurvedApostropheRef
    );
    moved!(
        Stem::new("λ x", StemNotation::Latexmath, location()),
        Stem::new("λ x", StemNotation::Latexmath, Location::default()),
        StemRef
    );
}

fn cost<T>(copy: impl FnOnce() -> T) -> (usize, usize) {
    let region = Region::new(ALLOCATOR);
    let value = black_box(copy());
    let stats = region.change();
    drop(value);
    (stats.allocations, stats.bytes_allocated)
}

macro_rules! free_copy {
    ($value:expr, $owned:ident, $read:ident) => {{
        let value = $value;
        assert_eq!(cost(|| $owned::from($read::from(&value))), (0, 0));
    }};
}

#[test]
fn selected_atom_copies_do_not_allocate_even_with_include_locations() -> Result<(), Error> {
    free_copy!(Plain::new("λ x", location()), Plain, PlainRef);
    free_copy!(Verbatim::new("λ x", location()), Verbatim, VerbatimRef);
    free_copy!(
        Symbol::new(SymbolKind::Copyright, location()),
        Symbol,
        SymbolRef
    );
    free_copy!(
        CharacterReference::new(CharacterReferenceKind::Named("Unknown"), location())
            .ok_or("reference")?,
        CharacterReference,
        CharacterReferenceRef
    );
    free_copy!(LineBreak::new(location()), LineBreak, LineBreakRef);
    free_copy!(
        StandaloneCurvedApostrophe::new(location()),
        StandaloneCurvedApostrophe,
        StandaloneCurvedApostropheRef
    );
    free_copy!(
        Stem::new("λ x", StemNotation::Latexmath, location()),
        Stem,
        StemRef
    );
    Ok(())
}

#[rstest::rstest]
fn selected_block_formulas_keep_prepared_text_and_notation(
    #[values(CompatibilityMode::Asciidoctor, CompatibilityMode::Draft)] mode: CompatibilityMode,
) -> Result<(), Error> {
    let parsed = parse(
        "= Math\n\n[latexmath]\n++++\nx^2 + y\n++++\n",
        &Options::builder().with_compatibility_mode(mode).build()?,
    )?;
    let block = parsed.document().blocks().next().ok_or("formula")?;
    let BlockKind::Stem(stem) = block.kind() else {
        return Err("formula".into());
    };
    assert_ne!(stem.text(), "");
    let copy = StemContent::from(stem);
    assert_eq!(copy.text(), stem.text());
    assert_eq!(copy.notation(), stem.notation());
    assert_eq!(cost(|| StemContent::from(stem)), (0, 0));
    Ok(())
}
