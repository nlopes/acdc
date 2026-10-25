use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

const DEFAULT_MAX_TOKENS: usize = 2000;

#[derive(Debug, Serialize, serde::Deserialize)]
struct Page {
    items: Vec<Value>,
    next_cursor: Option<String>,
    warning_count: usize,
}

fn cursor(digest: &[u8], index: usize, offset: usize, timestamp: u64) -> String {
    let mut bytes = digest.to_vec();
    bytes.extend_from_slice(&(index as u64).to_be_bytes());
    bytes.extend_from_slice(&(offset as u64).to_be_bytes());
    bytes.extend_from_slice(&timestamp.to_be_bytes());
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) fn cursor_timestamp(continuation: Option<&str>) -> Result<Option<u64>> {
    continuation
        .map(|continuation| {
            if continuation.len() != 75 {
                return Err(Error::Input("invalid cursor length".into()));
            }
            let bytes = URL_SAFE_NO_PAD
                .decode(continuation)
                .map_err(|_| Error::Input("invalid cursor".into()))?;
            if bytes.len() != 56 {
                return Err(Error::Input("invalid cursor length".into()));
            }
            Ok(u64::from_be_bytes(
                bytes[48..56].try_into().expect("eight bytes"),
            ))
        })
        .transpose()
}

fn boundary(text: &str, mut offset: usize) -> usize {
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn fragment_body<'a>(item: &'a Value, serialized: &'a str) -> &'a str {
    if item["kind"] == "content" {
        item["text"].as_str().unwrap_or(serialized)
    } else {
        serialized
    }
}

fn fragment(item: &Value, text: &str, index: usize, start: usize, end: usize) -> Value {
    let mut value = json!({
        "kind": "fragment", "record_index": index, "offset": start,
        "total_bytes": text.len(), "text": &text[start..end],
    });
    if item["kind"] == "content" {
        value["kind"] = json!("content");
        value["format"] = item["format"].clone();
    }
    value
}

pub(super) fn paginate(
    items: &[Value],
    identity: &[u8],
    warning_count: usize,
    max_tokens: Option<usize>,
    continuation: Option<&str>,
    timestamp: u64,
) -> Result<String> {
    let tokens = max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    if !(256..=16_000).contains(&tokens) {
        return Err(Error::Input(
            "max_tokens must be between 256 and 16000".into(),
        ));
    }
    let budget = tokens * 4;
    let records = items
        .iter()
        .map(serde_json::to_string)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut hash = Sha256::new();
    hash.update(identity);
    for record in &records {
        hash.update((record.len() as u64).to_be_bytes());
        hash.update(record.as_bytes());
    }
    let digest = hash.finalize();
    let (mut index, mut offset) = if let Some(continuation) = continuation {
        let bytes = URL_SAFE_NO_PAD
            .decode(continuation)
            .map_err(|_| Error::Input("invalid cursor".into()))?;
        if bytes.len() != 56 || bytes[..32] != digest[..] {
            return Err(Error::Input(
                "cursor is stale or belongs to another request; restart without cursor".into(),
            ));
        }
        let index = usize::try_from(u64::from_be_bytes(
            bytes[32..40].try_into().expect("eight bytes"),
        ))
        .map_err(|_| Error::Input("invalid cursor index".into()))?;
        let offset = usize::try_from(u64::from_be_bytes(
            bytes[40..48].try_into().expect("eight bytes"),
        ))
        .map_err(|_| Error::Input("invalid cursor offset".into()))?;
        if index >= records.len() {
            return Err(Error::Input("invalid cursor index".into()));
        }
        let body = fragment_body(&items[index], &records[index]);
        if offset != 0 && (offset >= body.len() || !body.is_char_boundary(offset)) {
            return Err(Error::Input("invalid cursor position".into()));
        }
        (index, offset)
    } else {
        (0, 0)
    };

    let mut page = Page {
        items: Vec::new(),
        next_cursor: None,
        warning_count,
    };
    while index < items.len() {
        let next = (index + 1 < items.len()).then(|| cursor(&digest, index + 1, 0, timestamp));
        if offset == 0 {
            page.items.push(items[index].clone());
            page.next_cursor = next;
            if serde_json::to_vec(&page)?.len() <= budget {
                index += 1;
                continue;
            }
            page.items.pop();
            if !page.items.is_empty() {
                page.next_cursor = Some(cursor(&digest, index, 0, timestamp));
                break;
            }
        }

        // Content continues as readable text; other oversized records preserve their JSON.
        let record = fragment_body(&items[index], &records[index]);
        let mut low = offset;
        let mut high = record.len().min(offset + budget);
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            let end = boundary(record, middle);
            page.items = vec![fragment(&items[index], record, index, offset, end)];
            page.next_cursor = Some(cursor(&digest, index, end, timestamp));
            if serde_json::to_vec(&page)?.len() <= budget {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        let mut end = boundary(record, low);
        if end < record.len()
            && let Some(newline) = record[offset..end].rfind('\n')
            && newline > (end - offset) / 2
        {
            end = offset + newline + 1;
        }
        if end == offset {
            return Err(Error::Input("response budget cannot fit a fragment".into()));
        }
        page.items = vec![fragment(&items[index], record, index, offset, end)];
        offset = end;
        if offset == record.len() {
            index += 1;
            offset = 0;
        }
        page.next_cursor = (index < items.len()).then(|| cursor(&digest, index, offset, timestamp));
        break;
    }
    Ok(serde_json::to_string(&page)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragments_are_bounded_lossless_unicode_and_restart_safe() {
        let original = json!({"kind":"warning", "text":"héllo 🦀\n\"".repeat(1000)});
        let records = vec![original.clone(), json!({"kind":"done"})];
        let mut next = None;
        let mut reconstructed = String::new();
        let mut saw_done = false;
        loop {
            let response =
                paginate(&records, b"request", 0, Some(256), next.as_deref(), 123).unwrap();
            assert!(response.len() <= 1024);
            let page: Page = serde_json::from_str(&response).unwrap();
            for record in page.items {
                if record["kind"] == "fragment" {
                    assert_eq!(record["offset"], reconstructed.len());
                    reconstructed.push_str(record["text"].as_str().unwrap());
                } else {
                    assert_eq!(record["kind"], "done");
                    saw_done = true;
                }
            }
            next = page.next_cursor;
            if next.is_none() {
                break;
            }
        }
        assert_eq!(
            serde_json::from_str::<Value>(&reconstructed).unwrap(),
            original
        );
        assert!(saw_done);
    }

    #[test]
    fn rejects_changed_content_and_invalid_budgets() {
        let records = vec![json!({"text":"a".repeat(2000)})];
        let page: Page =
            serde_json::from_str(&paginate(&records, b"a", 0, Some(256), None, 123).unwrap())
                .unwrap();
        assert!(paginate(&records, b"b", 0, None, page.next_cursor.as_deref(), 123).is_err());
        assert!(
            paginate(
                &[json!({"text":"changed"})],
                b"a",
                0,
                None,
                page.next_cursor.as_deref(),
                123
            )
            .is_err()
        );
        assert!(paginate(&records, b"a", 0, Some(0), None, 123).is_err());
    }

    #[test]
    fn oversized_content_remains_readable_and_reassembles_without_json_decoding() {
        let text = "echo 'héllo 🦀'\n".repeat(300);
        let items = vec![json!({"kind":"content", "format":"asciidoc", "text":text})];
        let mut next = None;
        let mut result = String::new();
        loop {
            let response = paginate(&items, b"read", 0, Some(256), next.as_deref(), 123).unwrap();
            assert!(response.len() <= 1024);
            let page: Page = serde_json::from_str(&response).unwrap();
            for item in page.items {
                assert_eq!(item["kind"], "content");
                assert_eq!(item["offset"], result.len());
                result.push_str(item["text"].as_str().unwrap());
            }
            next = page.next_cursor;
            if next.is_none() {
                break;
            }
        }
        assert_eq!(result, text);
    }
}
