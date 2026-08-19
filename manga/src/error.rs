use reqwest::StatusCode;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    #[error("Page not found")]
    PageNotFound,

    #[error("Too many requests")]
    TooManyRequests,

    #[error("Internal server error")]
    InternalServerError,

    #[error("Bad request")]
    BadRequest,

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Unknown error: {0}")]
    Unknown(String),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    HttpError(HttpError),

    #[error("Request error")]
    RequestError,

    #[error("Invalid header")]
    InvalidHeader,

    // TODO: Add detail?
    #[error("Decode error")]
    DecodeError,

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Invalid Url")]
    InvalidUrl,

    #[error("Invalid page")]
    InvalidPage,
}

impl ClientError {
    pub fn from_status(status: StatusCode) -> Self {
        let error = match status {
            StatusCode::NOT_FOUND => HttpError::PageNotFound,
            StatusCode::TOO_MANY_REQUESTS => HttpError::TooManyRequests,
            StatusCode::INTERNAL_SERVER_ERROR => HttpError::InternalServerError,
            StatusCode::BAD_REQUEST => HttpError::BadRequest,
            StatusCode::UNAUTHORIZED => HttpError::Unauthorized,
            StatusCode::FORBIDDEN => HttpError::Forbidden,
            _ => HttpError::Unknown(status.as_u16().to_string()),
        };
        Self::HttpError(error)
    }
}

#[derive(Error, Debug, Clone)]
pub enum PipelineError {
    #[error("Payment required")]
    PaymentRequired,

    #[error("Client error")]
    ClientError(#[from] ClientError),

    #[error("I/O error")]
    IoError,

    #[error("Solve error")]
    SolveError,

    #[error("Download error")]
    DownloadError,

    #[error("Progress error")]
    ProgressError,

    #[error("Unknown error")]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_http_status() {
        assert_eq!(
            ClientError::from_status(StatusCode::NOT_FOUND),
            ClientError::HttpError(HttpError::PageNotFound)
        );
    }

    #[test]
    fn preserves_unknown_http_status_code() {
        assert_eq!(
            ClientError::from_status(StatusCode::IM_A_TEAPOT),
            ClientError::HttpError(HttpError::Unknown("418".to_owned()))
        );
    }
}
