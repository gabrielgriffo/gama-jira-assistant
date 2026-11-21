use crate::domain::error::DomainError;

const MAX_PROJECT_KEY_LEN: usize = 255;
const MAX_ISSUE_NUMBER_LEN: usize = 18;

pub fn validate_issue_id_or_key(value: &str) -> Result<&str, DomainError> {
    if is_issue_id(value) || is_issue_key(value) {
        Ok(value)
    } else {
        Err(DomainError::InvalidInput(format!(
            "chave de issue inválida: {}",
            sanitize_for_message(value)
        )))
    }
}

fn is_issue_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ISSUE_NUMBER_LEN && value.bytes().all(|b| b.is_ascii_digit())
}

fn is_issue_key(value: &str) -> bool {
    let Some((project, number)) = value.rsplit_once('-') else {
        return false;
    };

    let mut project_bytes = project.bytes();
    let starts_with_letter = project_bytes.next().is_some_and(|b| b.is_ascii_alphabetic());

    starts_with_letter
        && project.len() <= MAX_PROJECT_KEY_LEN
        && project.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && is_issue_id(number)
        && !number.starts_with('0')
}

fn sanitize_for_message(value: &str) -> String {
    value.chars().filter(|c| !c.is_control()).take(40).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_keys_and_numeric_ids() {
        for value in ["PROJ-1", "AB_2-123", "X1-99", "10001"] {
            assert!(validate_issue_id_or_key(value).is_ok(), "{value}");
        }
    }

    #[test]
    fn rejects_path_traversal_and_malformed_keys() {
        for value in [
            "",
            "PROJ",
            "PROJ-",
            "PROJ-0",
            "-1",
            "1PROJ-1",
            "PROJ-1/../../myself",
            "PROJ-1?x=1",
            "PROJ-1#frag",
            "PROJ%2F-1",
            "PROJ 1-1",
            "PROJ-1\r\n",
        ] {
            assert!(validate_issue_id_or_key(value).is_err(), "{value:?}");
        }
    }
}
