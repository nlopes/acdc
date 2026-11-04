use std::{collections::BTreeMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use syn::{Fields, Item};

use super::schema::Result;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct Shape {
    pub(super) kind: String,
    pub(super) fields: Vec<String>,
    pub(super) variants: BTreeMap<String, Vec<String>>,
}

pub(super) type Shapes = BTreeMap<String, Shape>;

fn fields(fields: &Fields) -> Vec<String> {
    fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            field
                .ident
                .as_ref()
                .map_or_else(|| index.to_string(), ToString::to_string)
        })
        .collect()
}

fn visit(root: &Path, path: &Path, shapes: &mut Shapes) -> Result<()> {
    let mut files = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    files.sort_by_key(std::fs::DirEntry::file_name);
    for entry in files {
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            visit(root, &path, shapes)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let source = fs::read_to_string(&path)?;
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            for item in syn::parse_file(&source)?.items {
                let (name, shape) = match item {
                    Item::Struct(item) => (
                        item.ident,
                        Shape {
                            kind: "struct".into(),
                            fields: fields(&item.fields),
                            variants: BTreeMap::new(),
                        },
                    ),
                    Item::Enum(item) => (
                        item.ident,
                        Shape {
                            kind: "enum".into(),
                            fields: Vec::new(),
                            variants: item
                                .variants
                                .iter()
                                .map(|variant| (variant.ident.to_string(), fields(&variant.fields)))
                                .collect(),
                        },
                    ),
                    Item::Type(item) => (
                        item.ident,
                        Shape {
                            kind: "alias".into(),
                            fields: Vec::new(),
                            variants: BTreeMap::new(),
                        },
                    ),
                    Item::Const(_)
                    | Item::ExternCrate(_)
                    | Item::Fn(_)
                    | Item::ForeignMod(_)
                    | Item::Impl(_)
                    | Item::Macro(_)
                    | Item::Mod(_)
                    | Item::Static(_)
                    | Item::Trait(_)
                    | Item::TraitAlias(_)
                    | Item::Use(_)
                    | Item::Verbatim(_) => continue,
                    Item::Union(_) => return Err("model unions require inventory support".into()),
                    _ => return Err("unknown Rust model item requires inventory support".into()),
                };
                if shapes
                    .insert(format!("{relative}::{name}"), shape)
                    .is_some()
                {
                    return Err(format!("duplicate model declaration: {relative}::{name}").into());
                }
            }
        }
    }
    Ok(())
}

pub(super) fn collect() -> Result<Shapes> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut shapes = BTreeMap::new();
    visit(root, &root.join("src/model"), &mut shapes)?;
    Ok(shapes)
}
