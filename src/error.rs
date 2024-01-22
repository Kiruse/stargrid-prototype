use thiserror::Error;

#[derive(Debug, Error)]
pub enum StargridError {
  #[error("Invalid Block format")]
  InvalidBlockFormat(String),

  #[error("Invalid WebSocket message: {0}")]
  InvalidWSMessage(String),

  #[error("IO Error: {0}")]
  IOError(#[from] std::io::Error),

  #[error("Tungstenite Error: {0}")]
  WebSocketError(#[from] tungstenite::Error),

  #[error("JSON Error: {0}")]
  JsonError(#[from] json::Error),

  #[error("{0}")]
  Generic(String),

  #[error("{0}")]
  Assertion(String),

  #[error("Aggregate error: {0:?}")]
  Aggregate(Vec<StargridError>),
}

impl From<String> for StargridError {
  fn from(s: String) -> Self {
    StargridError::Generic(s)
  }
}

impl From<&str> for StargridError {
  fn from(s: &str) -> Self {
    StargridError::Generic(s.to_string())
  }
}
