// Copyright 2026 the Parley Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lazy embedder fonts without ambient system access.
use super::*;

/// A file with the selected face indices; other families in a collection are
/// deliberately excluded. The bytes may be shared across multiple families.
#[derive(Clone)]
pub struct ProvidedFont {
    pub data: Blob<u8>,
    pub indices: Vec<u32>,
}
#[derive(Clone)]
pub struct ProvidedFamily {
    pub name: String,
    pub fonts: Vec<ProvidedFont>,
}

/// A synchronous resolver supplied by an embedder. A sandbox may suspend its
/// guest stack during host I/O. It must not reenter this collection. None
/// leaves ordinary CSS and registered fallback selection intact.
pub trait FontProvider: Send + Sync {
    fn load_font(&self, family: &str) -> Option<ProvidedFamily>;
    fn load_fallback(&self, _key: FallbackKey) -> Option<ProvidedFamily> {
        None
    }
}

#[derive(Clone)]
pub(super) struct DeferredFonts {
    provider: Arc<dyn FontProvider>,
    names: HashMap<String, Option<FamilyId>>,
    fallbacks: HashMap<FallbackKey, Option<FamilyId>>,
    remaining: usize,
}
impl DeferredFonts {
    pub fn new(provider: Arc<dyn FontProvider>) -> Self {
        Self {
            provider,
            names: HashMap::new(),
            fallbacks: HashMap::new(),
            remaining: 64 << 20,
        }
    }
    pub fn reset(&mut self) {
        self.names.clear();
        self.fallbacks.clear();
        self.remaining = 64 << 20;
    }
    fn accept(&mut self, family: Option<ProvidedFamily>) -> Option<ProvidedFamily> {
        let family = family?;
        let bytes = family
            .fonts
            .iter()
            .try_fold(0usize, |sum, font| sum.checked_add(font.data.len()))?;
        let faces = family
            .fonts
            .iter()
            .map(|font| font.indices.len())
            .sum::<usize>();
        if family.name.is_empty()
            || family.name.len() > 256
            || family.fonts.len() > 16
            || faces > 64
            || bytes > self.remaining
        {
            return None;
        }
        self.remaining -= bytes;
        Some(family)
    }
}
impl Inner {
    pub(super) fn load_provider_family(&mut self, name: &str) -> Option<FamilyId> {
        let provider = self.provider.as_mut()?;
        if name.len() > 256 {
            return None;
        }
        let key = name.to_lowercase();
        if let Some(cached) = provider.names.get(&key) {
            match cached {
                None => return None,
                Some(id) if self.data.families.contains_key(id) => return Some(*id),
                _ => provider.reset(), // A shared clone cleared the registered fonts.
            }
        }
        if provider.names.len() + provider.fallbacks.len() >= 128 {
            return None;
        }
        provider.names.insert(key.clone(), None);
        let family = provider.provider.load_font(name);
        let family = provider.accept(family)?;
        let id = self.register_provider_family(family)?;
        self.provider.as_mut()?.names.insert(key, Some(id));
        Some(id)
    }
    pub(super) fn load_provider_fallback(&mut self, key: FallbackKey) {
        self.sync_shared();
        if self.data.fallbacks.get(key).is_some() {
            return;
        }
        let Some(provider) = self.provider.as_mut() else {
            return;
        };
        if let Some(cached) = provider.fallbacks.get(&key) {
            match cached {
                Some(id) if !self.data.families.contains_key(id) => provider.reset(),
                _ => return,
            }
        }
        if provider.names.len() + provider.fallbacks.len() >= 128 {
            return;
        }
        provider.fallbacks.insert(key, None);
        let family = provider.provider.load_fallback(key);
        let Some(family) = provider.accept(family) else {
            return;
        };
        if let Some(id) = self.register_provider_family(family) {
            self.set_fallbacks(key, core::iter::once(id));
            if let Some(provider) = &mut self.provider {
                provider.fallbacks.insert(key, Some(id));
            }
        }
    }
    fn register_provider_family(&mut self, family: ProvidedFamily) -> Option<FamilyId> {
        self.sync_shared();
        if let Some(name) = self.data.base_family_names.get(&family.name) {
            return Some(name.id());
        }
        let mut fonts = Vec::new();
        for font in family.fonts {
            let source = SourceInfo::new(SourceId::new(), SourceKind::Memory(font.data));
            for index in font.indices {
                if let Some(font) = FontInfo::from_source(source.clone(), index) {
                    fonts.push(font);
                }
            }
        }
        if fonts.is_empty() {
            return None;
        }
        #[cfg(feature = "std")]
        if let Some(shared) = &self.shared {
            let id = shared
                .data
                .lock()
                .unwrap()
                .register_provider_family(&family.name, fonts);
            shared.bump_version();
            self.sync_shared();
            return Some(id);
        }
        Some(self.data.register_provider_family(&family.name, fonts))
    }
}
impl CommonData {
    fn register_provider_family(&mut self, name: &str, fonts: Vec<FontInfo>) -> FamilyId {
        if let Some(existing) = self.base_family_names.get(name) {
            return existing.id();
        }
        let name = self.base_family_names.get_or_insert(name);
        let id = name.id();
        self.families.insert(id, Some(FamilyInfo::new(name, fonts)));
        id
    }
}
