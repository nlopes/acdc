use super::ContentAttribute;

/// The author or source of quoted content.
pub type Attribution<'a> = ContentAttribute<'a>;

/// The title of the work cited by quoted content.
pub type CiteTitle<'a> = ContentAttribute<'a>;
