use crate::AttributeValue;

#[derive(Debug)]
pub(crate) struct AttributeEntry<'a> {
    pub(crate) set: bool,
    pub(crate) key: &'a str,
    pub(crate) value: AttributeValue<'a>,
}
