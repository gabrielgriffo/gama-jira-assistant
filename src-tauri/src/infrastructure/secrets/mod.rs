mod store;

pub use store::{SecretNamespace, SecretStore};

#[cfg(test)]
pub(crate) use store::test_store;
