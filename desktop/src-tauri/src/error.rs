//! `IpcError`: the only error type that crosses the IPC boundary.
//! Messages are German and user-facing; they never contain secrets or personal data.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorCode {
    VaultLocked,
    WrongPassword,
    NotFound,
    Invalid,
    Network,
    Auth,
    Busy,
    Io,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IpcError {
    pub code: ErrorCode,
    pub message: String,
}

pub type IpcResult<T> = Result<T, IpcError>;

impl IpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn locked() -> Self {
        Self::new(ErrorCode::VaultLocked, "Der Tresor ist gesperrt")
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Invalid, message)
    }
    pub fn not_found(what: &str) -> Self {
        Self::new(ErrorCode::NotFound, format!("{what} nicht gefunden"))
    }
    pub fn busy(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Busy, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for IpcError {}

impl From<ausgeloggt_core::vault::VaultError> for IpcError {
    fn from(e: ausgeloggt_core::vault::VaultError) -> Self {
        use ausgeloggt_core::vault::VaultError as V;
        let code = match &e {
            V::WrongPasswordOrCorrupt => ErrorCode::WrongPassword,
            V::NotAVault | V::UnsupportedVersion(_) | V::NewerSchema(_) | V::AlreadyExists => {
                ErrorCode::Invalid
            }
            V::Io(_) => ErrorCode::Io,
            V::Serde(_) | V::Crypto(_) => ErrorCode::Internal,
        };
        Self::new(code, e.to_string())
    }
}

impl From<std::io::Error> for IpcError {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorCode::Io, format!("Ein-/Ausgabefehler: {e}"))
    }
}

impl From<ausgeloggt_mail::MailError> for IpcError {
    fn from(e: ausgeloggt_mail::MailError) -> Self {
        use ausgeloggt_mail::MailError as M;
        let code = match &e {
            M::Auth => ErrorCode::Auth,
            M::Connect(..) | M::Imap(_) | M::Smtp(_) => ErrorCode::Network,
            M::Invalid(_) => ErrorCode::Invalid,
            M::Cancelled => ErrorCode::Internal,
        };
        Self::new(code, e.to_string())
    }
}

impl From<ausgeloggt_mail::unsubscribe::UnsubscribeError> for IpcError {
    fn from(e: ausgeloggt_mail::unsubscribe::UnsubscribeError) -> Self {
        use ausgeloggt_mail::unsubscribe::UnsubscribeError as U;
        let code = match &e {
            U::NoAutomaticMethod | U::Refused | U::UnsafeTarget => ErrorCode::Invalid,
            U::Http(_) | U::Network(_) | U::Smtp(_) => ErrorCode::Network,
        };
        Self::new(code, e.to_string())
    }
}

impl From<ausgeloggt_breach::BreachError> for IpcError {
    fn from(e: ausgeloggt_breach::BreachError) -> Self {
        use ausgeloggt_breach::BreachError as B;
        let code = match &e {
            B::InvalidKey => ErrorCode::Auth,
            B::Network(_) | B::RateLimited | B::Invalid(_) => ErrorCode::Network,
        };
        Self::new(code, e.to_string())
    }
}

impl From<ausgeloggt_ai::AiError> for IpcError {
    fn from(e: ausgeloggt_ai::AiError) -> Self {
        use ausgeloggt_ai::AiError as A;
        let code = match &e {
            A::Disabled => ErrorCode::Invalid,
            A::OllamaUnreachable(_) | A::Request(_) => ErrorCode::Network,
            A::BadResponse(_) => ErrorCode::Internal,
        };
        Self::new(code, e.to_string())
    }
}

impl From<ausgeloggt_import::ImportError> for IpcError {
    fn from(e: ausgeloggt_import::ImportError) -> Self {
        use ausgeloggt_import::ImportError as I;
        let code = match &e {
            I::UnknownFormat | I::Invalid(_) => ErrorCode::Invalid,
            I::Io(_) => ErrorCode::Io,
        };
        Self::new(code, e.to_string())
    }
}

impl From<ausgeloggt_letters::LetterError> for IpcError {
    fn from(e: ausgeloggt_letters::LetterError) -> Self {
        use ausgeloggt_letters::LetterError as L;
        let code = match &e {
            L::MissingSender(_) | L::MissingRecipient => ErrorCode::Invalid,
            L::Pdf(_) => ErrorCode::Internal,
        };
        Self::new(code, e.to_string())
    }
}

impl From<ausgeloggt_catalog::CatalogError> for IpcError {
    fn from(e: ausgeloggt_catalog::CatalogError) -> Self {
        use ausgeloggt_catalog::CatalogError as C;
        let code = match &e {
            C::Network(_) => ErrorCode::Network,
            C::Invalid(_) => ErrorCode::Internal,
            C::Io(_) => ErrorCode::Io,
        };
        Self::new(code, e.to_string())
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(e: serde_json::Error) -> Self {
        Self::invalid(format!("Ungültige Daten: {e}"))
    }
}
