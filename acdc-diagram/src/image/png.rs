//! Reading a PNG's intrinsic size.
//!
//! Only the 8-byte signature and the `IHDR` chunk are needed, so this avoids
//! pulling a decoder into the dependency graph.

use crate::error::{Error, Result};

const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Width and height, in pixels, of a PNG image.
///
/// # Errors
///
/// Returns [`Error::Image`] when the signature or the leading `IHDR` chunk is
/// missing or malformed.
pub(super) fn dimensions(data: &[u8]) -> Result<(u32, u32)> {
    let header = data
        .get(..24)
        .ok_or_else(|| Error::Image("PNG image is truncated before its header".to_string()))?;

    if header.get(..8) != Some(&SIGNATURE[..]) {
        return Err(Error::Image("not a PNG image".to_string()));
    }

    let chunk_length = read_u32_be(header, 8)?;
    if chunk_length != 13 {
        return Err(Error::Image(format!(
            "unexpected PNG header chunk length {chunk_length}; expected 13"
        )));
    }

    let chunk_type = header
        .get(12..16)
        .ok_or_else(|| Error::Image("PNG image is truncated before its header".to_string()))?;
    if chunk_type != b"IHDR" {
        return Err(Error::Image(format!(
            "unexpected first PNG chunk `{}`; expected `IHDR`",
            String::from_utf8_lossy(chunk_type)
        )));
    }

    Ok((read_u32_be(header, 16)?, read_u32_be(header, 20)?))
}

fn read_u32_be(data: &[u8], offset: usize) -> Result<u32> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(|| Error::Image("PNG image is truncated".to_string()))?;
    Ok(u32::from_be_bytes(bytes))
}
