use thiserror::Error;

#[derive(Debug, Error)]
pub enum StargridError {
  #[error("Failed to parse: {0}")]
  ParseError(String),

  #[error("Expected value \"{0}\", got \"{1}\"")]
  UnexpectedValue(String, String),

  #[error("Invalid Block format")]
  InvalidFormat(String),

  #[error("Invalid WebSocket message: {0}")]
  InvalidListenerMessage(String),

  #[error("IO Error: {0}")]
  IOError(#[from] std::io::Error),

  #[error("Tungstenite Error: {0}")]
  WebSocketError(#[from] tungstenite::Error),

  #[error("JSON Error: {0}")]
  JsonError(#[from] serde_json::Error),

  #[error("{0}")]
  Generic(String),
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

impl StargridError {
  pub fn unexpected_value(expected: impl std::fmt::Debug, got: impl std::fmt::Debug) -> Self {
    StargridError::UnexpectedValue(format!("{:?}", expected), format!("{:?}", got))
  }
}
