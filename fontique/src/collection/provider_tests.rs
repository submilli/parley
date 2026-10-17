// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
use super::*;
use core::sync::atomic::{AtomicUsize, Ordering};

const ROBOTO: &[u8] =
    include_bytes!("../../../parley_dev/assets/fonts/roboto_fonts/Roboto-Regular.ttf");
struct Provider(AtomicUsize);
impl FontProvider for Provider {
    fn load_font(&self, family: &str) -> Option<ProvidedFamily> {
        self.0.fetch_add(1, Ordering::Relaxed);
        family.eq_ignore_ascii_case("Roboto").then(provided)
    }
    fn load_fallback(&self, _key: FallbackKey) -> Option<ProvidedFamily> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Some(provided())
    }
}
fn collection() -> (Collection, Arc<Provider>) {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    let provider = Arc::new(Provider(AtomicUsize::new(0)));
    collection.set_font_provider(provider.clone());
    (collection, provider)
}
#[test]
fn named_fonts_and_misses_are_lazy_and_cached_case_insensitively() {
    let (mut collection, provider) = collection();
    assert_eq!(provider.0.load(Ordering::Relaxed), 0);
    assert!(collection.family_id("Missing").is_none());
    assert!(collection.family_id("MISSING").is_none());
    assert_eq!(provider.0.load(Ordering::Relaxed), 1);
    let id = collection.family_id("Roboto").unwrap();
    assert_eq!(collection.family_id("ROBOTO"), Some(id));
    assert_eq!(provider.0.load(Ordering::Relaxed), 2);
}
#[test]
fn author_fonts_take_precedence_over_deferred_base_fonts() {
    let (mut collection, _) = collection();
    let base = collection.family_id("Roboto").unwrap();
    let author = collection.register_fonts(Blob::new(Arc::new(ROBOTO)), None)[0].0;
    assert_ne!(base, author);
    assert_eq!(collection.family_id("Roboto"), Some(author));
}
#[test]
fn provider_fallbacks_are_cached_and_do_not_replace_explicit_fallbacks() {
    let (mut collection, provider) = collection();
    let key = FallbackKey::new(Script::from_bytes(*b"Arab"), None);
    let first = collection.fallback_families(key).collect::<Vec<_>>();
    assert_eq!(first.len(), 1);
    assert_eq!(collection.fallback_families(key).collect::<Vec<_>>(), first);
    assert_eq!(provider.0.load(Ordering::Relaxed), 1);
    collection.set_fallbacks(key, core::iter::empty());
    assert_eq!(collection.fallback_families(key).count(), 0);
    assert_eq!(provider.0.load(Ordering::Relaxed), 1);
}
#[test]
fn missing_name_growth_and_provider_calls_are_bounded() {
    let (mut collection, provider) = collection();
    for index in 0..512 {
        assert!(
            collection
                .family_id(&alloc::format!("Missing{index}"))
                .is_none()
        );
    }
    assert_eq!(provider.0.load(Ordering::Relaxed), 128);
}

fn provided() -> ProvidedFamily {
    ProvidedFamily {
        name: "Roboto".into(),
        fonts: alloc::vec![ProvidedFont {
            data: Blob::new(Arc::new(ROBOTO)),
            indices: alloc::vec![0]
        }],
    }
}
#[test]
fn first_shared_lookup_succeeds_and_clear_allows_reloading() {
    let provider = Arc::new(Provider(AtomicUsize::new(0)));
    let mut collection = Collection::new(CollectionOptions {
        shared: true,
        system_fonts: false,
    });
    collection.set_font_provider(provider.clone());
    assert!(collection.family_id("Roboto").is_some());
    collection.clear();
    assert!(collection.family_id("Roboto").is_some());
    assert_eq!(provider.0.load(Ordering::Relaxed), 2);
}
struct Alias;
impl FontProvider for Alias {
    fn load_font(&self, _: &str) -> Option<ProvidedFamily> {
        Some(provided())
    }
}
#[test]
fn catalog_alias_resolves_to_the_selected_family() {
    let (mut collection, _) = collection();
    collection.set_font_provider(Arc::new(Alias));
    let id = collection.family_id("Localized Roboto").unwrap();
    assert_eq!(collection.family_by_name("Roboto").unwrap().id(), id);
    assert_eq!(collection.family_id("LOCALIZED ROBOTO"), Some(id));
}

#[test]
fn shared_clone_reloads_fallback_after_the_other_clone_clears() {
    let provider = Arc::new(Provider(AtomicUsize::new(0)));
    let mut first = Collection::new(CollectionOptions {
        shared: true,
        system_fonts: false,
    });
    first.set_font_provider(provider.clone());
    let key = FallbackKey::new(Script::from_bytes(*b"Arab"), None);
    assert_eq!(first.fallback_families(key).count(), 1);
    let mut second = first.clone();
    first.clear();
    assert_eq!(second.fallback_families(key).count(), 1);
    assert_eq!(provider.0.load(Ordering::Relaxed), 2);
}
#[test]
fn provider_alias_cannot_replace_a_supplied_default_family() {
    let (mut collection, _) = collection();
    let base = collection.register_base_fonts(
        Blob::new(Arc::new(ROBOTO)),
        Some(FontInfoOverride {
            weight: Some(FontWeight::new(100.0)),
            ..Default::default()
        }),
    )[0]
    .0;
    collection.set_font_provider(Arc::new(Alias));
    assert_eq!(collection.family_id("Host Alias"), Some(base));
    assert_eq!(
        collection.family(base).unwrap().fonts()[0].weight(),
        FontWeight::new(100.0)
    );
}

#[test]
fn unrelated_document_languages_use_the_default_script_fallback() {
    let (mut collection, provider) = collection();
    let script = Script::from_bytes(*b"Arab");
    let language = Language::parse("he").unwrap();
    let unrelated = FallbackKey::new(script, Some(&language));
    assert!(!unrelated.is_tracked());
    let fonts = collection.fallback_families(unrelated).collect::<Vec<_>>();
    assert_eq!(fonts.len(), 1);
    assert_eq!(
        collection
            .fallback_families(FallbackKey::new(script, None))
            .collect::<Vec<_>>(),
        fonts
    );
    assert_eq!(provider.0.load(Ordering::Relaxed), 1);
}

struct ArabicProvider;
impl FontProvider for ArabicProvider {
    fn load_font(&self, _: &str) -> Option<ProvidedFamily> {
        None
    }
    fn load_fallback(&self, key: FallbackKey) -> Option<ProvidedFamily> {
        (key.script() == Script::from_bytes(*b"Arab")).then(|| ProvidedFamily {
            name: "Noto Kufi Arabic".into(),
            fonts: alloc::vec![ProvidedFont {
                data: Blob::new(Arc::new(
                    include_bytes!(
                        "../../../parley_dev/assets/fonts/noto_fonts/NotoKufiArabic-Regular.otf"
                    )
                    .as_slice()
                )),
                indices: alloc::vec![0],
            }],
        })
    }
}
#[test]
fn queries_find_arabic_glyphs_under_a_hebrew_document_language() {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    collection.set_font_provider(Arc::new(ArabicProvider));
    let mut sources = SourceCache::new_shared();
    let mut query = collection.query(&mut sources);
    let language = Language::parse("he").unwrap();
    query.set_fallbacks(FallbackKey::new(
        Script::from_bytes(*b"Arab"),
        Some(&language),
    ));
    let mut found = false;
    query.matches_with(|font| {
        found |= font.charmap().is_some_and(|map| map.map('ع').is_some());
        QueryStatus::Continue
    });
    assert!(
        found,
        "mixed-language text must reach the provided script font"
    );
}
