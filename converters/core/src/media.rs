//! Media target resolution for URI-producing converters.

use relative_path::RelativePath;

use crate::TraversalContext;

fn uri_prefix_end(target: &str) -> Option<usize> {
    let scheme_end = target.find(':')?;
    let scheme = target.get(..scheme_end)?;
    let has_valid_scheme = !scheme.is_empty()
        && scheme
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'));
    if !has_valid_scheme {
        return None;
    }

    let path_start = scheme_end + 1;
    let slash_count = target
        .get(path_start..)?
        .bytes()
        .take(2)
        .take_while(|byte| *byte == b'/')
        .count();
    Some(path_start + slash_count)
}

fn target_parts(target: &str) -> (&str, &str, &str) {
    let (prefix, target) = uri_prefix_end(target).map_or(("", target), |end| target.split_at(end));
    if target == "." {
        return (prefix, "./", "");
    }
    if let Some(target) = target.strip_prefix("./") {
        return (prefix, "./", target);
    }

    let root_len = target.bytes().take_while(|byte| *byte == b'/').count();
    let (root, target) = target.split_at(root_len);
    (prefix, root, target)
}

fn normalize_target(target: &str) -> String {
    let (prefix, root, target) = target_parts(target);
    let target = RelativePath::new(target).normalize();
    format!("{prefix}{root}{target}")
}

fn join_target(start: &str, target: &str) -> String {
    let (prefix, root, start) = target_parts(start);
    let target = RelativePath::new(start).join_normalized(RelativePath::new(target));
    format!("{prefix}{root}{target}")
}

fn encode_spaces(target: &str) -> String {
    target.replace(' ', "%20")
}

/// Resolves a media target for use as a URI in converter output.
///
/// Relative targets use `imagesdir` when it is set. Absolute paths and targets
/// with a URI scheme ignore `imagesdir`. Local paths use forward slashes and
/// are normalized, while literal spaces are encoded as `%20`.
#[must_use]
pub fn resolve_target(target: &str, attributes: &TraversalContext<'_>) -> String {
    if uri_prefix_end(target).is_some() {
        return encode_spaces(target);
    }

    let target = target.replace('\\', "/");
    let target = if target.starts_with('/') {
        normalize_target(&target)
    } else if let Some(images_dir) = attributes
        .get("imagesdir")
        .and_then(|value| value.text())
        .map(acdc_parser::strip_quotes)
        .filter(|images_dir| !images_dir.is_empty())
    {
        let images_dir = images_dir.replace('\\', "/");
        join_target(&images_dir, &target)
    } else {
        normalize_target(&target)
    };
    encode_spaces(&target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(
        target: &str,
        images_dir: Option<&'static str>,
    ) -> Result<String, acdc_parser::Error> {
        let mut attributes = std::collections::HashMap::<
            std::borrow::Cow<'_, str>,
            acdc_parser::AttributeValue<'_>,
        >::new();
        if let Some(images_dir) = images_dir {
            attributes.insert("imagesdir".into(), images_dir.into());
        }
        Ok(resolve_target(
            target,
            &TraversalContext::new(
                &acdc_parser::Options::builder()
                    .with_defaults(attributes)
                    .build()?
                    .into_document_attributes(),
            ),
        ))
    }

    #[test]
    fn resolves_portable_media_targets() -> Result<(), Box<dyn std::error::Error>> {
        for (input, images_dir, expected) in [
            ("./clips/../demo file.mp4", None, "./demo%20file.mp4"),
            (
                "./clips/../demo file.mp4",
                Some("media/library/"),
                "media/library/demo%20file.mp4",
            ),
            ("./demo.mp4", Some("."), "./demo.mp4"),
            ("demo.mp4", Some("./assets"), "./assets/demo.mp4"),
            (
                "../clips/./demo.mp4",
                Some("../assets/media"),
                "../assets/clips/demo.mp4",
            ),
            ("clips/../demo.mp4", Some("/assets"), "/assets/demo.mp4"),
            ("/media/../demo.mp4", Some("assets"), "/demo.mp4"),
            (
                "already%20encoded.png",
                Some("media library"),
                "media%20library/already%20encoded.png",
            ),
            (
                "clips/../demo.mp4",
                Some("https://cdn.example/media"),
                "https://cdn.example/media/demo.mp4",
            ),
            (
                r"clips\..\demo.mp4",
                Some(r"media\library"),
                "media/library/demo.mp4",
            ),
            (
                "https://media.example/demo folder/../clip.mp4",
                Some("assets"),
                "https://media.example/demo%20folder/../clip.mp4",
            ),
        ] {
            assert_eq!(target(input, images_dir)?, expected);
        }
        Ok(())
    }
}
