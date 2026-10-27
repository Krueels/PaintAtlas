//! Source-text translations complement the stable keyed menu translations.
//! English is canonical in code; translate before widget layout, never painted text.
use super::Language;
use eframe::egui;
use std::{borrow::Cow, collections::BTreeMap, sync::OnceLock};

pub fn set_language(ctx: &egui::Context, language: Language) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("paintatlas_language"), language));
}
pub fn language(ctx: &egui::Context) -> Language {
    ctx.data(|data| {
        data.get_temp(egui::Id::new("paintatlas_language"))
            .unwrap_or_default()
    })
}
pub fn ui(ctx: &egui::Context, text: impl AsRef<str>) -> String {
    source(language(ctx), text.as_ref()).into_owned()
}
pub fn widget(ctx: &egui::Context, text: impl Into<egui::WidgetText>) -> egui::WidgetText {
    match text.into() {
        egui::WidgetText::Text(text) => ui(ctx, text).into(),
        styled => styled,
    }
}
struct Pattern {
    english: Vec<String>,
    portuguese: Vec<String>,
}
impl Pattern {
    fn apply(&self, message: &str) -> Option<String> {
        let mut rest = message.strip_prefix(&self.english[0])?;
        let mut values = Vec::new();
        for (i, next) in self.english.iter().enumerate().skip(1) {
            if i == self.english.len() - 1 {
                values.push(rest.strip_suffix(next)?);
            } else {
                if next.is_empty() {
                    return None;
                }
                let end = rest.find(next)?;
                values.push(&rest[..end]);
                rest = &rest[end + next.len()..];
            }
        }
        let mut result = self.portuguese[0].clone();
        for (value, literal) in values.into_iter().zip(&self.portuguese[1..]) {
            result.push_str(value);
            result.push_str(literal);
        }
        Some(result)
    }
}
fn parts(template: &str) -> (Vec<String>, Vec<String>) {
    let mut literals = Vec::new();
    let mut fields = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(end) = rest[start..].find('}') else {
            break;
        };
        let end = start + end;
        literals.push(rest[..start].into());
        fields.push(rest[start..=end].into());
        rest = &rest[end + 1..];
    }
    literals.push(rest.into());
    (literals, fields)
}
struct Catalog {
    entries: BTreeMap<String, String>,
    patterns: Vec<Pattern>,
}
fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let entries: BTreeMap<String, String> =
            serde_json::from_str(include_str!("translations/pt-BR.json"))
                .expect("valid bundled translation catalog");
        let mut patterns = Vec::new();
        for (english, portuguese) in &entries {
            let (english, fields) = parts(english);
            let (portuguese, translated_fields) = parts(portuguese);
            if !fields.is_empty() && fields == translated_fields && !english[0].is_empty() {
                patterns.push(Pattern {
                    english,
                    portuguese,
                });
            }
        }
        patterns
            .sort_by_key(|p| std::cmp::Reverse(p.english.iter().map(String::len).sum::<usize>()));
        Catalog { entries, patterns }
    })
}
pub fn source(language: Language, text: &str) -> Cow<'_, str> {
    if language == Language::English {
        return Cow::Borrowed(text);
    }
    let catalog = catalog();
    if let Some(translated) = catalog.entries.get(text) {
        return Cow::Borrowed(translated);
    }
    // Templates retain the already-formatted arguments, such as paths and sizes.
    for pattern in &catalog.patterns {
        if let Some(translated) = pattern.apply(text) {
            return Cow::Owned(translated);
        }
    }
    Cow::Borrowed(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_preserves_format_fields_and_english_fallback() {
        for (english, portuguese) in &catalog().entries {
            assert_eq!(
                parts(english).1,
                parts(portuguese).1,
                "format mismatch: {english}"
            );
            assert_eq!(source(Language::English, english), english.as_str());
            assert_eq!(
                source(Language::PortugueseBrazil, english),
                portuguese.as_str()
            );
        }
        assert_eq!(
            source(Language::English, "My custom brush"),
            "My custom brush"
        );
        assert_eq!(
            source(Language::PortugueseBrazil, "My custom brush"),
            "My custom brush"
        );
    }
    #[test]
    fn formatted_messages_preserve_user_content_and_contexts_have_independent_languages() {
        let english = egui::Context::default();
        let portuguese = egui::Context::default();
        set_language(&english, Language::English);
        set_language(&portuguese, Language::PortugueseBrazil);
        assert_eq!(ui(&english, "Clear list"), "Clear list");
        assert_eq!(ui(&portuguese, "Clear list"), "Limpar lista");
        assert_eq!(
            ui(&portuguese, "Could not paste: /home/user/Sketch.png"),
            "Erro ao colar: /home/user/Sketch.png"
        );
        assert_eq!(
            ui(&portuguese, "Remove all 12 references from the list?"),
            "Remover todas as 12 referências da lista?"
        );
    }
}
