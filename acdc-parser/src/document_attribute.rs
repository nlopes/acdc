//! Internal document-attribute registry and assignment policy.
//!
//! Public document values use [`DocumentAttributeValue`]. This module owns the
//! private inputs needed to decide an assignment without coupling element
//! attributes to document policy.

use std::{
    borrow::Cow,
    cell::RefCell,
    path::Path,
    sync::{Arc, LazyLock},
};

use chrono::{DateTime, Local, TimeZone, Utc};
use rustc_hash::FxHashMap;

use crate::{
    AttributeName, AttributeValue, DocumentAttributeAssignment, DocumentAttributeValue,
    DocumentAttributes, Error, SafeMode, SourceLocation, model::RawAttributes,
};

pub(crate) const DEFAULT_MAX_INCLUDE_DEPTH: i128 = 64;
pub(crate) const MAX_INCLUDE_DEPTH_ATTR: &str = "max-include-depth";
static DEFAULT_MAX_INCLUDE_DEPTH_VALUE: DocumentAttributeValue<'static> =
    DocumentAttributeValue::integer(DEFAULT_MAX_INCLUDE_DEPTH);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttributeOrigin {
    Processor,
    Caller,
    Document,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttributeLock {
    Unlocked,
    Locked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AssignmentState {
    pub(crate) origin: AttributeOrigin,
    pub(crate) lock: AttributeLock,
}

impl AssignmentState {
    pub(crate) const PROCESSOR: Self = Self {
        origin: AttributeOrigin::Processor,
        lock: AttributeLock::Unlocked,
    };

    pub(crate) const PROCESSOR_LOCKED: Self = Self {
        lock: AttributeLock::Locked,
        ..Self::PROCESSOR
    };

    pub(crate) const CALLER_LOCKED: Self = Self {
        origin: AttributeOrigin::Caller,
        lock: AttributeLock::Locked,
    };

    pub(crate) const fn document() -> Self {
        Self {
            origin: AttributeOrigin::Document,
            lock: AttributeLock::Unlocked,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssignmentPolicy {
    Modifiable,
    ApiOnly,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AttributeSpec {
    pub(crate) default: Option<&'static DocumentAttributeValue<'static>>,
    pub(crate) assignment_policy: AssignmentPolicy,
    pub(crate) intrinsic: bool,
}

const TEXT_MODIFIABLE: AttributeSpec = AttributeSpec {
    default: None,
    assignment_policy: AssignmentPolicy::Modifiable,
    intrinsic: false,
};
const TEXT_API_ONLY: AttributeSpec = AttributeSpec {
    assignment_policy: AssignmentPolicy::ApiOnly,
    ..TEXT_MODIFIABLE
};
const TEXT_READ_ONLY: AttributeSpec = AttributeSpec {
    assignment_policy: AssignmentPolicy::ReadOnly,
    ..TEXT_MODIFIABLE
};
const INTRINSIC_API_ONLY: AttributeSpec = AttributeSpec {
    intrinsic: true,
    ..TEXT_API_ONLY
};
const INTRINSIC_MODIFIABLE: AttributeSpec = AttributeSpec {
    intrinsic: true,
    ..TEXT_MODIFIABLE
};
const INTRINSIC_READ_ONLY: AttributeSpec = AttributeSpec {
    intrinsic: true,
    ..TEXT_READ_ONLY
};
const POST_CONVERSION_READ_ONLY: AttributeSpec = INTRINSIC_READ_ONLY;
const MAX_INCLUDE_DEPTH: AttributeSpec = AttributeSpec {
    default: Some(&DEFAULT_MAX_INCLUDE_DEPTH_VALUE),
    assignment_policy: AssignmentPolicy::ApiOnly,
    intrinsic: false,
};

const READ_ONLY_EXACT: &[&str] = &["asciidoctor", "asciidoctor-version"];
const API_ONLY_EXACT: &[&str] = &[
    "allow-uri-read",
    "max-attribute-value-size",
    "skip-front-matter",
];
const INTRINSIC_HEADER_LISTED_EXACT: &[&str] = &[
    "backend",
    "docdate",
    "docdatetime",
    "doctime",
    "docyear",
    "localdate",
    "localdatetime",
    "localtime",
    "localyear",
    "outfilesuffix",
];
const READ_ONLY_PREFIXES: &[&str] = &[
    "backend-",
    "basebackend-",
    "doctype-",
    "filetype-",
    "safe-mode-",
];

/// Look up a static specification without allocating.
pub(crate) fn attribute_spec(name: &str) -> AttributeSpec {
    if let Some(spec) = default_spec(name) {
        spec
    } else if let Some(spec) = intrinsic_spec(name) {
        spec
    } else if READ_ONLY_EXACT.contains(&name) {
        TEXT_READ_ONLY
    } else if API_ONLY_EXACT.contains(&name) {
        TEXT_API_ONLY
    } else {
        // Unregistered user and converter attributes remain modifiable opaque text.
        debug_assert!(!name.is_empty());
        TEXT_MODIFIABLE
    }
}

fn default_spec(name: &str) -> Option<AttributeSpec> {
    (name == MAX_INCLUDE_DEPTH_ATTR).then_some(MAX_INCLUDE_DEPTH)
}

fn intrinsic_spec(name: &str) -> Option<AttributeSpec> {
    if matches!(name, "outdir" | "outfile") {
        Some(POST_CONVERSION_READ_ONLY)
    } else if INTRINSIC_HEADER_LISTED_EXACT.contains(&name) {
        Some(INTRINSIC_MODIFIABLE)
    } else if matches!(
        name,
        "basebackend"
            | "embedded"
            | "htmlsyntax"
            | "safe-mode-level"
            | "safe-mode-name"
            | "user-home"
    ) || READ_ONLY_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        Some(INTRINSIC_READ_ONLY)
    } else if matches!(
        name,
        "docdir" | "docfile" | "docfilesuffix" | "docname" | "filetype"
    ) {
        Some(INTRINSIC_API_ONLY)
    } else {
        None
    }
}

pub(crate) fn default_value(name: &str) -> Option<&'static DocumentAttributeValue<'static>> {
    default_spec(name).and_then(|spec| spec.default)
}

pub(crate) fn is_intrinsic(name: &str) -> bool {
    attribute_spec(name).intrinsic
}

const BACKEND_FLAG_PREFIXES: [&str; 4] = ["backend-", "basebackend-", "doctype-", "filetype-"];

fn is_doctype_flag(name: &str) -> bool {
    name.starts_with("doctype-")
        || ((name.starts_with("backend-") || name.starts_with("basebackend-"))
            && name.contains("-doctype-"))
}

fn backend_owns_attribute(name: &str) -> bool {
    matches!(
        name,
        "backend" | "basebackend" | "filetype" | "htmlsyntax" | "embedded"
    ) || BACKEND_FLAG_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn initialize_convenience_attributes(defaults: &mut RawAttributes<'_>) {
    let doctype = defaults.get("doctype").and_then(|value| match value {
        AttributeValue::String(value) => Some(value.clone()),
        AttributeValue::Bool(_) | AttributeValue::None => None,
    });
    if defaults.contains_key("doctype") {
        defaults.retain(|name, _| !is_doctype_flag(name));
    }
    for name in ["backend", "basebackend", "filetype", "doctype"] {
        let Some(value) = defaults.get(name).cloned() else {
            continue;
        };
        let prefix = format!("{name}-");
        defaults.retain(|key, _| !key.starts_with(&prefix));
        if let AttributeValue::String(value) = value {
            defaults.insert(format!("{name}-{value}").into(), "".into());
            if matches!(name, "backend" | "basebackend")
                && let Some(doctype) = &doctype
            {
                defaults.insert(
                    format!("{name}-{value}-doctype-{doctype}").into(),
                    "".into(),
                );
            }
        }
    }
}

/// Resolve caller precedence and processor-derived flags before parsing starts.
pub(crate) fn initialize_configuration<'a>(
    mut caller: RawAttributes<'a>,
    mut defaults: RawAttributes<'a>,
) -> Result<DocumentAttributes<'a>, Error> {
    if defaults.contains_key("backend") {
        caller.retain(|name, _| !backend_owns_attribute(name));
    }
    initialize_convenience_attributes(&mut defaults);
    let mut attributes = DocumentAttributes::from_inputs(caller, AssignmentState::CALLER_LOCKED)?;
    attributes.merge(DocumentAttributes::from_inputs(
        defaults,
        AssignmentState::PROCESSOR,
    )?);
    Ok(attributes)
}

pub(crate) fn processor_assignment_state(
    name: &str,
    already_assigned: bool,
) -> Option<AssignmentState> {
    if name == "backend" {
        Some(AssignmentState::PROCESSOR_LOCKED)
    } else if !already_assigned
        || attribute_spec(name).assignment_policy == AssignmentPolicy::ReadOnly
    {
        Some(AssignmentState::PROCESSOR)
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RawAttributeValue<'a> {
    Text(Cow<'a, str>),
    Set,
    Unset,
}

impl<'a> From<AttributeValue<'a>> for RawAttributeValue<'a> {
    fn from(value: AttributeValue<'a>) -> Self {
        match value {
            AttributeValue::String(value) => Self::Text(value),
            AttributeValue::Bool(true) => Self::Set,
            AttributeValue::Bool(false) | AttributeValue::None => Self::Unset,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InvalidAttributeValue {
    pub(crate) value: String,
    pub(crate) expected: &'static str,
}

impl InvalidAttributeValue {
    pub(crate) fn into_error(self, name: &str, source_location: Option<SourceLocation>) -> Error {
        Error::InvalidDocumentAttribute {
            name: name.to_string(),
            value: self.value,
            expected: self.expected,
            location: source_location.map(Box::new),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssignmentDecision {
    Apply,
    Reject,
}

#[derive(Debug)]
pub(crate) struct AssignmentRequest<'a> {
    pub(crate) raw: RawAttributeValue<'a>,
    pub(crate) state: AssignmentState,
    pub(crate) in_header: bool,
    pub(crate) force_locked: bool,
    pub(crate) source_location: Option<SourceLocation>,
}

fn current_lock_rejects(
    name: &str,
    current: Option<(AssignmentState, bool)>,
    request: &AssignmentRequest<'_>,
) -> bool {
    if request.state.origin != AttributeOrigin::Document {
        return false;
    }
    if request.force_locked {
        return true;
    }
    let Some((current, current_is_set)) = current else {
        return false;
    };
    if current.lock == AttributeLock::Unlocked {
        return false;
    }

    // Asciidoctor lets a caller-set `sectnums` value become flexible after the
    // header, but a caller-requested unset remains locked.
    !(name == "sectnums" && !request.in_header && current_is_set)
}

pub(crate) fn assignment_decision(
    name: &str,
    current: Option<(AssignmentState, bool)>,
    request: &AssignmentRequest<'_>,
) -> AssignmentDecision {
    if matches!(name, "outdir" | "outfile") {
        return AssignmentDecision::Reject;
    }
    let spec = attribute_spec(name);
    if current_lock_rejects(name, current, request) {
        return AssignmentDecision::Reject;
    }
    if request.state.origin == AttributeOrigin::Document {
        match spec.assignment_policy {
            AssignmentPolicy::Modifiable => {}
            AssignmentPolicy::ApiOnly | AssignmentPolicy::ReadOnly => {
                return AssignmentDecision::Reject;
            }
        }
    }
    AssignmentDecision::Apply
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputKind<'a> {
    String,
    Reader,
    File(&'a Path),
}

fn source_date_epoch() -> Option<DateTime<Utc>> {
    static SOURCE_DATE_EPOCH: LazyLock<Option<i64>> =
        LazyLock::new(|| std::env::var("SOURCE_DATE_EPOCH").ok()?.parse::<i64>().ok());
    let seconds = (*SOURCE_DATE_EPOCH)?;
    Utc.timestamp_opt(seconds, 0).single()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimestampKey {
    seconds: i64,
    utc: bool,
}

struct CachedBaseAttributes {
    timestamp: TimestampKey,
    values: Arc<FxHashMap<AttributeName<'static>, DocumentAttributeValue<'static>>>,
}

thread_local! {
    static CACHED_BASE_ATTRIBUTES: RefCell<[Option<CachedBaseAttributes>; 4]> =
        const { RefCell::new([None, None, None, None]) };
}

const fn safe_mode_index(safe_mode: SafeMode) -> usize {
    match safe_mode {
        SafeMode::Unsafe => 0,
        SafeMode::Safe => 1,
        SafeMode::Server => 2,
        SafeMode::Secure => 3,
    }
}

fn build_cached_base_attributes(
    safe_mode: SafeMode,
    timestamp: DateTime<Utc>,
    utc: bool,
) -> Arc<FxHashMap<AttributeName<'static>, DocumentAttributeValue<'static>>> {
    let mut values = (*crate::model::default_document_attribute_values()).clone();
    values.insert(
        "safe-mode-level".into(),
        safe_mode.level().to_string().into(),
    );
    values.insert("safe-mode-name".into(), safe_mode.name().into());
    let convenience = format!("safe-mode-{}", safe_mode.name());
    values.insert(convenience.into(), "".into());
    let user_home = if safe_mode >= SafeMode::Server {
        Cow::Borrowed(".")
    } else {
        std::env::var("HOME").map_or(Cow::Borrowed("."), Cow::Owned)
    };
    values.insert("user-home".into(), DocumentAttributeValue::from(user_home));

    let timestamp = timestamp_values(timestamp, utc);
    for names in [
        LOCAL_TIMESTAMP_ATTRIBUTE_NAMES,
        DOC_TIMESTAMP_ATTRIBUTE_NAMES,
    ] {
        for (name, value) in timestamp_names(names).zip(timestamp.iter()) {
            values.insert(name.into(), value.clone().into());
        }
    }
    Arc::new(values)
}

fn cached_base_attributes(
    safe_mode: SafeMode,
) -> Arc<FxHashMap<AttributeName<'static>, DocumentAttributeValue<'static>>> {
    let epoch = source_date_epoch();
    let utc = epoch.is_some();
    let timestamp = epoch.unwrap_or_else(Utc::now);
    let key = TimestampKey {
        seconds: timestamp.timestamp(),
        utc,
    };
    CACHED_BASE_ATTRIBUTES.with(|cache| {
        let index = safe_mode_index(safe_mode);
        let mut cache = cache.borrow_mut();
        let Some(slot) = cache.get_mut(index) else {
            return build_cached_base_attributes(safe_mode, timestamp, utc);
        };
        if let Some(cached) = slot
            && cached.timestamp == key
        {
            return Arc::clone(&cached.values);
        }
        let values = build_cached_base_attributes(safe_mode, timestamp, utc);
        *slot = Some(CachedBaseAttributes {
            timestamp: key,
            values: Arc::clone(&values),
        });
        values
    })
}

fn timestamp_names(
    names: (&'static str, &'static str, &'static str, &'static str),
) -> impl Iterator<Item = &'static str> {
    [names.0, names.1, names.2, names.3].into_iter()
}

fn timestamp_values(time: DateTime<Utc>, utc: bool) -> [String; 4] {
    if utc {
        [
            time.format("%Y-%m-%d").to_string(),
            time.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            time.format("%H:%M:%S UTC").to_string(),
            time.format("%Y").to_string(),
        ]
    } else {
        let time = time.with_timezone(&Local);
        [
            time.format("%Y-%m-%d").to_string(),
            time.format("%Y-%m-%d %H:%M:%S %z").to_string(),
            time.format("%H:%M:%S %z").to_string(),
            time.format("%Y").to_string(),
        ]
    }
}

fn set_source_timestamp(attributes: &mut DocumentAttributes<'_>, time: DateTime<Utc>, utc: bool) {
    for (name, value) in
        timestamp_names(DOC_TIMESTAMP_ATTRIBUTE_NAMES).zip(timestamp_values(time, utc))
    {
        attributes.set_base_intrinsic(name.into(), value.into());
    }
}

const DOC_TIMESTAMP_ATTRIBUTE_NAMES: (&str, &str, &str, &str) =
    ("docdate", "docdatetime", "doctime", "docyear");
const LOCAL_TIMESTAMP_ATTRIBUTE_NAMES: (&str, &str, &str, &str) =
    ("localdate", "localdatetime", "localtime", "localyear");

fn initialize_safe_mode(attributes: &mut DocumentAttributes<'_>, safe_mode: SafeMode) {
    attributes.remove_explicit_with_prefix("safe-mode-");
    attributes.remove_explicit("user-home");
    attributes.set_base_map(cached_base_attributes(safe_mode));
}

fn initialize_source_metadata(
    attributes: &mut DocumentAttributes<'_>,
    path: &Path,
    safe_mode: SafeMode,
) {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let parent = absolute.parent().unwrap_or(Path::new(""));
    let file_name = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let docfile = if safe_mode >= SafeMode::Server {
        file_name.to_string()
    } else {
        absolute.to_string_lossy().into_owned()
    };
    let docdir = if safe_mode >= SafeMode::Server {
        String::new()
    } else {
        parent.to_string_lossy().into_owned()
    };
    attributes.set_intrinsic("docdir".into(), docdir.into());
    attributes.set_intrinsic("docfile".into(), docfile.into());
    attributes.set_intrinsic(
        "docfilesuffix".into(),
        path.extension()
            .and_then(|extension| extension.to_str())
            .map_or_else(String::new, |extension| format!(".{extension}"))
            .into(),
    );
    attributes.set_intrinsic(
        "docname".into(),
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("")
            .to_string()
            .into(),
    );

    let epoch = source_date_epoch();
    let source_time = epoch.or_else(|| {
        std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .map(DateTime::<Utc>::from)
    });
    if let Some(source_time) = source_time {
        set_source_timestamp(attributes, source_time, epoch.is_some());
    }
}

fn initialize_filetype(attributes: &mut DocumentAttributes<'_>, input_kind: InputKind<'_>) {
    let filetype = if input_kind == InputKind::String {
        attributes
            .text("filetype")
            .map(crate::strip_quotes)
            .map(str::to_owned)
    } else {
        attributes.processor_convenience_value("filetype-")
    };
    attributes.remove_explicit_with_prefix("filetype-");

    if input_kind != InputKind::String {
        attributes.remove("filetype");
        if let Some(filetype) = &filetype {
            attributes.set_intrinsic("filetype".into(), filetype.clone().into());
        }
    }
    if let Some(filetype) = filetype {
        attributes.set_intrinsic(format!("filetype-{filetype}").into(), "".into());
    }
}

pub(crate) fn initialize_intrinsics(
    attributes: &mut DocumentAttributes<'_>,
    safe_mode: SafeMode,
    input_kind: InputKind<'_>,
) {
    initialize_safe_mode(attributes, safe_mode);
    initialize_filetype(attributes, input_kind);

    match input_kind {
        InputKind::String => {}
        InputKind::Reader => {
            for name in ["docdir", "docfile", "docfilesuffix", "docname"] {
                attributes.remove(name);
            }
        }
        InputKind::File(path) => {
            initialize_source_metadata(attributes, path, safe_mode);
        }
    }
}

fn complete_non_negative_integer(value: &str) -> Option<i128> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(value.bytes().fold(0_i128, |number, digit| {
        number
            .saturating_mul(10)
            .saturating_add(i128::from(digit - b'0'))
    }))
}

pub(crate) fn validate_assignment_value<'a>(
    name: &str,
    raw: RawAttributeValue<'a>,
) -> Result<DocumentAttributeAssignment<'a>, InvalidAttributeValue> {
    let mut original_source_text = None;
    let assignment = match raw {
        RawAttributeValue::Unset => DocumentAttributeAssignment::Unset,
        RawAttributeValue::Text(value) if name != MAX_INCLUDE_DEPTH_ATTR => {
            DocumentAttributeAssignment::Set(DocumentAttributeValue::from(value))
        }
        RawAttributeValue::Set if name != MAX_INCLUDE_DEPTH_ATTR => {
            DocumentAttributeAssignment::Set(DocumentAttributeValue::presence())
        }
        RawAttributeValue::Set => {
            original_source_text = Some(Cow::Borrowed(""));
            DocumentAttributeAssignment::Set(DocumentAttributeValue::integer(0))
        }
        RawAttributeValue::Text(text) => {
            let Some(value) = complete_non_negative_integer(&text) else {
                return Err(InvalidAttributeValue {
                    value: text.into_owned(),
                    expected: "a complete non-negative integer",
                });
            };
            original_source_text = Some(text);
            DocumentAttributeAssignment::Set(DocumentAttributeValue::integer(value))
        }
    };
    Ok(DocumentAttributeAssignment::new(
        assignment,
        original_source_text,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_exact_and_generated_names() {
        assert_eq!(
            attribute_spec(MAX_INCLUDE_DEPTH_ATTR),
            AttributeSpec {
                default: Some(&DEFAULT_MAX_INCLUDE_DEPTH_VALUE),
                assignment_policy: AssignmentPolicy::ApiOnly,
                intrinsic: false,
            }
        );
        assert_eq!(
            attribute_spec("safe-mode-probe").assignment_policy,
            AssignmentPolicy::ReadOnly
        );
        assert_eq!(
            attribute_spec("converter-local").assignment_policy,
            AssignmentPolicy::Modifiable
        );
        assert_eq!(
            attribute_spec("outfile").assignment_policy,
            AssignmentPolicy::ReadOnly
        );
    }

    #[test]
    fn intrinsic_inventory_recognizes_processor_attributes() {
        let names = [
            "backend",
            "backend-html5",
            "basebackend",
            "basebackend-html",
            "doctype-article",
            "filetype",
            "filetype-html",
            "htmlsyntax",
            "outfilesuffix",
            "docdate",
            "docdatetime",
            "doctime",
            "docyear",
            "docdir",
            "docfile",
            "docfilesuffix",
            "docname",
            "localdate",
            "localdatetime",
            "localtime",
            "localyear",
            "embedded",
            "outdir",
            "outfile",
            "safe-mode-level",
            "safe-mode-name",
            "safe-mode-unsafe",
            "safe-mode-safe",
            "safe-mode-server",
            "safe-mode-secure",
            "user-home",
        ];

        assert_eq!(names.len(), 31);
        assert!(names.into_iter().all(is_intrinsic));
    }

    #[test]
    fn complete_non_negative_integer_rejects_the_full_invalid_value() {
        assert_eq!(complete_non_negative_integer(" 42 "), Some(42));
        assert_eq!(complete_non_negative_integer("42junk"), None);
        assert_eq!(complete_non_negative_integer("-1"), None);
        assert_eq!(complete_non_negative_integer(""), None);
    }

    #[test]
    fn assignment_state_equality_includes_origin_and_lock() {
        assert_ne!(AssignmentState::PROCESSOR, AssignmentState::document());
        assert_ne!(AssignmentState::PROCESSOR, AssignmentState::CALLER_LOCKED);
        assert!(format!("{:?}", AssignmentState::document()).contains("Document"));
    }

    #[test]
    fn policy_uses_origin_header_state_and_lock_as_separate_inputs() {
        let document_header = AssignmentRequest {
            raw: RawAttributeValue::Text("value".into()),
            state: AssignmentState::document(),
            in_header: true,
            force_locked: false,
            source_location: None,
        };
        let document_body = AssignmentRequest {
            raw: RawAttributeValue::Text("value".into()),
            state: AssignmentState::document(),
            in_header: false,
            force_locked: false,
            source_location: None,
        };

        assert_eq!(
            assignment_decision("safe-mode-name", None, &document_header),
            AssignmentDecision::Reject
        );
        assert_eq!(
            assignment_decision("ordinary", None, &document_header),
            AssignmentDecision::Apply
        );
        let processor = AssignmentRequest {
            raw: RawAttributeValue::Text("value".into()),
            state: AssignmentState::PROCESSOR,
            in_header: true,
            force_locked: false,
            source_location: None,
        };
        assert_eq!(
            assignment_decision("safe-mode-name", None, &processor),
            AssignmentDecision::Apply
        );
        let caller = AssignmentRequest {
            state: AssignmentState {
                origin: AttributeOrigin::Caller,
                lock: AttributeLock::Locked,
            },
            ..processor
        };
        assert_eq!(
            assignment_decision("safe-mode-name", None, &caller),
            AssignmentDecision::Apply
        );
        assert_eq!(
            assignment_decision(
                "ordinary",
                Some((
                    AssignmentState {
                        origin: AttributeOrigin::Caller,
                        lock: AttributeLock::Locked,
                    },
                    true,
                )),
                &document_body,
            ),
            AssignmentDecision::Reject
        );
        let nested = AssignmentRequest {
            raw: RawAttributeValue::Text("value".into()),
            state: AssignmentState::document(),
            in_header: false,
            force_locked: true,
            source_location: None,
        };
        assert_eq!(
            assignment_decision("ordinary", None, &nested),
            AssignmentDecision::Reject
        );
        assert_eq!(
            assignment_decision(
                "sectnums",
                Some((
                    AssignmentState {
                        origin: AttributeOrigin::Caller,
                        lock: AttributeLock::Locked,
                    },
                    true,
                )),
                &document_body,
            ),
            AssignmentDecision::Apply
        );
        assert_eq!(
            assignment_decision(
                "sectnums",
                Some((
                    AssignmentState {
                        origin: AttributeOrigin::Caller,
                        lock: AttributeLock::Locked,
                    },
                    false,
                )),
                &document_body,
            ),
            AssignmentDecision::Reject
        );
    }

    #[test]
    fn max_depth_validation_keeps_numeric_semantics_separate() -> Result<(), String> {
        let request = AssignmentRequest {
            raw: RawAttributeValue::Text("064".into()),
            state: AssignmentState::PROCESSOR,
            in_header: true,
            force_locked: false,
            source_location: None,
        };
        let value = validate_assignment_value(MAX_INCLUDE_DEPTH_ATTR, request.raw)
            .map_err(|invalid| invalid.value)?;

        assert_eq!(
            value.value().and_then(DocumentAttributeValue::as_integer),
            Some(64)
        );
        assert_eq!(
            value.value().and_then(DocumentAttributeValue::text),
            Some("064")
        );
        Ok(())
    }
}
