use zeroize::Zeroizing;

const KEYRING_SERVICE: &str = "gama-jira-assistant";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretNamespace {
    Jira,
    PontoMais,
    Custom(String),
}

impl SecretNamespace {
    fn as_str(&self) -> &str {
        match self {
            SecretNamespace::Jira => "jira",
            SecretNamespace::PontoMais => "pontomais",
            SecretNamespace::Custom(name) => name.as_str(),
        }
    }
}

impl From<String> for SecretNamespace {
    fn from(value: String) -> Self {
        match value.trim().to_lowercase().as_str() {
            "jira" => SecretNamespace::Jira,
            "pontomais" => SecretNamespace::PontoMais,
            normalized => SecretNamespace::Custom(normalized.to_string()),
        }
    }
}

#[derive(Debug)]
pub enum SecretStoreError {
    SecretNotFound,
    InvalidInput,
    Internal,
}

impl std::fmt::Display for SecretStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            SecretStoreError::SecretNotFound => "secret not found",
            SecretStoreError::InvalidInput => "invalid input",
            SecretStoreError::Internal => "internal vault error",
        };
        write!(f, "{message}")
    }
}

impl std::error::Error for SecretStoreError {}

impl serde::Serialize for SecretStoreError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

fn storage_key(namespace: &SecretNamespace, key: &str) -> String {
    format!("{}/{}", namespace.as_str(), key)
}

fn validate(namespace: &SecretNamespace, key: &str) -> Result<(), SecretStoreError> {
    if namespace.as_str().trim().is_empty() || key.trim().is_empty() {
        return Err(SecretStoreError::InvalidInput);
    }
    Ok(())
}

fn entry(service: &str, namespace: &SecretNamespace, key: &str) -> Result<keyring::Entry, SecretStoreError> {
    keyring::Entry::new(service, &storage_key(namespace, key)).map_err(|_| SecretStoreError::Internal)
}

/// Facade over the OS-native credential store (Windows Credential Manager,
/// macOS Keychain, Linux Secret Service). The rest of the app only knows it
/// can store/get/delete secrets — not that the backing store is a keyring.
#[derive(Clone)]
pub struct SecretStore {
    service: &'static str,
}

impl SecretStore {
    pub fn new() -> Self {
        Self {
            service: KEYRING_SERVICE,
        }
    }

    pub fn store(
        &self,
        namespace: &SecretNamespace,
        key: &str,
        value: Zeroizing<Vec<u8>>,
    ) -> Result<(), SecretStoreError> {
        validate(namespace, key)?;

        entry(self.service, namespace, key)?
            .set_secret(&value)
            .map_err(|_| SecretStoreError::Internal)?;

        log::info!("secret stored (namespace={})", namespace.as_str());
        Ok(())
    }

    pub fn get(&self, namespace: &SecretNamespace, key: &str) -> Result<Zeroizing<Vec<u8>>, SecretStoreError> {
        validate(namespace, key)?;

        match entry(self.service, namespace, key)?.get_secret() {
            Ok(value) => Ok(Zeroizing::new(value)),
            Err(keyring::Error::NoEntry) => Err(SecretStoreError::SecretNotFound),
            Err(_) => Err(SecretStoreError::Internal),
        }
    }

    pub fn delete(&self, namespace: &SecretNamespace, key: &str) -> Result<(), SecretStoreError> {
        validate(namespace, key)?;

        match entry(self.service, namespace, key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {
                log::info!("secret deleted (namespace={})", namespace.as_str());
                Ok(())
            }
            Err(_) => Err(SecretStoreError::Internal),
        }
    }
}

impl Default for SecretStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
pub(crate) fn test_store() -> SecretStore {
    SecretStore {
        service: "gama-jira-assistant-tests",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_namespace(test_name: &str) -> SecretNamespace {
        SecretNamespace::Custom(format!("{test_name}-{}", uuid_like()))
    }

    fn uuid_like() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64
    }

    fn secret(value: &str) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(value.as_bytes().to_vec())
    }

    fn cleanup(service: &SecretStore, namespace: &SecretNamespace, key: &str) {
        let _ = service.delete(namespace, key);
    }

    #[test]
    fn store_secret_creates_a_key() {
        let service = test_store();
        let namespace = unique_namespace("store");

        let result = service.store(&namespace, "access_token", secret("abc123"));

        assert!(result.is_ok());
        cleanup(&service, &namespace, "access_token");
    }

    #[test]
    fn get_secret_returns_the_value() {
        let service = test_store();
        let namespace = unique_namespace("get");
        service.store(&namespace, "access_token", secret("abc123")).unwrap();

        let value = service.get(&namespace, "access_token").unwrap();

        assert_eq!(value.as_slice(), b"abc123");
        cleanup(&service, &namespace, "access_token");
    }

    #[test]
    fn delete_secret_removes_the_key() {
        let service = test_store();
        let namespace = unique_namespace("delete");
        service.store(&namespace, "access_token", secret("abc123")).unwrap();

        service.delete(&namespace, "access_token").unwrap();

        let result = service.get(&namespace, "access_token");
        assert!(matches!(result, Err(SecretStoreError::SecretNotFound)));
    }

    #[test]
    fn delete_secret_is_idempotent() {
        let service = test_store();
        let namespace = unique_namespace("delete-idempotent");

        let first = service.delete(&namespace, "access_token");
        let second = service.delete(&namespace, "access_token");

        assert!(first.is_ok());
        assert!(second.is_ok());
    }

    #[test]
    fn get_secret_returns_not_found_when_missing() {
        let service = test_store();
        let namespace = unique_namespace("missing");

        let result = service.get(&namespace, "missing_key");

        assert!(matches!(result, Err(SecretStoreError::SecretNotFound)));
    }

    #[test]
    fn different_namespaces_do_not_collide() {
        let service = test_store();
        let jira_namespace = unique_namespace("jira");
        let github_namespace = unique_namespace("github");
        service.store(&jira_namespace, "token", secret("jira-value")).unwrap();
        service
            .store(&github_namespace, "token", secret("github-value"))
            .unwrap();

        let jira_value = service.get(&jira_namespace, "token").unwrap();
        let github_value = service.get(&github_namespace, "token").unwrap();

        assert_eq!(jira_value.as_slice(), b"jira-value");
        assert_eq!(github_value.as_slice(), b"github-value");
        cleanup(&service, &jira_namespace, "token");
        cleanup(&service, &github_namespace, "token");
    }

    #[test]
    fn overwriting_a_key_keeps_only_the_last_value() {
        let service = test_store();
        let namespace = unique_namespace("overwrite");
        service.store(&namespace, "token", secret("first")).unwrap();

        service.store(&namespace, "token", secret("second")).unwrap();

        let value = service.get(&namespace, "token").unwrap();
        assert_eq!(value.as_slice(), b"second");
        cleanup(&service, &namespace, "token");
    }

    #[test]
    fn empty_key_returns_invalid_input() {
        let service = test_store();
        let namespace = unique_namespace("empty-key");

        let result = service.store(&namespace, "   ", secret("value"));

        assert!(matches!(result, Err(SecretStoreError::InvalidInput)));
    }

    #[test]
    fn empty_namespace_returns_invalid_input() {
        let service = test_store();

        let result = service.store(&SecretNamespace::Custom("   ".to_string()), "key", secret("value"));

        assert!(matches!(result, Err(SecretStoreError::InvalidInput)));
    }

    #[test]
    fn namespace_from_string_normalizes_case_and_whitespace() {
        let namespace: SecretNamespace = " JIRA ".to_string().into();
        assert_eq!(namespace, SecretNamespace::Jira);

        let custom: SecretNamespace = " GitHub ".to_string().into();
        assert_eq!(custom, SecretNamespace::Custom("github".to_string()));
    }
}
