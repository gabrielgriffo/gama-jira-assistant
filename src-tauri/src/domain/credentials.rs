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

    if !is_allowed_jira_host(&host) {
        return Err(invalid("o endereço deve ser um site Jira Cloud (*.atlassian.net)"));
    }

    Ok(format!("https://{host}"))
}

fn is_allowed_jira_host(host: &str) -> bool {
    ALLOWED_JIRA_HOST_SUFFIXES
        .iter()
        .any(|suffix| host.len() > suffix.len() && host.ends_with(suffix))
}

pub fn validate_jira_page_url(raw: &str) -> Result<Url, DomainError> {
    let invalid = || DomainError::InvalidInput("endereço do Jira inválido".to_string());
    let url = Url::parse(raw.trim()).map_err(|_| invalid())?;
    let safe = url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url
            .host_str()
            .is_some_and(|host| is_allowed_jira_host(&host.to_ascii_lowercase()));
    if safe {
        Ok(url)
    } else {
        Err(invalid())
    }
}

pub fn belongs_to_connection(page: &Url, base_url: &str) -> bool {
    let connected = Url::parse(base_url)
        .ok()
        .and_then(|base| base.host_str().map(str::to_ascii_lowercase));
    connected.is_some() && connected == page.host_str().map(str::to_ascii_lowercase)
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

    #[test]
    fn accepts_jira_cloud_pages() {
        for url in [
            "https://acme.atlassian.net/browse/ABC-1",
            "https://ACME.atlassian.net/browse/ABC-1?focusedWorklogId=10",
            "https://acme.jira.com/browse/ABC-1",
        ] {
            assert!(validate_jira_page_url(url).is_ok(), "{url} deveria ser aceita");
        }
    }

    #[test]
    fn only_pages_of_the_connected_site_belong_to_the_connection() {
        let page = validate_jira_page_url("https://ACME.atlassian.net/browse/ABC-1").unwrap();
        assert!(belongs_to_connection(&page, "https://acme.atlassian.net"));
        assert!(!belongs_to_connection(&page, "https://other.atlassian.net"));
        assert!(!belongs_to_connection(&page, ""));
    }

    #[test]
    fn rejects_pages_outside_jira_cloud() {
        for url in [
            "http://acme.atlassian.net/browse/ABC-1",
            "https://evil.com/acme.atlassian.net/browse/ABC-1",
            "https://acme.atlassian.net.evil.com/browse/ABC-1",
            "https://acme.atlassian.net@evil.com/browse/ABC-1",
            "https://user:pass@acme.atlassian.net/browse/ABC-1",
            "https://acme.atlassian.net:8443/browse/ABC-1",
            "https://atlassian.net/browse/ABC-1",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
            "acme.atlassian.net/browse/ABC-1",
            "",
        ] {
            assert!(validate_jira_page_url(url).is_err(), "{url} deveria ser rejeitada");
        }
    }
}
