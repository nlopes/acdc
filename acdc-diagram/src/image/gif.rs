//! Reading a GIF's intrinsic size from its logical screen descriptor.

use crate::error::{Error, Result};

/// Width and height, in pixels, of a GIF image.
///
/// # Errors
///
/// Returns [`Error::Image`] when the header is missing or does not start with
/// a `GIF87a`/`GIF89a` signature.
pub(super) fn dimensions(data: &[u8]) -> Result<(u16, u16)> {
    let header = data
        .get(..10)
        .ok_or_else(|| Error::Image("GIF image is truncated before its header".to_string()))?;

    let signature = header
        .get(..6)
        .ok_or_else(|| Error::Image("GIF image is truncated".to_string()))?;
    if signature != b"GIF87a" && signature != b"GIF89a" {
        return Err(Error::Image("not a GIF image".to_string()));
    }

    Ok((read_u16_le(header, 6)?, read_u16_le(header, 8)?))
}

fn read_u16_le(data: &[u8], offset: usize) -> Result<u16> {
    let bytes: [u8; 2] = data
        .get(offset..offset + 2)
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(|| Error::Image("GIF image is truncated".to_string()))?;
    Ok(u16::from_le_bytes(bytes))
}
