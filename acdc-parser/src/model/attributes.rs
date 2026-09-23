use std::{borrow::Cow, fmt, sync::Arc};

use rustc_hash::{FxBuildHasher, FxHashMap};
use serde::{
    Serialize,
    ser::{SerializeMap, Serializer},
};

use crate::{
    Error, SourceLocation,
    document_attribute::{
        AssignmentDecision, AssignmentRequest, AssignmentState, AttributeLock, AttributeOrigin,
        MAX_INCLUDE_DEPTH_ATTR, RawAttributeValue, assignment_decision, default_value,
        is_intrinsic, processor_assignment_state, validate_assignment_value,
    },
};

pub(crate) type RawAttributes<'a> = FxHashMap<AttributeName<'a>, AttributeValue<'a>>;

pub const MAX_TOC_LEVELS: u8 = 5;
pub const MAX_SECTION_LEVELS: u8 = 5;

/// Strip surrounding single or double quotes from a string.
///
/// Attribute values in `AsciiDoc` can be quoted with either single or double quotes.
/// This function strips the outermost matching quotes from both ends.
#[must_use]
pub fn strip_quotes(s: &str) -> &str {
    s.trim_start_matches(['"', '\''])
        .trim_end_matches(['"', '\''])
}

/// A defined document attribute, including its retained text representation.
///
/// Numeric values use attribute-specific validation. Text is not inferred to be
/// numeric merely because it contains digits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentAttributeValue<'a>(ValueKind<'a>);

#[derive(Clone, Debug, PartialEq, Eq)]
enum ValueKind<'a> {
    Text(Cow<'a, str>),
    Presence,
    Integer {
        value: i128,
        source: Option<Cow<'a, str>>,
    },
}

impl<'a> DocumentAttributeValue<'a> {
    pub(crate) const fn presence() -> Self {
        Self(ValueKind::Presence)
    }

    pub(crate) const fn integer(value: i128) -> Self {
        Self(ValueKind::Integer {
            value,
            source: None,
        })
    }

    /// Return text only when the semantic value is textual.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match &self.0 {
            ValueKind::Text(value) => Some(value),
            ValueKind::Presence | ValueKind::Integer { .. } => None,
        }
    }

    /// Return the validated integer, without parsing a textual value.
    #[must_use]
    pub const fn as_integer(&self) -> Option<i128> {
        match self.0 {
            ValueKind::Integer { value, .. } => Some(value),
            ValueKind::Text(_) | ValueKind::Presence => None,
        }
    }

    /// Return whether the attribute was set without a semantic value.
    #[must_use]
    pub const fn is_presence(&self) -> bool {
        matches!(self.0, ValueKind::Presence)
    }

    /// Return retained text, including the spelling of an explicit integer.
    ///
    /// Presence and generated integer defaults have no supplied text. This does
    /// not remove surrounding quotes or undo definition-time substitution.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.stored_text().map(AsRef::as_ref)
    }

    /// Write the attribute-reference representation without allocating.
    ///
    /// Presence writes nothing; explicit integers retain their spelling.
    ///
    /// # Errors
    ///
    /// Returns errors from the output buffer.
    pub fn write_text<W: fmt::Write>(&self, output: &mut W) -> fmt::Result {
        match &self.0 {
            ValueKind::Text(value) => output.write_str(value),
            ValueKind::Presence => Ok(()),
            ValueKind::Integer { value, source } => match source {
                Some(text) => output.write_str(text),
                None => write!(output, "{value}"),
            },
        }
    }

    fn stored_text(&self) -> Option<&Cow<'a, str>> {
        match &self.0 {
            ValueKind::Text(value) => Some(value),
            ValueKind::Integer { source, .. } => source.as_ref().filter(|text| !text.is_empty()),
            ValueKind::Presence => None,
        }
    }

    pub(crate) fn as_borrowed(&self) -> DocumentAttributeValue<'_> {
        DocumentAttributeValue(match &self.0 {
            ValueKind::Text(value) => ValueKind::Text(Cow::Borrowed(value)),
            ValueKind::Presence => ValueKind::Presence,
            ValueKind::Integer { value, source } => ValueKind::Integer {
                value: *value,
                source: source.as_deref().map(Cow::Borrowed),
            },
        })
    }

    /// Consume the value, retaining existing allocations for owned text.
    #[must_use]
    pub fn into_static(self) -> DocumentAttributeValue<'static> {
        DocumentAttributeValue(match self.0 {
            ValueKind::Text(value) => ValueKind::Text(Cow::Owned(value.into_owned())),
            ValueKind::Presence => ValueKind::Presence,
            ValueKind::Integer { value, source } => ValueKind::Integer {
                value,
                source: source.map(|text| Cow::Owned(text.into_owned())),
            },
        })
    }

    fn into_input(self) -> AttributeValue<'a> {
        match self.0 {
            ValueKind::Text(text) => AttributeValue::String(text),
            ValueKind::Presence => AttributeValue::Bool(true),
            ValueKind::Integer { value, source } => match source {
                Some(text) if text.is_empty() => AttributeValue::Bool(true),
                Some(text) => AttributeValue::String(text),
                None => AttributeValue::String(value.to_string().into()),
            },
        }
    }

    fn serialized_value(&self, presence_as_empty: bool) -> SerializedDocumentAttributeValue<'_> {
        match &self.0 {
            ValueKind::Text(value) => SerializedDocumentAttributeValue::Text(value),
            ValueKind::Presence if presence_as_empty => SerializedDocumentAttributeValue::Text(""),
            ValueKind::Presence => SerializedDocumentAttributeValue::Bool(true),
            ValueKind::Integer { value, source } => match source.as_deref() {
                Some("") => SerializedDocumentAttributeValue::Bool(true),
                Some(text) => SerializedDocumentAttributeValue::Text(text),
                None => SerializedDocumentAttributeValue::Integer(*value),
            },
        }
    }
}

impl<'a> From<Cow<'a, str>> for DocumentAttributeValue<'a> {
    fn from(value: Cow<'a, str>) -> Self {
        Self(ValueKind::Text(value))
    }
}

impl<'a> From<&'a DocumentAttributeAssignment<'_>> for AttributeValue<'a> {
    fn from(assignment: &'a DocumentAttributeAssignment<'_>) -> Self {
        match assignment {
            DocumentAttributeAssignment::Set(value) => value.as_borrowed().into_input(),
            DocumentAttributeAssignment::Unset => Self::Bool(false),
        }
    }
}

impl<'a> From<&'a str> for DocumentAttributeValue<'a> {
    fn from(value: &'a str) -> Self {
        Self::from(Cow::Borrowed(value))
    }
}

impl From<String> for DocumentAttributeValue<'_> {
    fn from(value: String) -> Self {
        Self::from(Cow::Owned(value))
    }
}

/// An accepted set or unset document-attribute assignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentAttributeAssignment<'a> {
    /// Replace the active value.
    Set(DocumentAttributeValue<'a>),
    /// Hide the active value, including an inherited value.
    Unset,
}

impl<'a> DocumentAttributeAssignment<'a> {
    pub(crate) fn new(mut assignment: Self, original_source_text: Option<Cow<'a, str>>) -> Self {
        if let Self::Set(DocumentAttributeValue(ValueKind::Integer { source, .. })) =
            &mut assignment
        {
            *source = original_source_text;
        }
        assignment
    }

    /// Borrow the assigned value, or return none for an unset.
    #[must_use]
    pub const fn value(&self) -> Option<&DocumentAttributeValue<'a>> {
        match self {
            Self::Set(value) => Some(value),
            Self::Unset => None,
        }
    }

    pub(crate) fn to_static(&self) -> DocumentAttributeAssignment<'static> {
        match self {
            Self::Set(value) => DocumentAttributeAssignment::Set(value.as_borrowed().into_static()),
            Self::Unset => DocumentAttributeAssignment::Unset,
        }
    }

    pub(crate) fn into_static(self) -> DocumentAttributeAssignment<'static> {
        match self {
            Self::Set(value) => DocumentAttributeAssignment::Set(value.into_static()),
            Self::Unset => DocumentAttributeAssignment::Unset,
        }
    }

    fn stored_text(&self) -> Option<&Cow<'a, str>> {
        self.value().and_then(DocumentAttributeValue::stored_text)
    }

    pub(crate) fn serialized_value(
        &self,
        presence_as_empty: bool,
    ) -> SerializedDocumentAttributeValue<'_> {
        match self {
            Self::Set(value) => value.serialized_value(presence_as_empty),
            Self::Unset => SerializedDocumentAttributeValue::Bool(false),
        }
    }
}

pub(crate) enum SerializedDocumentAttributeValue<'a> {
    Text(&'a str),
    Bool(bool),
    Integer(i128),
}

impl Serialize for SerializedDocumentAttributeValue<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Text(value) => serializer.serialize_str(value),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Integer(value) => serializer.serialize_i128(*value),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DocumentAttributeStatus<'a> {
    Set(&'a DocumentAttributeValue<'a>),
    Unset,
    Absent,
}

/// Validate bounded attributes and emit warnings for out-of-range values.
///
/// Some attributes like `sectnumlevels` and `toclevels` have valid ranges.
/// This function emits a warning if the value is outside the valid range.
fn validate_bounded_attribute(key: &str, value: &AttributeValue<'_>) {
    let AttributeValue::String(s) = value else {
        return;
    };

    match key {
        "sectnumlevels" => {
            if let Ok(level) = s.parse::<u8>()
                && level > MAX_SECTION_LEVELS
            {
                tracing::warn!(
                    attribute = "sectnumlevels",
                    value = level,
                    "sectnumlevels must be between 0 and {MAX_SECTION_LEVELS}, got {level}. \
                         Values above {MAX_SECTION_LEVELS} will be treated as {MAX_SECTION_LEVELS}."
                );
            }
        }
        "toclevels" => {
            if let Ok(level) = s.parse::<u8>()
                && level > MAX_TOC_LEVELS
            {
                tracing::warn!(
                    attribute = "toclevels",
                    value = level,
                    "toclevels must be between 0 and {MAX_TOC_LEVELS}, got {level}. \
                         Values above {MAX_TOC_LEVELS} will be treated as {MAX_TOC_LEVELS}."
                );
            }
        }
        _ => {}
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredAssignment<'a> {
    assignment: DocumentAttributeAssignment<'a>,
    state: AssignmentState,
}

// A 128-bit approximate membership filter for explicit attributes. `lower`
// stores slots 0..63 and `upper` stores slots 64..127. A clear bit proves that
// a name is absent; a set bit requires a map lookup because names can collide.
#[derive(Clone, Default)]
struct EntryFilter {
    lower: u64,
    upper: u64,
}

impl EntryFilter {
    fn slot(name: &str) -> usize {
        // Length and boundary bytes spread common attribute names across the
        // slots without hashing the full string.
        let bytes = name.as_bytes();
        let length = bytes.len();
        let first = usize::from(bytes.first().copied().unwrap_or_default());
        let last = usize::from(bytes.last().copied().unwrap_or_default());
        (length.wrapping_mul(13) ^ first.wrapping_mul(7) ^ last) & 127
    }

    fn may_contain(&self, name: &str) -> bool {
        let slot = Self::slot(name);
        if slot < 64 {
            self.lower & (1_u64 << slot) != 0
        } else {
            self.upper & (1_u64 << (slot - 64)) != 0
        }
    }

    fn insert(&mut self, name: &str) {
        let slot = Self::slot(name);
        if slot < 64 {
            self.lower |= 1_u64 << slot;
        } else {
            self.upper |= 1_u64 << (slot - 64);
        }
    }
}

/// Validated document attributes with universal defaults.
///
/// Parsed documents expose their end-of-header snapshot through this read-only
/// view. Use [`crate::Options::builder()`] to supply values.
///
/// Use `DocumentAttributes::default()` to get a map with universal defaults applied.
///
/// Parsed attributes cannot be changed through the read API:
///
/// ```compile_fail
/// let mut attributes = acdc_parser::DocumentAttributes::default();
/// attributes.set("name".into(), "value".into());
/// ```
#[derive(Clone)]
pub struct DocumentAttributes<'a> {
    entries: FxHashMap<AttributeName<'a>, StoredAssignment<'a>>,
    base: Option<Arc<FxHashMap<AttributeName<'static>, DocumentAttributeValue<'static>>>>,
    entry_filter: EntryFilter,
}

impl<'a> DocumentAttributes<'a> {
    pub(crate) fn from_inputs(
        inputs: RawAttributes<'a>,
        state: AssignmentState,
    ) -> Result<DocumentAttributes<'a>, Error> {
        let mut attributes = DocumentAttributes::default();
        attributes.entries.reserve(inputs.len());
        for (name, value) in inputs {
            if matches!(name.as_ref(), "outdir" | "outfile") {
                continue;
            }
            validate_bounded_attribute(&name, &value);
            let entry = validate_assignment_value(&name, RawAttributeValue::from(value))
                .map_err(|invalid| invalid.into_error(&name, None))?;
            attributes.set_entry(name, entry, state);
        }
        Ok(attributes)
    }
}

impl<'a> DocumentAttributes<'a> {
    fn into_input_map(self) -> RawAttributes<'a> {
        let mut inputs = RawAttributes::default();
        let defaults = default_document_attribute_values();
        if let Some(base) = self.base
            && !Arc::ptr_eq(&base, &defaults)
        {
            for (name, value) in base.iter() {
                if defaults.get(name.as_ref()) != Some(value) {
                    inputs.insert(name.clone(), value.clone().into_input());
                }
            }
        }
        for (name, stored) in self.entries {
            let value = match stored.assignment {
                DocumentAttributeAssignment::Unset => AttributeValue::None,
                DocumentAttributeAssignment::Set(value) => value.into_input(),
            };
            inputs.insert(name, value);
        }
        inputs
    }
}

pub(crate) fn default_document_attribute_values()
-> Arc<FxHashMap<AttributeName<'static>, DocumentAttributeValue<'static>>> {
    use std::sync::LazyLock;

    static DEFAULTS: LazyLock<
        Arc<FxHashMap<AttributeName<'static>, DocumentAttributeValue<'static>>>,
    > = LazyLock::new(|| {
        Arc::new(
            crate::constants::DEFAULT_ATTRIBUTE_ENTRIES
                .iter()
                .filter_map(|(name, value)| {
                    let value = match value {
                        AttributeValue::String(value) => {
                            DocumentAttributeValue::from(value.clone())
                        }
                        AttributeValue::Bool(true) => DocumentAttributeValue::presence(),
                        AttributeValue::Bool(false) | AttributeValue::None => return None,
                    };
                    Some((name.clone(), value))
                })
                .collect(),
        )
    });
    Arc::clone(&DEFAULTS)
}

impl fmt::Debug for DocumentAttributes<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DocumentAttributes")
            .field("entries", &self.entries)
            .field(
                "base_entry_count",
                &self.base.as_ref().map(|base| base.len()),
            )
            .finish()
    }
}

impl PartialEq for DocumentAttributes<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries && self.base == other.base
    }
}

impl Default for DocumentAttributes<'_> {
    fn default() -> Self {
        Self {
            entries: FxHashMap::default(),
            base: Some(default_document_attribute_values()),
            entry_filter: EntryFilter::default(),
        }
    }
}

impl<'a> DocumentAttributes<'a> {
    pub(crate) fn into_configuration(self) -> (RawAttributes<'a>, RawAttributes<'a>) {
        let Self { entries, base, .. } = self;
        let mut caller = RawAttributes::default();
        let mut defaults = Self {
            entries: FxHashMap::default(),
            base,
            entry_filter: EntryFilter::default(),
        }
        .into_input_map();
        for (name, stored) in entries {
            let value = match stored.assignment {
                DocumentAttributeAssignment::Set(value) => value.into_input(),
                DocumentAttributeAssignment::Unset => AttributeValue::None,
            };
            if stored.state.origin == AttributeOrigin::Caller {
                caller.insert(name, value);
            } else {
                defaults.insert(name, value);
            }
        }
        (caller, defaults)
    }

    /// Reuse explicit values as fresh configuration, retaining unsets and spelling.
    ///
    /// Universal defaults remain implicit. Rebuilding options determines the new
    /// assignment precedence; parser-internal locks and origins are not exported.
    pub fn into_inputs(self) -> impl Iterator<Item = (AttributeName<'a>, AttributeValue<'a>)> {
        self.into_input_map().into_iter()
    }

    fn entry(&self, name: &str) -> Option<&StoredAssignment<'a>> {
        if !self.entry_filter.may_contain(name) {
            return None;
        }
        self.entries.get(name)
    }

    fn record_entry(&mut self, name: &str) {
        self.entry_filter.insert(name);
    }

    fn rebuild_entry_filter(&mut self) {
        let mut filter = EntryFilter::default();
        for name in self.entries.keys() {
            filter.insert(name);
        }
        self.entry_filter = filter;
    }

    /// Create an empty `DocumentAttributes` without default attributes.
    /// Used for lightweight parsing contexts (e.g., quotes-only) where
    /// document attributes aren't needed.
    pub(crate) fn empty() -> Self {
        Self {
            entries: FxHashMap::default(),
            base: None,
            entry_filter: EntryFilter::default(),
        }
    }

    /// Iterate over effective, referenceable attributes.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &DocumentAttributeValue<'_>)> {
        let stored = self.entries.keys().map(AsRef::as_ref);
        let base = self
            .base
            .iter()
            .flat_map(|base| base.iter())
            .filter(|(name, _)| !self.entries.contains_key(name.as_ref()))
            .map(|(name, _)| name.as_ref());
        let registry_defaults = std::iter::once(MAX_INCLUDE_DEPTH_ATTR)
            .filter(|name| self.effective_default(name).is_some());
        stored
            .chain(base)
            .chain(registry_defaults)
            .filter_map(|name| self.get(name).map(|value| (name, value)))
    }

    /// Check whether the effective attribute view is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }

    /// Only for derived text attributes, not typed caller inputs.
    pub(crate) fn set_text(&mut self, name: AttributeName<'a>, value: Cow<'a, str>) {
        debug_assert_ne!(name.as_ref(), MAX_INCLUDE_DEPTH_ATTR);
        if matches!(name.as_ref(), "outdir" | "outfile") {
            return;
        }
        self.set_entry(
            name,
            DocumentAttributeAssignment::new(
                DocumentAttributeAssignment::Set(DocumentAttributeValue::from(value)),
                None,
            ),
            AssignmentState::PROCESSOR,
        );
    }

    pub(crate) fn insert_text(&mut self, name: AttributeName<'a>, value: Cow<'a, str>) {
        if !self.entries.contains_key(name.as_ref())
            && !self
                .base
                .as_ref()
                .is_some_and(|base| base.contains_key(name.as_ref()))
        {
            self.set_text(name, value);
        }
    }

    #[cfg(test)]
    pub(crate) fn insert(
        &mut self,
        name: AttributeName<'a>,
        value: AttributeValue<'a>,
    ) -> Result<(), Error> {
        if self.entries.contains_key(name.as_ref())
            || self
                .base
                .as_ref()
                .is_some_and(|base| base.contains_key(name.as_ref()))
        {
            return Ok(());
        }
        self.set(name, value)
    }

    #[cfg(test)]
    pub(crate) fn set(
        &mut self,
        name: AttributeName<'a>,
        value: AttributeValue<'a>,
    ) -> Result<(), Error> {
        if matches!(name.as_ref(), "outdir" | "outfile") {
            return Ok(());
        }
        validate_bounded_attribute(&name, &value);
        let entry = validate_assignment_value(&name, RawAttributeValue::from(value))
            .map_err(|invalid| invalid.into_error(&name, None))?;
        self.set_entry(name, entry, AssignmentState::PROCESSOR);
        Ok(())
    }

    pub(crate) fn set_entry(
        &mut self,
        name: AttributeName<'a>,
        assignment: DocumentAttributeAssignment<'a>,
        state: AssignmentState,
    ) {
        self.record_entry(&name);
        self.entries
            .insert(name, StoredAssignment { assignment, state });
    }

    pub(crate) fn normalize_assignment(
        &mut self,
        name: AttributeName<'a>,
        assignment: DocumentAttributeAssignment<'a>,
    ) {
        let state = self
            .entry(&name)
            .map_or(AssignmentState::PROCESSOR, |entry| entry.state);
        self.set_entry(name, assignment, state);
    }

    pub(crate) fn assign_document_value(
        &mut self,
        name: AttributeName<'a>,
        value: RawAttributeValue<'a>,
        in_header: bool,
        force_locked: bool,
        source_location: Option<SourceLocation>,
    ) -> Result<Option<DocumentAttributeAssignment<'a>>, Error> {
        self.assign(
            name,
            AssignmentRequest {
                raw: value,
                state: AssignmentState::document(),
                in_header,
                force_locked,
                source_location,
            },
        )
    }

    pub(crate) fn set_intrinsic(
        &mut self,
        name: AttributeName<'static>,
        value: DocumentAttributeValue<'static>,
    ) {
        debug_assert!(is_intrinsic(&name));
        if self.entries.remove(name.as_ref()).is_some() {
            self.rebuild_entry_filter();
        }
        Arc::make_mut(self.base.get_or_insert_with(|| {
            Arc::new(FxHashMap::with_capacity_and_hasher(16, FxBuildHasher))
        }))
        .insert(name, value);
    }

    pub(crate) fn set_base_intrinsic(
        &mut self,
        name: AttributeName<'static>,
        value: DocumentAttributeValue<'static>,
    ) {
        debug_assert!(is_intrinsic(&name));
        Arc::make_mut(self.base.get_or_insert_with(|| {
            Arc::new(FxHashMap::with_capacity_and_hasher(16, FxBuildHasher))
        }))
        .insert(name, value);
    }

    pub(crate) fn set_base_map(
        &mut self,
        base: Arc<FxHashMap<AttributeName<'static>, DocumentAttributeValue<'static>>>,
    ) {
        let canonical = default_document_attribute_values();
        if self
            .base
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &canonical))
        {
            self.base = Some(base);
            return;
        }

        let current = Arc::make_mut(self.base.get_or_insert_with(|| {
            Arc::new(FxHashMap::with_capacity_and_hasher(16, FxBuildHasher))
        }));
        for (name, value) in base.iter() {
            if is_intrinsic(name) {
                current.insert(name.clone(), value.clone());
            }
        }
    }

    /// Remove explicit assignments without changing shared base values.
    pub(crate) fn remove_explicit_with_prefix(&mut self, prefix: &str) {
        let names: Vec<_> = self
            .entries
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .map(|(name, _)| name)
            .cloned()
            .collect();
        for name in names {
            self.entries.remove(name.as_ref());
        }
        self.rebuild_entry_filter();
    }

    /// Remove one explicit assignment without changing shared base values.
    pub(crate) fn remove_explicit(&mut self, name: &str) {
        if self.entries.remove(name).is_some() {
            self.rebuild_entry_filter();
        }
    }

    fn assign(
        &mut self,
        name: AttributeName<'a>,
        request: AssignmentRequest<'a>,
    ) -> Result<Option<DocumentAttributeAssignment<'a>>, Error> {
        let current = self.entries.get(name.as_ref()).map(|entry| {
            (
                entry.state,
                matches!(entry.assignment, DocumentAttributeAssignment::Set(_)),
            )
        });
        if assignment_decision(&name, current, &request) == AssignmentDecision::Reject {
            return Ok(None);
        }
        let entry = validate_assignment_value(&name, request.raw)
            .map_err(|invalid| invalid.into_error(&name, request.source_location))?;
        self.set_entry(name, entry.clone(), request.state);
        Ok(Some(entry))
    }

    /// Whether an explicit assignment exists, including an unset assignment.
    #[must_use]
    pub fn is_explicit(&self, name: &str) -> bool {
        self.entry(name).is_some()
    }

    pub(crate) fn locks_nested_attribute(&self, name: &str) -> bool {
        crate::document_attribute::nested_attribute_is_inherited(name)
            && (self.contains_key(name)
                || self
                    .entry(name)
                    .is_some_and(|entry| entry.state.lock == AttributeLock::Locked))
    }

    #[cfg(test)]
    fn assignment_state(&self, name: &str) -> Option<AssignmentState> {
        self.entries.get(name).map(|entry| entry.state)
    }

    pub(crate) fn processor_convenience_value(&self, prefix: &str) -> Option<String> {
        self.entries
            .iter()
            .filter_map(|(name, entry)| {
                (entry.state.origin == AttributeOrigin::Processor
                    && entry.assignment.value().is_some())
                .then(|| name.strip_prefix(prefix))
                .flatten()
                .filter(|suffix| !suffix.is_empty() && !suffix.contains('-'))
            })
            .min()
            .map(str::to_string)
    }

    /// Borrow the effective value, including referenceable defaults.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&DocumentAttributeValue<'_>> {
        if let Some(entry) = self.entry(name) {
            return match &entry.assignment {
                DocumentAttributeAssignment::Set(value) => Some(value),
                DocumentAttributeAssignment::Unset => {
                    self.base.as_ref()?;
                    default_value(name)
                }
            };
        }
        self.resolve_base(name)
    }

    /// Borrow an explicit assignment, distinguishing unset from absent.
    #[must_use]
    pub fn assignment(&self, name: &str) -> Option<&DocumentAttributeAssignment<'_>> {
        self.entry(name).map(|entry| &entry.assignment)
    }

    /// Iterate over explicit assignments, including unsets but not implicit defaults.
    pub fn assignments(&self) -> impl Iterator<Item = (&str, &DocumentAttributeAssignment<'_>)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_ref(), &entry.assignment))
    }

    pub(crate) fn status(&self, name: &str) -> DocumentAttributeStatus<'_> {
        if let Some(entry) = self.entry(name) {
            return match &entry.assignment {
                DocumentAttributeAssignment::Set(value) => DocumentAttributeStatus::Set(value),
                DocumentAttributeAssignment::Unset => DocumentAttributeStatus::Unset,
            };
        }
        self.resolve_base(name).map_or(
            DocumentAttributeStatus::Absent,
            DocumentAttributeStatus::Set,
        )
    }

    fn resolve_base(&self, name: &str) -> Option<&DocumentAttributeValue<'_>> {
        let base = self.base.as_ref()?;
        base.get(name).or_else(|| default_value(name))
    }

    pub(crate) fn text(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(DocumentAttributeValue::text)
    }

    pub(crate) fn stored_text(&self, name: &str) -> Option<&Cow<'a, str>> {
        if let Some(entry) = self.entry(name) {
            return entry.assignment.stored_text();
        }
        self.base.as_ref()?.get(name)?.stored_text()
    }

    pub(crate) fn write_text<W: fmt::Write>(
        &self,
        name: &str,
        output: &mut W,
    ) -> Result<bool, fmt::Error> {
        let Some(value) = self.get(name) else {
            return Ok(false);
        };
        value.write_text(output)?;
        Ok(true)
    }

    /// Return whether the attribute has an effective value.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Remove stored values for a name. A synthesized registry default may remain.
    pub(crate) fn remove(&mut self, name: &str) -> Option<DocumentAttributeValue<'a>> {
        let explicit = self.entries.remove(name);
        let removed_explicit = explicit.is_some();
        let explicit = explicit.and_then(|entry| match entry.assignment {
            DocumentAttributeAssignment::Set(value) => Some(value),
            DocumentAttributeAssignment::Unset => None,
        });
        if removed_explicit {
            self.rebuild_entry_filter();
        }
        // Avoid cloning the shared base map when the requested value is absent.
        let base = if self
            .base
            .as_ref()
            .is_some_and(|base| base.contains_key(name))
        {
            self.base
                .as_mut()
                .and_then(|base| Arc::make_mut(base).remove(name))
        } else {
            None
        };
        explicit.or(base)
    }

    fn effective_default(&self, name: &str) -> Option<&'static DocumentAttributeValue<'static>> {
        if self.base.is_none()
            || self.entries.contains_key(name)
            || self
                .base
                .as_ref()
                .is_some_and(|base| base.contains_key(name))
        {
            return None;
        }
        default_value(name)
    }

    pub(crate) fn serialization_is_empty(&self) -> bool {
        self.entries
            .values()
            .all(|entry| matches!(entry.assignment, DocumentAttributeAssignment::Unset))
    }

    /// Merge processor defaults without replacing explicit input, except for
    /// protected processor attributes. Incoming assignments become processor values.
    pub(crate) fn merge(&mut self, other: Self) {
        let Self {
            entries: other_entries,
            base,
            entry_filter: _,
        } = other;
        if let Some(base) = base {
            if self.base.is_none() {
                self.base = Some(base);
            } else if !self
                .base
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &base))
            {
                for (name, value) in base.iter() {
                    if !self.entries.contains_key(name.as_ref())
                        && !self
                            .base
                            .as_ref()
                            .is_some_and(|current| current.contains_key(name.as_ref()))
                    {
                        Arc::make_mut(self.base.get_or_insert_with(|| {
                            Arc::new(FxHashMap::with_capacity_and_hasher(16, FxBuildHasher))
                        }))
                        .insert(name.clone(), value.clone());
                    }
                }
            }
        }
        for (name, mut entry) in other_entries {
            let Some(state) =
                processor_assignment_state(&name, self.entries.contains_key(name.as_ref()))
            else {
                continue;
            };
            entry.state = state;
            self.entries.insert(name, entry);
        }
        self.rebuild_entry_filter();
    }

    /// Clone the attributes into an independent `'static` copy. Used by
    /// converters that cache document attributes on a processor whose
    /// lifetime is independent of the document being rendered.
    #[must_use]
    pub fn to_static(&self) -> DocumentAttributes<'static> {
        DocumentAttributes {
            entries: self
                .entries
                .iter()
                .map(|(name, entry)| {
                    (
                        Cow::Owned(name.to_string()),
                        StoredAssignment {
                            assignment: entry.assignment.to_static(),
                            state: entry.state,
                        },
                    )
                })
                .collect(),
            base: self.base.clone(),
            entry_filter: self.entry_filter.clone(),
        }
    }

    /// Consume the attributes, producing an independent `'static` copy.
    #[must_use]
    pub fn into_static(self) -> DocumentAttributes<'static> {
        let Self {
            entries,
            base,
            entry_filter: _,
        } = self;
        let entries = entries
            .into_iter()
            .map(|(name, entry)| {
                (
                    Cow::Owned(name.into_owned()),
                    StoredAssignment {
                        assignment: entry.assignment.into_static(),
                        state: entry.state,
                    },
                )
            })
            .collect();
        let mut attributes = DocumentAttributes {
            entries,
            base,
            entry_filter: EntryFilter::default(),
        };
        attributes.rebuild_entry_filter();
        attributes
    }
}

impl Serialize for DocumentAttributes<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut entries: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, entry)| matches!(entry.assignment, DocumentAttributeAssignment::Set(_)))
            .collect();
        entries.sort_by_key(|(name, _)| *name);

        let mut state = serializer.serialize_map(Some(entries.len()))?;
        for (name, entry) in entries {
            state.serialize_entry(name, &entry.assignment.serialized_value(name == "toc"))?;
        }
        state.end()
    }
}

#[cfg(test)]
mod document_attribute_tests {
    use super::*;
    use crate::document_attribute::{AttributeLock, AttributeOrigin, MAX_INCLUDE_DEPTH_ATTR};
    use serde_json::{Error as JsonError, json, to_value};

    #[test]
    fn owned_attribute_text_and_names_survive_consuming_conversion_without_copying() {
        let name = String::from("owned-name");
        let value = String::from("owned attribute value");
        let name_pointer = name.as_ptr();
        let value_pointer = value.as_ptr();
        let mut attributes = DocumentAttributes::empty();
        assert!(
            attributes
                .set(Cow::Owned(name), AttributeValue::String(Cow::Owned(value)))
                .is_ok()
        );
        assert_eq!(
            attributes.text("owned-name").map(str::as_ptr),
            Some(value_pointer)
        );
        let attributes = attributes.into_static();
        assert_eq!(
            attributes.text("owned-name").map(str::as_ptr),
            Some(value_pointer)
        );
        assert_eq!(
            attributes
                .iter()
                .find(|(name, _)| *name == "owned-name")
                .map(|(name, _)| name.as_ptr()),
            Some(name_pointer)
        );
    }

    #[test]
    fn captions_are_default_attributes_but_numbering_depths_are_not() {
        let attributes = DocumentAttributes::default();

        assert_eq!(attributes.text("example-caption"), Some("Example"));
        assert_eq!(attributes.text("figure-caption"), Some("Figure"));
        assert_eq!(attributes.text("table-caption"), Some("Table"));
        assert_eq!(attributes.get("listing-caption"), None);
        assert_eq!(attributes.get("toclevels"), None);
        assert_eq!(attributes.get("sectnumlevels"), None);
    }

    #[test]
    fn effective_defaults_obey_the_public_map_laws() -> Result<(), JsonError> {
        let attributes = DocumentAttributes::default();

        assert_eq!(
            attributes
                .get(MAX_INCLUDE_DEPTH_ATTR)
                .and_then(DocumentAttributeValue::as_integer),
            Some(64)
        );
        assert!(attributes.contains_key(MAX_INCLUDE_DEPTH_ATTR));
        assert_eq!(
            attributes
                .iter()
                .filter(|(name, _)| *name == MAX_INCLUDE_DEPTH_ATTR)
                .count(),
            1
        );
        assert!(!attributes.is_empty());
        assert_eq!(to_value(&attributes)?, json!({}));

        let static_attributes = attributes.into_static();
        assert_eq!(
            static_attributes
                .get(MAX_INCLUDE_DEPTH_ATTR)
                .and_then(DocumentAttributeValue::as_integer),
            Some(64)
        );
        Ok(())
    }

    #[test]
    fn empty_document_attributes_do_not_synthesize_defaults() -> Result<(), JsonError> {
        let attributes = DocumentAttributes::empty();

        assert_eq!(attributes.get(MAX_INCLUDE_DEPTH_ATTR), None);
        assert!(!attributes.contains_key(MAX_INCLUDE_DEPTH_ATTR));
        assert_eq!(attributes.iter().count(), 0);
        assert!(attributes.is_empty());
        assert_eq!(to_value(&attributes)?, json!({}));

        let static_attributes = attributes.into_static();
        assert_eq!(static_attributes.get(MAX_INCLUDE_DEPTH_ATTR), None);
        Ok(())
    }

    #[test]
    fn explicit_max_include_depth_uses_normal_map_semantics() -> Result<(), JsonError> {
        let mut attributes = DocumentAttributes::default();
        assert!(
            attributes
                .set(MAX_INCLUDE_DEPTH_ATTR.into(), "8".into())
                .is_ok()
        );

        assert_eq!(attributes.text(MAX_INCLUDE_DEPTH_ATTR), Some("8"));
        assert!(attributes.contains_key(MAX_INCLUDE_DEPTH_ATTR));
        assert_eq!(
            attributes
                .iter()
                .find(|(name, _)| *name == MAX_INCLUDE_DEPTH_ATTR)
                .map(|(_, value)| value)
                .and_then(DocumentAttributeValue::as_integer),
            Some(8)
        );
        assert!(!attributes.is_empty());
        assert_eq!(to_value(&attributes)?, json!({ "max-include-depth": "8" }));
        Ok(())
    }

    #[test]
    fn unset_max_include_depth_values_are_not_serialized() -> Result<(), JsonError> {
        for value in [AttributeValue::Bool(false), AttributeValue::None] {
            let mut attributes = DocumentAttributes::default();
            assert!(
                attributes
                    .set(MAX_INCLUDE_DEPTH_ATTR.into(), value.clone())
                    .is_ok()
            );

            assert_eq!(
                attributes
                    .get(MAX_INCLUDE_DEPTH_ATTR)
                    .and_then(DocumentAttributeValue::as_integer),
                Some(64)
            );
            assert!(attributes.contains_key(MAX_INCLUDE_DEPTH_ATTR));
            assert_eq!(
                attributes
                    .iter()
                    .find(|(name, _)| *name == MAX_INCLUDE_DEPTH_ATTR)
                    .map(|(_, stored)| stored)
                    .and_then(DocumentAttributeValue::as_integer),
                Some(64)
            );
            assert!(!attributes.is_empty());
            assert_eq!(to_value(&attributes)?, json!({}));
        }
        Ok(())
    }

    #[test]
    fn unset_values_are_excluded_from_effective_lookup_and_iteration() {
        let mut attributes = DocumentAttributes::empty();
        assert!(
            attributes
                .set("false".into(), AttributeValue::Bool(false))
                .is_ok()
        );
        assert!(attributes.set("none".into(), AttributeValue::None).is_ok());

        for name in ["false", "none"] {
            assert_eq!(attributes.get(name), None);
            assert!(!attributes.contains_key(name));
            assert!(attributes.iter().all(|(candidate, _)| candidate != name));
            assert!(attributes.is_explicit(name));
        }
        assert!(attributes.is_empty());
        assert_eq!(
            attributes
                .entries
                .values()
                .filter(|entry| { matches!(entry.assignment, DocumentAttributeAssignment::Unset) })
                .count(),
            2
        );
    }

    #[test]
    fn value_and_text_accessors_keep_distinct_values() {
        let mut attributes = DocumentAttributes::default();
        assert!(
            attributes
                .set(MAX_INCLUDE_DEPTH_ATTR.into(), "064".into())
                .is_ok()
        );
        assert!(
            attributes
                .set("present".into(), AttributeValue::Bool(true))
                .is_ok()
        );
        assert!(attributes.set("empty-text".into(), "".into()).is_ok());
        assert!(attributes.set("quoted".into(), "\"value\"".into()).is_ok());

        assert_eq!(
            attributes
                .get(MAX_INCLUDE_DEPTH_ATTR)
                .and_then(DocumentAttributeValue::as_integer),
            Some(64)
        );
        assert_eq!(attributes.text(MAX_INCLUDE_DEPTH_ATTR), Some("064"));
        assert_eq!(attributes.text("quoted"), Some("\"value\""));
        assert!(
            attributes
                .get("present")
                .is_some_and(DocumentAttributeValue::is_presence)
        );
        assert_eq!(attributes.text("present"), None);
        assert_eq!(
            attributes
                .get("empty-text")
                .and_then(|value| value.as_str()),
            Some("")
        );
        assert!(attributes.contains_key("empty-text"));

        assert!(
            attributes
                .set(MAX_INCLUDE_DEPTH_ATTR.into(), "0".into())
                .is_ok()
        );
        assert_eq!(
            attributes
                .get(MAX_INCLUDE_DEPTH_ATTR)
                .and_then(DocumentAttributeValue::as_integer),
            Some(0)
        );
        assert!(attributes.contains_key(MAX_INCLUDE_DEPTH_ATTR));

        let default = DocumentAttributes::default();
        assert_eq!(
            default
                .get(MAX_INCLUDE_DEPTH_ATTR)
                .and_then(DocumentAttributeValue::as_integer),
            Some(64)
        );
        assert_eq!(default.text(MAX_INCLUDE_DEPTH_ATTR), None);
        let mut rendered = String::new();
        assert!(matches!(
            default.write_text(MAX_INCLUDE_DEPTH_ATTR, &mut rendered),
            Ok(true)
        ));
        assert_eq!(rendered, "64");
        rendered.clear();
        assert!(matches!(
            attributes.write_text("present", &mut rendered),
            Ok(true)
        ));
        assert!(rendered.is_empty());
    }

    #[test]
    fn caller_and_document_assignments_retain_private_provenance() -> Result<(), Error> {
        let mut attributes = DocumentAttributes::default();
        assert!(attributes.set("name".into(), "processor".into()).is_ok());
        assert_eq!(
            attributes.assignment_state("name"),
            Some(AssignmentState::PROCESSOR)
        );

        let mut attributes =
            crate::Options::with_attributes(attributes.into_inputs())?.document_attributes;
        assert_eq!(
            attributes
                .assignment_state("name")
                .map(|state| state.origin),
            Some(AttributeOrigin::Caller)
        );
        assert_eq!(
            attributes.assignment_state("name").map(|state| state.lock),
            Some(AttributeLock::Locked)
        );

        assert!(
            attributes
                .assign_document_value(
                    "name".into(),
                    RawAttributeValue::Text("document".into()),
                    true,
                    false,
                    None,
                )?
                .is_none()
        );
        assert_eq!(attributes.text("name"), Some("processor"));

        assert!(
            attributes
                .assign_document_value(
                    "other".into(),
                    RawAttributeValue::Text("document".into()),
                    false,
                    false,
                    None,
                )?
                .is_some()
        );
        assert_eq!(
            attributes
                .assignment_state("other")
                .map(|state| state.origin),
            Some(AttributeOrigin::Document)
        );
        assert_eq!(
            attributes.assignment_state("other").map(|state| state.lock),
            Some(AttributeLock::Unlocked)
        );
        Ok(())
    }

    #[test]
    fn merge_preserves_processor_origin_and_numeric_value() {
        let mut processor = DocumentAttributes::default();
        assert!(
            processor
                .set(MAX_INCLUDE_DEPTH_ATTR.into(), "064".into())
                .is_ok()
        );

        let mut attributes = DocumentAttributes::default();
        attributes.merge(processor);

        assert_eq!(
            attributes.assignment_state(MAX_INCLUDE_DEPTH_ATTR),
            Some(AssignmentState::PROCESSOR)
        );
        assert_eq!(
            attributes
                .get(MAX_INCLUDE_DEPTH_ATTR)
                .and_then(DocumentAttributeValue::as_integer),
            Some(64)
        );
        assert_eq!(attributes.text(MAX_INCLUDE_DEPTH_ATTR), Some("064"));
    }

    #[test]
    fn document_attributes_track_clones_merges_and_removed_assignments() {
        let mut attributes = DocumentAttributes::default();
        assert!(attributes.set("name".into(), "header".into()).is_ok());
        let header = attributes.clone();

        assert!(attributes.set("name".into(), "body".into()).is_ok());
        assert_eq!(header.text("name"), Some("header"));
        assert_eq!(attributes.text("name"), Some("body"));

        let mut processor = DocumentAttributes::empty();
        assert!(processor.set("merged".into(), "value".into()).is_ok());
        attributes.merge(processor);
        assert_eq!(attributes.text("merged"), Some("value"));

        assert!(
            attributes
                .set("figure-caption".into(), false.into())
                .is_ok()
        );
        assert_eq!(attributes.text("figure-caption"), None);
        attributes.remove_explicit("figure-caption");
        assert_eq!(attributes.text("figure-caption"), Some("Figure"));
    }

    #[test]
    fn processor_convenience_value_is_deterministic() {
        let mut attributes = DocumentAttributes::default();
        assert!(attributes.set("filetype-zeta".into(), true.into()).is_ok());
        assert!(attributes.set("filetype-alpha".into(), true.into()).is_ok());

        assert_eq!(
            attributes.processor_convenience_value("filetype-"),
            Some("alpha".to_string())
        );
    }

    #[test]
    fn configuration_validation_rejects_invalid_inputs() -> Result<(), Error> {
        let inputs =
            crate::Options::builder().with_default_attribute(MAX_INCLUDE_DEPTH_ATTR, "2junk");
        assert!(matches!(
            inputs.clone().build(),
            Err(Error::InvalidDocumentAttribute {
                ref name,
                ref value,
                expected: "a complete non-negative integer",
                location: None,
            }) if name == MAX_INCLUDE_DEPTH_ATTR && value == "2junk"
        ));

        let attributes = inputs
            .with_default_attribute(MAX_INCLUDE_DEPTH_ATTR, "02")
            .build()?
            .into_document_attributes();
        assert_eq!(
            attributes
                .get(MAX_INCLUDE_DEPTH_ATTR)
                .and_then(DocumentAttributeValue::as_integer),
            Some(2)
        );
        assert_eq!(attributes.text(MAX_INCLUDE_DEPTH_ATTR), Some("02"));
        Ok(())
    }
}

/// Element-level attributes (for blocks, sections, etc.).
///
/// These attributes are specific to individual elements and start empty.
///
/// Use `ElementAttributes::default()` to get an empty attribute map.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct ElementAttributes<'a>(FxHashMap<AttributeName<'a>, AttributeValue<'a>>);

impl<'a> ElementAttributes<'a> {
    /// Iterate over all attributes.
    pub fn iter(&self) -> impl Iterator<Item = (&AttributeName<'a>, &AttributeValue<'a>)> {
        self.0.iter()
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut AttributeValue<'a>> {
        self.0.values_mut()
    }

    /// Check if the attribute map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Insert an attribute without replacing an existing value.
    pub fn insert(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        self.0.entry(name).or_insert(value);
    }

    /// Set an attribute, overwriting any existing value.
    pub fn set(&mut self, name: AttributeName<'a>, value: AttributeValue<'a>) {
        self.0.insert(name, value);
    }

    /// Get an attribute value by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&AttributeValue<'a>> {
        self.0.get(name)
    }

    /// Check if an attribute exists.
    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// Remove an attribute by name.
    pub fn remove(&mut self, name: &str) -> Option<AttributeValue<'a>> {
        self.0.remove(name)
    }

    /// Merge attributes without replacing existing values.
    pub fn merge(&mut self, other: Self) {
        for (key, value) in other.0 {
            self.insert(key, value);
        }
    }

    /// Convert all borrowed content to owned, producing `'static` lifetime attributes.
    #[must_use]
    pub fn into_static(self) -> ElementAttributes<'static> {
        ElementAttributes(
            self.0
                .into_iter()
                .map(|(k, v)| {
                    let key: AttributeName<'static> = Cow::Owned(k.into_owned());
                    let val = match v {
                        AttributeValue::String(s) => {
                            AttributeValue::String(Cow::Owned(s.into_owned()))
                        }
                        AttributeValue::Bool(b) => AttributeValue::Bool(b),
                        AttributeValue::None => AttributeValue::None,
                    };
                    (key, val)
                })
                .collect(),
        )
    }

    /// Get a string attribute value as an owned `String`.
    ///
    /// Strips surrounding quotes from the value if present.
    #[must_use]
    pub fn get_string(&self, name: &str) -> Option<Cow<'a, str>> {
        self.get(name).and_then(|v| match v {
            AttributeValue::String(s) => Some(match s {
                Cow::Borrowed(b) => Cow::Borrowed(strip_quotes(b)),
                Cow::Owned(o) => Cow::Owned(strip_quotes(o).to_string()),
            }),
            AttributeValue::None | AttributeValue::Bool(_) => None,
        })
    }
}

impl Serialize for ElementAttributes<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut entries: Vec<_> = self.0.iter().collect();
        entries.sort_by_key(|(key, _)| *key);

        let mut state = serializer.serialize_map(Some(entries.len()))?;
        for (key, value) in entries {
            match value {
                AttributeValue::Bool(true) if key == "toc" => {
                    state.serialize_entry(key, "")?;
                }
                AttributeValue::Bool(true) => {
                    state.serialize_entry(key, &true)?;
                }
                AttributeValue::Bool(false) | AttributeValue::String(_) | AttributeValue::None => {
                    state.serialize_entry(key, value)?;
                }
            }
        }
        state.end()
    }
}

#[cfg(test)]
mod element_attribute_tests {
    use super::{AttributeValue, ElementAttributes};
    use serde_json::{Error, json, to_value};

    #[test]
    fn insert_set_and_merge_preserve_existing_value_rules() -> Result<(), Error> {
        let mut attributes = ElementAttributes::default();
        assert!(attributes.is_empty());
        attributes.insert("role".into(), "first".into());
        attributes.insert("role".into(), "ignored".into());
        assert_eq!(attributes.get_string("role").as_deref(), Some("first"));

        attributes.set("role".into(), "replacement".into());
        let mut defaults = ElementAttributes::default();
        defaults.set("role".into(), "default".into());
        defaults.set("width".into(), "50%".into());
        attributes.merge(defaults);
        assert_eq!(attributes.iter().count(), 2);
        assert_eq!(
            to_value(&attributes)?,
            json!({"role": "replacement", "width": "50%"})
        );

        let snapshot = attributes.clone().into_static();
        assert_eq!(snapshot, attributes);
        assert_eq!(attributes.remove("role"), Some("replacement".into()));
        assert!(!attributes.contains_key("role"));
        assert_eq!(attributes.remove("width"), Some("50%".into()));
        assert_eq!(attributes.remove("missing"), None);
        assert!(attributes.is_empty());
        assert_eq!(to_value(&attributes)?, json!({}));
        assert_eq!(snapshot.iter().count(), 2);
        Ok(())
    }

    #[test]
    fn serialization_preserves_all_value_forms_and_sorted_names() -> Result<(), Error> {
        let mut attributes = ElementAttributes::default();
        for (name, value) in [
            ("text", "value".into()),
            ("toc", AttributeValue::Bool(true)),
            ("present", AttributeValue::Bool(true)),
            ("false", AttributeValue::Bool(false)),
            ("none", AttributeValue::None),
        ] {
            attributes.set(name.into(), value);
        }
        let expected = r#"{"false":false,"none":null,"present":true,"text":"value","toc":""}"#;
        assert_eq!(serde_json::to_string(&attributes)?, expected);
        assert_eq!(serde_json::to_string(&attributes.into_static())?, expected);
        Ok(())
    }
}

/// An `AttributeName` represents the name of an attribute in a document.
pub type AttributeName<'a> = Cow<'a, str>;

/// An `AttributeValue` represents the value of an attribute in a document.
///
/// An attribute value can be a string, a boolean, or nothing
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum AttributeValue<'a> {
    /// A string attribute value.
    String(Cow<'a, str>),
    /// A boolean attribute value. `false` means it is unset.
    Bool(bool),
    /// No value (or it was unset)
    None,
}

impl std::fmt::Display for AttributeValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttributeValue::String(value) => write!(f, "{value}"),
            AttributeValue::Bool(value) => write!(f, "{value}"),
            AttributeValue::None => write!(f, "null"),
        }
    }
}

impl<'a> From<&'a str> for AttributeValue<'a> {
    fn from(value: &'a str) -> Self {
        AttributeValue::String(Cow::Borrowed(value))
    }
}

impl From<String> for AttributeValue<'_> {
    fn from(value: String) -> Self {
        AttributeValue::String(Cow::Owned(value))
    }
}

impl From<bool> for AttributeValue<'_> {
    fn from(value: bool) -> Self {
        AttributeValue::Bool(value)
    }
}

impl From<()> for AttributeValue<'_> {
    fn from((): ()) -> Self {
        AttributeValue::None
    }
}
