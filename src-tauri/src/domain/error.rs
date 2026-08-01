#[derive(Debug, PartialEq, Eq)]
pub enum DomainError {
    InvalidCredentials(String),
    InvalidInput(String),
    AuthenticationFailed,
    CredentialsNotFound,
    NotFound(String),
    Network(String),
    UnexpectedResponse(String),
    Storage(String),
    ExternalApiError(String),
    ParseError(String),
    SessionExpired,
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DomainError::InvalidCredentials(reason) => write!(f, "credenciais inválidas: {reason}"),
            DomainError::InvalidInput(reason) => write!(f, "dados inválidos: {reason}"),
            DomainError::AuthenticationFailed => {
                write!(f, "não foi possível autenticar no Jira com as credenciais informadas")
            }
            DomainError::CredentialsNotFound => {
                write!(f, "nenhuma credencial do Jira foi configurada")
            }
            DomainError::NotFound(reason) => write!(f, "não encontrado: {reason}"),
            DomainError::Network(reason) => {
                write!(f, "falha de rede ao falar com o Jira: {reason}")
            }
            DomainError::UnexpectedResponse(reason) => {
                write!(f, "resposta inesperada do Jira: {reason}")
            }
            DomainError::Storage(reason) => write!(f, "falha ao ler/gravar dados locais: {reason}"),
            DomainError::ExternalApiError(reason) => write!(f, "erro na API externa: {reason}"),
            DomainError::ParseError(reason) => write!(f, "erro ao parsear dados: {reason}"),
            DomainError::SessionExpired => {
                write!(f, "a sessão do PontoMais expirou; conecte novamente em Configurações")
            }
        }
    }
}

impl std::error::Error for DomainError {}
