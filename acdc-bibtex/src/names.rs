//! Reading and arranging the names credited for a work.
//!
//! BibTeX writes a name list as `First Last and Other Name`, with each name
//! either `Given Family` or `Family, Given`. Which way round a style prints
//! them, and how it joins the list, is the most visible difference between
//! citation styles, so both live here and the styles choose.

/// One credited person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Name {
    /// Given names, as written — often already initials in a `.bib` file.
    pub(crate) given: String,
    /// The family name a citation is sorted and abbreviated by.
    pub(crate) family: String,
}

impl Name {
    /// `Given Family`, the order a name is spoken in.
    pub(crate) fn given_first(&self) -> String {
        if self.given.is_empty() {
            self.family.clone()
        } else {
            format!("{} {}", self.given, self.family)
        }
    }

    /// `Family, Given`, the order a bibliography is sorted by.
    pub(crate) fn family_first(&self) -> String {
        if self.given.is_empty() {
            self.family.clone()
        } else {
            format!("{}, {}", self.family, self.given)
        }
    }

    /// `Y. Guo`: the initials before the family name, as IEEE sets them.
    pub(crate) fn initials_first(&self) -> String {
        let initials = self.initials();
        if initials.is_empty() {
            self.family.clone()
        } else {
            format!("{initials} {}", self.family)
        }
    }

    /// `Guo, Y.`: the family name first, the given names as initials.
    pub(crate) fn family_then_initials(&self) -> String {
        let initials = self.initials();
        if initials.is_empty() {
            self.family.clone()
        } else {
            format!("{}, {initials}", self.family)
        }
    }

    /// The given names reduced to initials.
    ///
    /// A name a `.bib` file already wrote as an initial keeps its form, and a
    /// hyphenated given name keeps its hyphen: `Jean-Luc` becomes `J.-L.`
    fn initials(&self) -> String {
        self.given
            .split_whitespace()
            .map(|word| {
                word.split('-')
                    .filter_map(|part| part.chars().next())
                    .map(|first| format!("{first}."))
                    .collect::<Vec<_>>()
                    .join("-")
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Split a BibTeX name field into its names.
///
/// The separator is the word `and`, which has to be matched as a word so that
/// a family name containing it survives.
pub(crate) fn parse_list(field: &str) -> Vec<Name> {
    split_on_and(field)
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .map(|part| parse_one(&part))
        .collect()
}

/// One name, in either of the orders BibTeX allows.
fn parse_one(text: &str) -> Name {
    let text = text.trim();
    if let Some((family, given)) = text.rsplit_once(',') {
        return Name {
            given: given.trim().to_string(),
            family: family.trim().to_string(),
        };
    }
    // `Given Family`: everything up to the last word is the given name. A
    // lower-case particle belongs with the family name — `Ludwig van
    // Beethoven` is filed under `van Beethoven`.
    let words: Vec<&str> = text.split_whitespace().collect();
    let particle = words
        .iter()
        .position(|word| word.chars().next().is_some_and(char::is_lowercase));
    let split = particle.unwrap_or_else(|| words.len().saturating_sub(1));
    let (given, family) = words.split_at(split);
    Name {
        given: given.join(" "),
        family: family.join(" "),
    }
}

/// Split on the word `and`, ignoring one inside braces.
fn split_on_and(field: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0_usize;
    for word in field.split(' ') {
        depth += word.matches('{').count();
        depth = depth.saturating_sub(word.matches('}').count());
        if word == "and" && depth == 0 {
            parts.push(std::mem::take(&mut current));
            continue;
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    parts.push(current);
    parts
}

/// Join a list the way `A, B, and C` reads, with `last` before the final name.
///
/// `serial` decides whether the separator before the last name keeps its
/// comma, which is where the styles disagree for two names.
pub(crate) fn join(parts: &[String], last: &str, serial: bool) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] if !serial => format!("{first} {last} {second}"),
        [first, second] => format!("{first}, {last} {second}"),
        _ => {
            let Some((final_name, rest)) = parts.split_last() else {
                return String::new();
            };
            format!("{}, {last} {final_name}", rest.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(field: &str) -> Vec<(String, String)> {
        parse_list(field)
            .into_iter()
            .map(|name| (name.given, name.family))
            .collect()
    }

    #[test]
    fn reads_given_family_order() {
        assert_eq!(names("D. Smith"), [("D.".into(), "Smith".into())]);
        assert_eq!(
            names("J. R. Anderson"),
            [("J. R.".into(), "Anderson".into())]
        );
    }

    #[test]
    fn reads_family_first_order() {
        assert_eq!(names("Smith, David"), [("David".into(), "Smith".into())]);
    }

    #[test]
    fn splits_a_list_on_the_word_and() {
        assert_eq!(
            names("K. Mane and D. Smith"),
            [("K.".into(), "Mane".into()), ("D.".into(), "Smith".into())]
        );
    }

    #[test]
    fn keeps_a_lower_case_particle_with_the_family_name() {
        assert_eq!(
            names("Ludwig van Beethoven"),
            [("Ludwig".into(), "van Beethoven".into())]
        );
    }

    #[test]
    fn handles_a_single_word_name() {
        assert_eq!(names("Aristotle"), [(String::new(), "Aristotle".into())]);
    }

    #[test]
    fn abbreviates_given_names_to_initials() {
        let full = parse_one("Yinghua Guo");
        assert_eq!(full.initials_first(), "Y. Guo");
        assert_eq!(full.family_then_initials(), "Guo, Y.");
        // Names already written as initials are left as they are.
        let short = parse_one("J. R. Anderson");
        assert_eq!(short.initials_first(), "J. R. Anderson");
        assert_eq!(short.family_then_initials(), "Anderson, J. R.");
        assert_eq!(
            parse_one("Jean-Luc Picard").initials_first(),
            "J.-L. Picard"
        );
        assert_eq!(parse_one("Aristotle").initials_first(), "Aristotle");
    }

    #[test]
    fn joins_lists_the_way_each_style_wants() {
        let one = vec!["A".to_string()];
        let two = vec!["A".to_string(), "B".to_string()];
        let three = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        assert_eq!(join(&one, "and", true), "A");
        assert_eq!(join(&two, "and", false), "A and B");
        assert_eq!(join(&two, "and", true), "A, and B");
        assert_eq!(join(&three, "and", true), "A, B, and C");
        assert_eq!(join(&three, "&", true), "A, B, & C");
    }
}
