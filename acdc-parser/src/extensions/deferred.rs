use serde::{Serialize, Serializer, ser::SerializeMap};

use crate::{BlockMetadata, Location, Title};

use super::{MacroInvocation, TargetId};

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

/// A deferred block that must be resolved before conversion.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ExtensionBlock<'a> {
    pub(crate) metadata: BlockMetadata<'a>,
    pub(crate) title: Title<'a>,
    pub(crate) location: Location,
    pub(crate) processor: usize,
    pub(crate) invocation: MacroInvocation<'a>,
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
        map.serialize_entry("macro", &self.invocation.name)?;
        map.serialize_entry("target", &self.invocation.target)?;
        map.serialize_entry("attributes", &self.invocation.attributes)?;
        map.serialize_entry("metadata", &self.metadata)?;
        map.serialize_entry("title", &self.title)?;
        map.serialize_entry("location", &self.location)?;
        map.end()
    }
}
