// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use super::*;
use crate::{QueryFamily, QueryStatus};
use alloc::vec;

const ROBOTO: &[u8] =
    include_bytes!("../../../parley_dev/assets/fonts/roboto_fonts/Roboto-Regular.ttf");

fn fonts(shared: bool) -> Collection {
    Collection::new(CollectionOptions {
        shared,
        system_fonts: false,
    })
}

fn blob() -> Blob<u8> {
    Blob::new(Arc::new(ROBOTO))
}

fn face(weight: f32) -> FontInfoOverride<'static> {
    FontInfoOverride {
        family_name: Some("Collision"),
        weight: Some(FontWeight::new(weight)),
        ..Default::default()
    }
}

#[test]
fn author_family_shadows_all_base_faces_in_either_registration_order() {
    for base_first in [false, true] {
        let mut collection = fonts(false);
        let (base, author) = if base_first {
            let base = collection.register_base_fonts(blob(), Some(face(400.0)))[0].0;
            let author = collection.register_fonts(blob(), Some(face(300.0)))[0].0;
            (base, author)
        } else {
            let author = collection.register_fonts(blob(), Some(face(300.0)))[0].0;
            let base = collection.register_base_fonts(blob(), Some(face(400.0)))[0].0;
            (base, author)
        };
        collection.register_base_fonts(blob(), Some(face(700.0)));
        assert_ne!(base, author);
        assert_eq!(collection.family_id("COLLISION"), Some(author));
        let family = collection.family_by_name("Collision").unwrap();
        assert_eq!(
            family.fonts().len(),
            1,
            "base bold must not leak into author matching"
        );
        assert_eq!(family.fonts()[0].weight(), FontWeight::new(300.0));
        assert_eq!(collection.family(base).unwrap().fonts().len(), 2);
        assert_eq!(collection.family_name(base), Some("Collision"));
        assert_eq!(collection.family_ids().count(), 2);
    }
}

#[test]
fn generic_lookup_keeps_base_id_when_a_named_family_shadows_it() {
    let mut collection = fonts(false);
    let base = collection.register_base_fonts(blob(), Some(face(400.0)))[0].0;
    collection.set_generic_families(GenericFamily::SansSerif, [base].into_iter());
    let author = collection.register_fonts(blob(), Some(face(400.0)))[0].0;
    let mut source_cache = SourceCache::default();
    for (family, expected) in [
        (QueryFamily::Named("Collision"), author),
        (QueryFamily::Generic(GenericFamily::SansSerif), base),
    ] {
        let mut query = collection.query(&mut source_cache);
        query.set_families([family]);
        let mut selected = Vec::new();
        query.matches_with(|font| {
            selected.push(font.family.0);
            QueryStatus::Stop
        });
        assert_eq!(selected, vec![expected]);
    }
}

#[test]
fn script_fallback_retains_base_id_when_its_name_is_shadowed() {
    let mut collection = fonts(false);
    let base = collection.register_base_fonts(blob(), Some(face(400.0)))[0].0;
    let key = FallbackKey::new(Script::from_bytes(*b"Latn"), None);
    assert!(collection.set_fallbacks(key, [base].into_iter()));
    collection.register_fonts(blob(), Some(face(400.0)));
    assert_eq!(
        collection.fallback_families(key).collect::<Vec<_>>(),
        vec![base]
    );
}

#[cfg(feature = "std")]
#[test]
fn shared_clones_observe_base_and_author_registrations_and_clear() {
    let mut collection = fonts(true);
    let mut clone = collection.clone();
    let base = collection.register_base_fonts(blob(), Some(face(400.0)))[0].0;
    assert_eq!(clone.family_id("Collision"), Some(base));
    let author = clone.register_fonts(blob(), Some(face(300.0)))[0].0;
    assert_eq!(collection.family_id("Collision"), Some(author));
    assert!(collection.unregister_font(
        base,
        FontWidth::NORMAL,
        FontStyle::Normal,
        FontWeight::NORMAL
    ));
    assert!(collection.family(base).unwrap().fonts().is_empty());
    collection.clear();
    assert_eq!(clone.family_ids().count(), 0);
    assert_eq!(clone.family_names().count(), 0);
}

#[test]
fn unshared_clones_keep_their_own_author_family() {
    let mut collection = fonts(false);
    let base = collection.register_base_fonts(blob(), Some(face(400.0)))[0].0;
    let mut clone = collection.clone();
    let author = clone.register_fonts(blob(), Some(face(400.0)))[0].0;
    assert_eq!(collection.family_id("Collision"), Some(base));
    assert_eq!(clone.family_id("Collision"), Some(author));
}
