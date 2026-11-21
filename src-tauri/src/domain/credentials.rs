use url::Url;
use zeroize::Zeroizing;

use crate::domain::error::DomainError;

const ALLOWED_JIRA_HOST_SUFFIXES: [&str; 2] = [".atlassian.net", ".jira.com"];

/// Everything needed to make an authenticated call to a Jira Cloud site.
/// Constructed once (login) or read back from storage (CredentialStore) —
/// never assembled piecemeal by infrastructure code.
#[derive(Clone)]
pub struct JiraCredentials {
    pub base_url: String,
    pub email: String,
    pub api_token: Zeroizing<String>,
}

pub fn normalize_jira_base_url(raw: &str) -> Result<String, DomainError> {
    let invalid = |reason: &str| DomainError::InvalidCredentials(reason.to_string());

    let url = Url::parse(raw.trim()).map_err(|_| invalid("URL do Jira inválida"))?;

    if url.scheme() != "https" {
        return Err(invalid("URL do Jira deve usar https://"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("URL do Jira não pode conter usuário ou senha"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid("URL do Jira não pode conter query string ou fragmento"));
    }
    if !matches!(url.path(), "" | "/") {
        return Err(invalid(
            "informe apenas o endereço do site, por exemplo https://empresa.atlassian.net",
        ));
    }
    if url.port().is_some() {
        return Err(invalid("URL do Jira não pode conter porta"));
    }

    let host = url
        .host_str()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| invalid("URL do Jira sem host"))?;

    let allowed = ALLOWED_JIRA_HOST_SUFFIXES
        .iter()
        .any(|suffix| host.len() > suffix.len() && host.ends_with(suffix));
    if !allowed {
        return Err(invalid("o endereço deve ser um site Jira Cloud (*.atlassian.net)"));
    }

    Ok(format!("https://{host}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_normalizes_jira_cloud_urls() {
        assert_eq!(
            normalize_jira_base_url(" https://Acme.atlassian.net/ ").unwrap(),
            "https://acme.atlassian.net"
        );
        assert_eq!(
            normalize_jira_base_url("https://acme.jira.com").unwrap(),
            "https://acme.jira.com"
        );
    }

    #[test]
    fn rejects_unsafe_or_foreign_urls() {
        for url in [
            "http://acme.atlassian.net",
            "https://acme.atlassian.net@evil.com",
            "https://user:pass@acme.atlassian.net",
            "https://evil.com",
            "https://atlassian.net",
            "https://acme.atlassian.net.evil.com",
            "https://acme.atlassian.net/rest",
            "https://acme.atlassian.net?x=1",
            "https://acme.atlassian.net#x",
            "https://acme.atlassian.net:8443",
            "ftp://acme.atlassian.net",
            "acme.atlassian.net",
        ] {
            assert!(normalize_jira_base_url(url).is_err(), "{url} deveria ser rejeitada");
        }
    }
}
