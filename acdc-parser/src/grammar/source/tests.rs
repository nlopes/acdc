use std::rc::Rc;

use crate::{
    Options,
    grammar::{ParserState, state::InlineContext},
    model::SourceRange,
};

use super::{RoundDirection, SourceIndex};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn windows_match_complete_input_geometry_and_keep_inline_maps_local() -> TestResult {
    let arena = bumpalo::Bump::new();
    let index = Rc::new(SourceIndex::default());
    let first = "Préface.\n\n";
    let second = "== Hèading\n";
    let third = "Body 世界.\n";
    index.append(first);
    let middle = index.append(second);
    let end = index.append(third);
    let complete = format!("{first}{second}{third}");
    let options = Rc::new(Options::default());
    let reference = ParserState::new(&complete, &arena, Rc::clone(&options));
    let view = index.window(middle..end, &arena).ok_or("missing window")?;
    let saved = view.clone();
    let mut state = ParserState::new_with_source(view, &arena, options);
    for offset in 0..=complete.len() + 2 {
        assert_eq!(
            state.create_location(offset, offset),
            reference.create_location(offset, offset)
        );
    }
    state.set_source_view(
        index
            .window(end..index.len(), &arena)
            .ok_or("missing final window")?,
    );
    assert_eq!(state.source_slice(0..first.len()), Some(first));
    assert_eq!(saved.get(middle..end), Some(second));
    assert_eq!(saved.position(middle), reference.source_position(middle));
    assert_eq!(
        saved.snap(middle + 5, RoundDirection::Backward),
        reference.source_snap(middle + 5, RoundDirection::Backward)
    );
    let inline = ParserState::for_inline_parsing(state.input, &state, InlineContext::default());
    assert!(inline.input_origin.is_none());
    assert!(Rc::ptr_eq(&inline.line_map, &state.line_map));
    assert_eq!(inline.create_location(0, 4).start.line, 1);
    assert_eq!(inline.create_location(0, 4).absolute_start, 0);
    Ok(())
}

#[test]
fn joined_windows_retain_source_and_reuse_unchanged_chunk_storage() -> TestResult {
    let arena = bumpalo::Bump::new();
    let index = Rc::new(SourceIndex::default());
    let first = "before\nleft\n";
    let second = "right 世界\n";
    index.append(first);
    let middle = index.append(second);
    let single = index
        .window(0..middle, &arena)
        .ok_or("missing first chunk")?;
    assert!(std::ptr::eq(single.input.as_ptr(), first.as_ptr()));
    let again = index.window(2..4, &arena).ok_or("missing chunk subset")?;
    assert!(Rc::ptr_eq(&single.line_map, &again.line_map));
    let joined = index
        .window(9..index.len(), &arena)
        .ok_or("missing joined window")?;
    assert_eq!(joined.input, "left\nright 世界\n");
    let repeated = index
        .window(9..index.len(), &arena)
        .ok_or("missing retained window")?;
    assert!(std::ptr::eq(joined.input.as_ptr(), repeated.input.as_ptr()));
    let later = index.append("later\n");
    let state = ParserState::new_with_source(
        index
            .window(later..index.len(), &arena)
            .ok_or("missing later window")?,
        &arena,
        Rc::new(Options::default()),
    );
    assert_eq!(state.source_slice(7..later), Some("left\nright 世界\n"));
    assert!(
        state
            .source_slice(std::ops::Range { start: 9, end: 8 })
            .is_none()
    );
    Ok(())
}

#[test]
fn windows_preserve_included_diagnostic_ranges_and_final_separator_clipping() -> TestResult {
    let arena = bumpalo::Bump::new();
    let index = Rc::new(SourceIndex::default());
    let first = "preamble\n";
    let included = "第一行\n";
    let last = "café 世界\n";
    index.append(first);
    let included_start = index.append(included);
    let last_start = index.append(last);
    let complete = format!("{first}{included}{last}");
    let range = SourceRange {
        start_offset: included_start,
        end_offset: complete.len(),
        file: Some("/virtual/part.adoc".into()),
        file_chain: vec!["part.adoc".into()],
        start_line: 41,
        source_start_offset: 900,
        column_shift: 0,
    };
    let mut reference = ParserState::new(&complete, &arena, Rc::new(Options::default()));
    reference.source_ranges.push(range.clone());
    let mut state = ParserState::new_with_source(
        index
            .window(last_start..index.len(), &arena)
            .ok_or("missing last window")?,
        &arena,
        Rc::new(Options::default()),
    );
    state.source_ranges.push(range);
    for offset in last_start..complete.len() {
        assert_eq!(
            state.create_error_source_location(state.create_location(offset, offset)),
            reference.create_error_source_location(reference.create_location(offset, offset))
        );
    }
    let saved = state.source_view();
    index.seal(complete.len() - 1);
    let clipped = index
        .window(last_start..index.len(), &arena)
        .ok_or("missing clipped EOF window")?;
    assert_eq!(clipped.input, last.trim_end_matches('\n'));
    state.set_source_view(clipped);
    assert_eq!(
        saved.position(index.len() + 1),
        state.source_position(index.len())
    );
    assert_eq!(
        saved.step(index.len(), RoundDirection::Backward),
        state.source_step(index.len(), RoundDirection::Backward)
    );
    Ok(())
}

#[test]
fn windowed_inline_preprocessing_matches_complete_input_nodes_and_diagnostics() -> TestResult {
    use crate::grammar::{helpers::BlockParsingMetadata, inline_processing::process_inlines};

    let arena = bumpalo::Bump::new();
    let prefix = "Before 世界.\n\n";
    let content = "café [[target,pass:[*label*]]] <<target>> {counter:test} +raw 世界+";
    let source = format!("{prefix}{content}\n");
    for mode in [
        crate::CompatibilityMode::Asciidoctor,
        crate::CompatibilityMode::Draft,
    ] {
        let options = Rc::new(Options::builder().with_compatibility_mode(mode).build()?);
        let mut reference = ParserState::new(&source, &arena, Rc::clone(&options));
        let index = Rc::new(SourceIndex::default());
        index.append(prefix);
        let start = index.append(arena.alloc_str(&format!("{content}\n")));
        let mut windowed = ParserState::new_with_source(
            index
                .window(start..index.len(), &arena)
                .ok_or("missing inline window")?,
            &arena,
            options,
        );
        let metadata = BlockParsingMetadata::new(mode);
        let expected = process_inlines(
            &mut reference,
            &metadata,
            start,
            start + content.len(),
            0,
            content,
        )?;
        let actual = process_inlines(&mut windowed, &metadata, 0, content.len(), start, content)?;
        assert_eq!(actual, expected);
        assert_eq!(
            format!("{:?}", &windowed.warnings.borrow()[..]),
            format!("{:?}", &reference.warnings.borrow()[..])
        );
        let count = reference.reference_text_sources.borrow().len();
        assert_eq!(windowed.reference_text_sources.borrow().len(), count);
        let next = index.append("Later.\n");
        windowed.set_source_view(
            index
                .window(next..index.len(), &arena)
                .ok_or("missing later view")?,
        );
        for id in 0..count {
            assert_eq!(
                crate::grammar::reference_text::resolve(&windowed, id)?,
                crate::grammar::reference_text::resolve(&reference, id)?
            );
        }
    }
    Ok(())
}
