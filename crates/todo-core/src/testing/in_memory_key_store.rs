//! `InMemoryKeyStore`: flüchtige [`KeyStore`]-Implementierung für Tests und die Dev-CLI.

use std::{collections::BTreeMap, sync::Mutex};

use crate::{
    ports::{KeyStore, SecretBytes},
    CoreResult,
};

#[derive(Debug, Default)]
pub struct InMemoryKeyStore {
    entries: Mutex<BTreeMap<String, SecretBytes>>,
}

impl InMemoryKeyStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, SecretBytes>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl KeyStore for InMemoryKeyStore {
    async fn get(&self, name: &str) -> CoreResult<Option<SecretBytes>> {
        Ok(self.entries().get(name).cloned())
    }

    async fn put(&self, name: &str, value: SecretBytes) -> CoreResult<()> {
        self.entries().insert(name.to_owned(), value);
        Ok(())
    }

    async fn delete(&self, name: &str) -> CoreResult<()> {
        self.entries().remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;

    #[test]
    fn put_get_delete() {
        let store = InMemoryKeyStore::new();
        assert_eq!(block_on(store.get("k")).unwrap(), None);
        block_on(store.put("k", SecretBytes::new(vec![1]))).unwrap();
        assert_eq!(
            block_on(store.get("k")).unwrap(),
            Some(SecretBytes::new(vec![1]))
        );
        block_on(store.delete("k")).unwrap();
        block_on(store.delete("k")).unwrap();
        assert_eq!(block_on(store.get("k")).unwrap(), None);
    }
}
