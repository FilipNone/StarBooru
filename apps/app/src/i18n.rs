//! UI text. Every string the app shows lives in a Fluent catalogue under
//! `locales/` and is looked up with [`t!`]. Adding a language means adding
//! a `.ftl` file and a [`Lang`] variant; the tests check that every
//! catalogue has every message with the same variables.

use std::cell::RefCell;
use std::collections::HashMap;

use dioxus::core::Runtime;
use dioxus::prelude::*;
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use serde::{Deserialize, Serialize};
use unic_langid::LanguageIdentifier;

use crate::store::PREFS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "it")]
    Italian,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::English, Lang::Italian];

    /// BCP 47 code, also used for the `lang` attribute.
    pub fn code(self) -> &'static str {
        match self {
            Lang::English => "en",
            Lang::Italian => "it",
        }
    }

    /// The language's name in itself, so people can find their own.
    pub fn name(self) -> &'static str {
        match self {
            Lang::English => "English",
            Lang::Italian => "Italiano",
        }
    }

    fn source(self) -> &'static str {
        match self {
            Lang::English => include_str!("../locales/en.ftl"),
            Lang::Italian => include_str!("../locales/it.ftl"),
        }
    }

    /// The best match for a locale like `it-IT` or `en_GB.UTF-8`.
    pub fn from_locale(locale: &str) -> Option<Lang> {
        let primary = locale
            .split(['-', '_', '.', '@'])
            .next()?
            .to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == primary)
    }
}

/// The language in use: the one picked in Settings, or the system's.
pub static LANG: GlobalSignal<Lang> = Signal::global(|| resolve(PREFS.peek().language));

pub fn resolve(picked: Option<Lang>) -> Lang {
    picked
        .or_else(|| sys_locale::get_locale().and_then(|l| Lang::from_locale(&l)))
        .unwrap_or_default()
}

type Bundle = FluentBundle<FluentResource>;

thread_local! {
    static BUNDLES: RefCell<HashMap<Lang, Bundle>> = RefCell::new(HashMap::new());
}

fn bundle(lang: Lang) -> Bundle {
    let id: LanguageIdentifier = lang.code().parse().unwrap_or_default();
    let mut bundle = FluentBundle::new(vec![id]);
    // Plain text: no bidi isolation marks around variables.
    bundle.set_use_isolating(false);
    // A parse error drops only the broken entries; the tests keep the
    // catalogues clean.
    let resource = FluentResource::try_new(lang.source().to_owned()).unwrap_or_else(|(r, _)| r);
    // Duplicate ids keep the first entry; the tests catch them too.
    let _ = bundle.add_resource(resource);
    bundle
}

fn format(lang: Lang, id: &str, args: Option<&FluentArgs>) -> Option<String> {
    BUNDLES.with_borrow_mut(|bundles| {
        let bundle = bundles.entry(lang).or_insert_with(|| bundle(lang));
        let pattern = bundle.get_message(id)?.value()?;
        let mut errors = Vec::new();
        Some(
            bundle
                .format_pattern(pattern, args, &mut errors)
                .into_owned(),
        )
    })
}

/// The text for message `id` in the current language, falling back to
/// English and then to the id itself. Inside the UI this subscribes the
/// component to language changes; elsewhere (tests) it uses English.
pub fn tr(id: &str, args: Option<&FluentArgs>) -> String {
    let lang = if Runtime::try_current().is_some() {
        *LANG.read()
    } else {
        Lang::English
    };
    format(lang, id, args)
        .or_else(|| format(Lang::English, id, args))
        .unwrap_or_else(|| id.to_owned())
}

/// `t!(<id>)` or `t!(<id>, count = n, name = s)`: the localised text.
macro_rules! t {
    ($id:literal) => {
        $crate::i18n::tr($id, None)
    };
    ($id:literal, $($key:ident = $value:expr),+ $(,)?) => {{
        let mut args = fluent_bundle::FluentArgs::new();
        $( args.set(stringify!($key), $value); )+
        $crate::i18n::tr($id, Some(&args))
    }};
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use fluent_syntax::ast;

    use super::*;

    /// Message id → the variables it uses.
    fn catalogue(lang: Lang) -> BTreeMap<String, BTreeSet<String>> {
        let resource = fluent_syntax::parser::parse(lang.source())
            .unwrap_or_else(|(_, errors)| panic!("{}: {errors:?}", lang.code()));
        let mut out = BTreeMap::new();
        for entry in resource.body {
            if let ast::Entry::Message(m) = entry {
                let mut vars = BTreeSet::new();
                if let Some(p) = &m.value {
                    variables(p, &mut vars);
                }
                let id = m.id.name.to_owned();
                assert!(
                    out.insert(id.clone(), vars).is_none(),
                    "{}: duplicate {id}",
                    lang.code()
                );
            }
        }
        out
    }

    fn variables(pattern: &ast::Pattern<&str>, out: &mut BTreeSet<String>) {
        for element in &pattern.elements {
            if let ast::PatternElement::Placeable { expression } = element {
                expression_vars(expression, out);
            }
        }
    }

    fn expression_vars(expression: &ast::Expression<&str>, out: &mut BTreeSet<String>) {
        match expression {
            ast::Expression::Select { selector, variants } => {
                inline_vars(selector, out);
                for v in variants {
                    variables(&v.value, out);
                }
            }
            ast::Expression::Inline(inline) => inline_vars(inline, out),
        }
    }

    fn inline_vars(inline: &ast::InlineExpression<&str>, out: &mut BTreeSet<String>) {
        match inline {
            ast::InlineExpression::VariableReference { id } => {
                out.insert(id.name.to_owned());
            }
            ast::InlineExpression::Placeable { expression } => expression_vars(expression, out),
            _ => {}
        }
    }

    /// Every message id passed to `t!` in the app's source.
    fn used_ids() -> BTreeSet<String> {
        fn walk(dir: &Path, out: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    // Split so this line does not match itself.
                    for (i, _) in text.match_indices(concat!("t", "!(")) {
                        // Skip `format!(` and friends.
                        let before = text[..i].chars().next_back().unwrap_or(' ');
                        if before.is_alphanumeric() || before == '_' {
                            continue;
                        }
                        // rustfmt may move the id to the next line.
                        let Some(rest) = text[i + 3..].trim_start().strip_prefix('"') else {
                            continue;
                        };
                        if let Some(end) = rest.find('"') {
                            out.insert(rest[..end].to_owned());
                        }
                    }
                }
            }
        }
        let mut out = BTreeSet::new();
        walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
        out
    }

    #[test]
    fn catalogues_match() {
        let english = catalogue(Lang::English);
        for lang in Lang::ALL {
            let other = catalogue(lang);
            let missing: Vec<_> = english.keys().filter(|k| !other.contains_key(*k)).collect();
            let extra: Vec<_> = other.keys().filter(|k| !english.contains_key(*k)).collect();
            assert!(
                missing.is_empty() && extra.is_empty(),
                "{}: missing {missing:?}, extra {extra:?}",
                lang.code()
            );
            for (id, vars) in &english {
                assert_eq!(
                    Some(vars),
                    other.get(id),
                    "{}: variables of {id}",
                    lang.code()
                );
            }
        }
        let used = used_ids();
        let unknown: Vec<_> = used
            .iter()
            .filter(|id| !english.contains_key(*id))
            .collect();
        assert!(unknown.is_empty(), "used but not in en.ftl: {unknown:?}");
        let unused: Vec<_> = english.keys().filter(|id| !used.contains(*id)).collect();
        assert!(unused.is_empty(), "in en.ftl but never used: {unused:?}");
    }

    #[test]
    fn formats_with_plurals_and_fallbacks() {
        let mut args = FluentArgs::new();
        args.set("count", 1);
        assert_eq!(
            format(Lang::English, "posts-count", Some(&args)).as_deref(),
            Some("1 post")
        );
        assert_eq!(
            format(Lang::Italian, "posts-count", Some(&args)).as_deref(),
            Some("1 post")
        );
        args.set("count", 3);
        assert_eq!(
            format(Lang::English, "posts-count", Some(&args)).as_deref(),
            Some("3 posts")
        );
        assert_eq!(
            format(Lang::Italian, "upload-start", Some(&args)).as_deref(),
            Some("Carica 3 file")
        );
        args.set("count", 0);
        assert_eq!(
            format(Lang::English, "upload-start", Some(&args)).as_deref(),
            Some("Upload")
        );
        assert_eq!(
            format(Lang::Italian, "bulk-selected", Some(&args)).as_deref(),
            Some("0 selezionati")
        );
        assert_eq!(tr("no-such-message", None), "no-such-message");
        assert_eq!(t!("posts-count", count = 2), "2 posts");
    }

    #[test]
    fn matches_locales() {
        assert_eq!(Lang::from_locale("it-IT"), Some(Lang::Italian));
        assert_eq!(Lang::from_locale("en_GB.UTF-8"), Some(Lang::English));
        assert_eq!(Lang::from_locale("IT"), Some(Lang::Italian));
        assert_eq!(Lang::from_locale("de-DE"), None);
        assert_eq!(Lang::from_locale(""), None);
        assert_eq!(resolve(Some(Lang::Italian)), Lang::Italian);
    }
}
