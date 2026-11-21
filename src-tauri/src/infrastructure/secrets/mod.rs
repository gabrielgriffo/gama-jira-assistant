mod store;

pub use store::{SecretNamespace, SecretStore, SecretStoreError};

#[cfg(test)]
pub(crate) use store::test_store;
