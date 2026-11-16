//! Frozen selections and declarative inline display requests.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bumpalo::Bump;

use crate::{
    Caption, CrossReference, Document, Error, InlineMacro, InlineNode, SourceLocation, Title,
    ast::{CaptionRef, ContentRef},
    grammar::walk_document_inline_nodes_mut,
};

use super::{
    DocumentEditor, ExtensionParseResult, InlineSpec, TargetId, TitleSource, inline::plain,
    target::DocumentIdentity,
};

mod prepared;
pub use prepared::{DisplayLabel, PreparedDocument};

/// Explicit link content, independently of a target's generic reference label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayIntent {
    /// Link the finalized caption prefix.
    Caption,
    /// Link the title captured during selection.
    Title,
    /// Link the finalized caption, then append the captured title outside the link.
    CaptionAndTitle,
}

/// An immutable selection belonging to one working document.
#[derive(Clone, Debug)]
pub struct CapturedTarget {
    document: DocumentIdentity,
    selection: usize,
}

impl CapturedTarget {
    /// Describe an inline reference without allocating document storage.
    #[must_use]
    pub fn display(&self, intent: DisplayIntent) -> InlineSpec {
        InlineSpec::Display(DisplayRequest {
            target: self.clone(),
            intent,
        })
    }
}

/// An opaque captured-target display request for generated inline content.
///
/// Create requests through [`CapturedTarget::display`]. The host checks their
/// document identity when applying an edit. Titles remain captured snapshots;
/// captions and output IDs follow surviving targets during finalization.
#[derive(Clone, Debug)]
pub struct DisplayRequest {
    target: CapturedTarget,
    intent: DisplayIntent,
}

impl DisplayRequest {
    pub(super) fn build_into<'a>(
        self,
        document: &Document<'a>,
        identity: &DocumentIdentity,
        origin: &SourceLocation,
        nodes: &mut Vec<InlineNode<'a>>,
    ) -> Result<(), Error> {
        let Self { target, intent } = self;
        if target.document != *identity {
            return Err(failure(
                origin,
                "display selection belongs to another document",
            ));
        }
        let selection = document
            .generated_displays
            .selection(target.selection)
            .ok_or_else(|| failure(origin, "display selection is absent"))?;
        let mut xref = CrossReference::new(selection.id, origin.location.clone());
        xref.target_is_local = true;
        xref.generated_display = Some(Box::new(GeneratedDisplay {
            selection: Arc::clone(selection),
            intent,
            origin: origin.clone(),
            resolved: None,
        }));
        nodes.push(InlineNode::Macro(InlineMacro::CrossReference(Box::new(
            xref,
        ))));
        if intent == DisplayIntent::CaptionAndTitle && selection.title.is_present() {
            nodes.push(plain(" ", origin));
            nodes.extend(selection.title.as_inlines().iter().cloned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Selection<'a> {
    target: usize,
    id: &'a str,
    title: Title<'a>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GeneratedDisplay<'a> {
    selection: Arc<Selection<'a>>,
    intent: DisplayIntent,
    origin: SourceLocation,
    resolved: Option<ResolvedTarget<'a>>,
}

#[derive(Clone, Debug, PartialEq)]
struct ResolvedTarget<'a> {
    id: &'a str,
    caption: Option<Caption<'a>>,
    missing: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DisplayState<'a>(Option<Box<Selections<'a>>>);

#[derive(Clone, Debug, Default)]
struct Selections<'a> {
    captured: Vec<Arc<Selection<'a>>>,
    active: bool,
}

// This cache is not semantic document content. Requests themselves participate
// in cross-reference equality, and neither output nor identity depends on the cache.
impl PartialEq for DisplayState<'_> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<'a> DisplayState<'a> {
    fn selection(&self, index: usize) -> Option<&Arc<Selection<'a>>> {
        self.0.as_ref()?.captured.get(index)
    }

    pub(crate) fn is_active(&self) -> bool {
        self.0.as_ref().is_some_and(|state| state.active)
    }

    pub(super) fn activate(&mut self) {
        self.0.get_or_insert_with(Default::default).active = true;
    }
}

impl DocumentEditor<'_> {
    /// Freeze a target's title and source context, assigning an output ID when needed.
    /// Later edits do not refresh this selection. The target may subsequently be removed.
    ///
    /// # Errors
    /// Rejects foreign, removed, and unreferenceable targets.
    pub fn capture(&mut self, target: &TargetId) -> Result<CapturedTarget, Error> {
        self.ensure_id(target)?;
        let path = self.path(target)?;
        let origin = self.location(target);
        let selection = self.result.parsed.with_document_mut(|_, document| {
            let block = path
                .get(&document.blocks)
                .ok_or_else(|| failure(&origin, "target is absent"))?;
            let selected = Selection {
                target: target.slot,
                id: super::query::output_id(block)
                    .ok_or_else(|| failure(&origin, "output ID is absent"))?,
                title: block.title().cloned().unwrap_or_default(),
            };
            let state = document
                .generated_displays
                .0
                .get_or_insert_with(Default::default);
            let index = state.captured.len();
            state.captured.push(Arc::new(selected));
            Ok::<_, Error>(index)
        })?;
        Ok(CapturedTarget {
            document: target.document.clone(),
            selection,
        })
    }
}

/// Read-only display intent after structural finalization.
#[derive(Clone, Copy, Debug)]
pub struct GeneratedDisplayRef<'doc>(&'doc GeneratedDisplay<'doc>);

impl<'doc> GeneratedDisplayRef<'doc> {
    pub(crate) const fn new(value: &'doc GeneratedDisplay<'doc>) -> Self {
        Self(value)
    }
    #[must_use]
    pub fn intent(self) -> DisplayIntent {
        self.0.intent
    }
    #[must_use]
    pub fn title(self) -> Option<ContentRef<'doc>> {
        self.0.selection.title.content()
    }
    #[must_use]
    pub fn title_source(self) -> Option<&'doc TitleSource<'doc>> {
        self.0.selection.title.source()
    }
    #[must_use]
    pub fn caption(self) -> Option<CaptionRef<'doc>> {
        self.0
            .resolved
            .as_ref()?
            .caption
            .as_ref()
            .map(CaptionRef::from)
    }
    #[must_use]
    pub fn target(self) -> &'doc str {
        self.0
            .resolved
            .as_ref()
            .filter(|target| !target.missing)
            .map_or(self.0.selection.id, |target| target.id)
    }
    #[must_use]
    pub fn is_missing(self) -> bool {
        self.0.resolved.as_ref().is_none_or(|target| target.missing)
    }
}

impl serde::Serialize for GeneratedDisplayRef<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap as _;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("intent", &self.0.intent)?;
        if self.0.selection.title.is_present() {
            map.serialize_entry("title", &self.0.selection.title)?;
        }
        if let Some(target) = &self.0.resolved {
            map.serialize_entry("missing", &target.missing)?;
        }
        map.end()
    }
}

impl ExtensionParseResult {
    pub(super) fn bind_displays(&mut self) -> Result<(), Error> {
        let Some(state) = self.parsed.document_storage().generated_displays.0.as_ref() else {
            return Ok(());
        };
        if !state.active {
            self.parsed.with_document_mut(|_, document| {
                document.generated_displays = DisplayState::default();
            });
            return Ok(());
        }
        let paths: HashMap<_, _> = state
            .captured
            .iter()
            .map(|selection| {
                let target = TargetId {
                    document: self.session.identity().clone(),
                    slot: selection.target,
                };
                (
                    selection.target,
                    self.target_index()
                        .entry(&target)
                        .map(|entry| entry.path.clone()),
                )
            })
            .collect();
        let reserved: HashSet<_> = self
            .warnings()
            .iter()
            .filter_map(|warning| {
                if let crate::WarningKind::UnresolvedReference { target } = &warning.kind {
                    Some(target.clone())
                } else {
                    None
                }
            })
            .collect();
        let remapping = self.parsed.with_document_mut(|owner, document| {
            let resolved = resolve_targets(&owner.arena, document, paths, &reserved);
            let mut remapping = Vec::new();
            let mut active = false;
            walk_document_inline_nodes_mut(document, &mut |node| {
                if let InlineNode::Macro(InlineMacro::CrossReference(xref)) = node
                    && let Some(display) = &mut xref.generated_display
                    && let Some(target) = resolved.get(&display.selection.target)
                {
                    active = true;
                    xref.target = target.id;
                    display.resolved = Some(target.clone());
                    if target.missing {
                        remapping.push((
                            target.id.to_string(),
                            display.selection.id.to_string(),
                            display.origin.clone(),
                        ));
                    }
                }
            });
            if active {
                if let Some(state) = &mut document.generated_displays.0 {
                    state.captured.clear();
                }
            } else {
                document.generated_displays = DisplayState::default();
            }
            remapping
        });
        self.repair_after_edit()?;
        for warning in self.parsed.extension_warnings_mut() {
            if let crate::WarningKind::UnresolvedReference { target } = &mut warning.kind
                && let Some((_, original, origin)) =
                    remapping.iter().find(|(internal, _, origin)| {
                        internal == target
                            && warning
                                .location
                                .as_ref()
                                .is_some_and(|location| location.location == origin.location)
                    })
            {
                target.clone_from(original);
                warning.location = Some(origin.clone());
            }
        }
        Ok(())
    }
}

fn resolve_targets<'a>(
    arena: &'a Bump,
    document: &Document<'a>,
    paths: HashMap<usize, Option<super::traversal::BlockPath>>,
    reserved: &HashSet<String>,
) -> HashMap<usize, ResolvedTarget<'a>> {
    let mut resolved = HashMap::new();
    for (slot, path) in paths {
        let found = path.and_then(|path| {
            let block = path.get(&document.blocks)?;
            Some(ResolvedTarget {
                id: super::query::output_id(block)?,
                caption: block
                    .metadata()
                    .and_then(|metadata| metadata.caption.clone()),
                missing: false,
            })
        });
        let target = found.unwrap_or_else(|| {
            let mut id = format!("_acdc_missing_{}", slot + 1);
            while document.references.contains_key(id.as_str())
                || reserved.contains(&id)
                || resolved
                    .values()
                    .any(|value: &ResolvedTarget<'_>| value.id == id)
            {
                id.push('_');
            }
            ResolvedTarget {
                id: arena.alloc_str(&id),
                caption: None,
                missing: true,
            }
        });
        resolved.insert(slot, target);
    }
    resolved
}

fn failure(origin: &SourceLocation, message: &str) -> Error {
    Error::Extension(Box::new(origin.clone()), message.into())
}
