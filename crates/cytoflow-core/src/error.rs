use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid FCS file: {0}")]
    Fcs(String),
    #[error("Unsupported FCS feature: {0}")]
    Unsupported(String),
    #[error("Channel not found: {0}")]
    ChannelNotFound(String),
    #[error("Gate not found: {0}")]
    GateNotFound(String),
    #[error("Sample not found: {0}")]
    SampleNotFound(String),
    #[error("Invalid gate: {0}")]
    InvalidGate(String),
    #[error("Compensation error: {0}")]
    Compensation(String),
    #[error("Workspace error: {0}")]
    Workspace(String),
    #[error("XML error: {0}")]
    Xml(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<quick_xml::Error> for Error {
    fn from(e: quick_xml::Error) -> Self {
        Error::Xml(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
