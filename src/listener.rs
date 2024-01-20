use std::task::Poll;

use futures_util::{SinkExt, StreamExt, Future};
use json::{object, array};
use log::*;
use primitive_types::U256;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async_tls_with_config, WebSocketStream, MaybeTlsStream};
use tungstenite::Message;
use tungstenite::protocol::CloseFrame;

use crate::Result;
use crate::data::parse_block;
use crate::error::StargridError;

enum ListenerState {
  /// before any connection attempt has taken place
  Initial,
  /// after a connection has been established or reestablished
  Connected(WebSocketStream<MaybeTlsStream<TcpStream>>),
  /// when the connection has been dropped disorderly. the listener is attempting to reconnect.
  Disconnected,
  /// when the connection has been closed orderly
  Closed,
}

/// listener connects to the blockchain full node and listens for new blocks
pub struct Listener {
  state: ListenerState,
  remote: String,
}

impl Listener {
  pub fn new(endpoint: impl Into<String>) -> Self {
    Self {
      state: ListenerState::Initial,
      remote: endpoint.into(),
    }
  }

  pub async fn run(self: &mut Self) -> Result<()> {
    self.open().await?;

    loop {
      let res = self.next().await;
      match res {
        Ok(Some(msg)) => self.handle_message(msg).await?,
        Ok(None) => {
          info!("closed");
          self.state = ListenerState::Closed;
          break;
        }
        Err(e) => {
          error!("Error: {:?}. Reconnecting.", e);
          self.state = ListenerState::Disconnected;
          self.open().await?;
        }
      }
    }

    Ok(())
  }

  async fn handle_message(self: &mut Self, msg: Message) -> Result<()> {
    let ListenerState::Connected(ref mut ws) = self.state else {
      return Err(StargridError::Assertion("Received message while not connected".into()));
    };

    match msg {
      Message::Ping(_) => ws.send(Message::Pong(vec![])).await?,
      Message::Pong(_) => debug!("received pong"),
      Message::Close(close_frame) => {
        if let Some(CloseFrame { code, reason }) = close_frame {
          info!("Remote closing connection with code {:?} and reason {:?}", code, reason);
        }
        ws.close(None).await?;
        self.state = ListenerState::Disconnected;
        self.open().await?;
      }
      Message::Binary(_) => error!("Unexpected binary message, skipping"),
      Message::Text(msg) => {
        let _block = parse_block(msg.as_str())?;
        todo!();
      }
      _ => todo!(),
    }

    Ok(())
  }

  async fn open(self: &mut Self) -> Result<()> {
    if let ListenerState::Connected(_) = self.state {
      return Ok(());
    }

    let (mut ws, res) = connect_async_tls_with_config(
      self.remote.clone(),
      None,
      false,
      None,
    ).await?;

    if res.body().is_some() {
      error!("Unexpected body in response: {:?}", res.body().as_ref().unwrap());
      return Err(StargridError::InvalidWSMessage("Unexpected body in connection response".into()));
    }

    info!("Connected to {}", self.remote);
    ws.send(Message::Text(json::stringify(object! {
      "jsonrpc" => "2.0",
      "id" => 1,
      "method" => "subscribe",
      "params" => array!["tm.event='NewBlock'"],
    }))).await?;

    let Some(res) = ws.next().await else {
      return Err(StargridError::InvalidWSMessage("Unexpected EOF in connection response".into()));
    };
    let msg = res?;
    match msg {
      Message::Text(msg) => {
        let msg = json::parse(msg.as_str())?;
        if !msg.is_object() || msg["jsonrpc"] != "2.0" || msg["id"] != 1 {
          return Err(StargridError::InvalidWSMessage("Invalid subscription response".into()));
        }
        debug!("Successfully subscribed to NewBlock events");
      }
      _ => {
        return Err(StargridError::InvalidWSMessage("Unexpected subscription response".into()));
      }
    }

    self.state = ListenerState::Connected(ws);
    Ok(())
  }

  pub async fn close(self: &mut Self) -> Result<()> {
    if let ListenerState::Connected(ref mut ws) = self.state {
      ws.close(None).await?;
    }
    self.state = ListenerState::Closed;
    Ok(())
  }

  pub fn is_open(self: &Self) -> bool {
    match self.state {
      ListenerState::Connected(_) => true,
      _ => false,
    }
  }

  fn next(self: &mut Self) -> FutureMessage {
    FutureMessage(self)
  }
}

struct FutureMessage<'a>(&'a mut Listener);

impl<'a> Future for FutureMessage<'a> {
  type Output = Result<Option<Message>>;

  fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
    use ListenerState::*;
    let fut = self.get_mut();
    let result = match fut.0.state {
      Connected(ref mut ws) => {
        match ws.poll_next_unpin(cx) {
          Poll::Pending => Poll::Pending,
          Poll::Ready(msg) => Poll::Ready(Ok(msg.transpose()?)),
        }
      }
      Closed => Poll::Ready(Ok(None)),
      _ => Poll::Pending,
    };
    debug!("Listener poll result: {:?}", result);
    result
  }
}
