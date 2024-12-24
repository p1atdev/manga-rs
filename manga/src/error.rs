use thiserror::Error;

#[derive(Error, Debug, Clone)]
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

#[derive(Error, Debug, Clone)]
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
