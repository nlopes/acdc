use serde::{Serialize, Serializer, ser::SerializeMap};

use crate::{BlockMetadata, DocumentAttributes, Location, SourceLocation, Title};

use super::{MacroInvocation, RawBlockContext, TargetId};

/// An opaque identity valid only in the document session that allocated it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlaceholderId(pub(crate) usize);

/// A request retained by its document session for deferred construction.
#[derive(Debug)]
#[non_exhaustive]
pub struct DeferredBlock {
    pub placeholder: PlaceholderId,
    pub invocation: MacroInvocation<'static>,
    pub(super) target: TargetId,
}

impl DeferredBlock {
    /// The handle used to query this request's placeholder block.
    #[must_use]
    pub const fn target(&self) -> &TargetId {
        &self.target
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Invocation<'a> {
    Macro(MacroInvocation<'a>),
    Style {
        context: RawBlockContext,
        parse_children: bool,
        body: &'a str,
        body_location: SourceLocation,
        location: SourceLocation,
    },
}

impl<'a> Invocation<'a> {
    pub(crate) fn location(&self) -> &SourceLocation {
        match self {
            Self::Macro(call) => &call.location,
            Self::Style { location, .. } => location,
        }
    }

    pub(crate) fn document_attributes<'b>(
        &'b self,
        current: &'b DocumentAttributes<'a>,
    ) -> &'b DocumentAttributes<'a> {
        match self {
            Self::Macro(call) => &call.document_attributes,
            Self::Style { .. } => current,
        }
    }
}

/// A deferred block that must be resolved before conversion.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ExtensionBlock<'a> {
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) title: Title<'a>,
    pub(crate) location: Location,
    pub(crate) processor: usize,
    pub(crate) invocation: Invocation<'a>,
    pub(crate) placeholder: Option<PlaceholderId>,
}

impl ExtensionBlock<'_> {
    #[must_use]
    pub const fn placeholder(&self) -> Option<PlaceholderId> {
        self.placeholder
    }
}

impl Serialize for ExtensionBlock<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("name", "extension")?;
        map.serialize_entry("type", "block")?;
        let Invocation::Macro(call) = &self.invocation else {
            return Err(serde::ser::Error::custom("unprocessed raw block style"));
        };
        map.serialize_entry("macro", &call.name)?;
        map.serialize_entry("target", &call.target)?;
        map.serialize_entry("attributes", &call.attributes)?;
        map.serialize_entry("metadata", &self.metadata)?;
        map.serialize_entry("title", &self.title)?;
        map.serialize_entry("location", &self.location)?;
        map.end()
    }
}
