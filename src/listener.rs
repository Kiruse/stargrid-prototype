use futures_util::{SinkExt, StreamExt};
use json::{object, array};
use log::*;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{connect_async_tls_with_config, WebSocketStream, MaybeTlsStream};
use tungstenite::Message;
use tungstenite::protocol::CloseFrame;

use crate::Result;
use crate::data::{parse_block, Block};
use crate::error::StargridError;

pub enum ListenerCommand {
  Close,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum ListenerEvent {
  Initial,
  Connect,
  Reconnect,
  NewBlock(Block),
  Disconnect,
  Close,
}

pub async fn listen(endpoint: String) -> Result<Listener> {
  let (tx_commands, rx_commands) = mpsc::channel(32);
  let (tx_events, rx_events) = watch::channel(ListenerEvent::Initial);

  tokio::spawn(main_loop(endpoint, rx_commands, tx_events));

  Ok(Listener {
    tx_commands,
    rx_events,
  })
}

async fn main_loop(
  endpoint: String,
  mut rx_commands: mpsc::Receiver<ListenerCommand>,
  tx_events: watch::Sender<ListenerEvent>,
) {
  let mut ws = connect(endpoint.clone()).await;
  try_send_event(&tx_events, ListenerEvent::Connect);

  loop {
    select! {
      Some(cmd) = rx_commands.recv() => {
        use ListenerCommand::*;
        match cmd {
          Close => {
            try_send_event(&tx_events, ListenerEvent::Close);
            break;
          }
        }
      }
      Some(msg) = ws.next() => {
        match msg {
          Ok(msg) => {
            match msg {
              Message::Ping(_) => ws.send(Message::Pong(vec![])).await.log_error(),
              Message::Pong(_) => debug!("received pong"),
              Message::Close(close_frame) => {
                if let Some(CloseFrame { code, reason }) = close_frame {
                  info!("Remote closing connection with code {:?} and reason {:?}", code, reason);
                }
                ws.close(None).await.log_error();
                try_send_event(&tx_events, ListenerEvent::Disconnect);
                ws = connect(endpoint.clone()).await;
                try_send_event(&tx_events, ListenerEvent::Reconnect);
              }
              Message::Binary(_) => error!("Unexpected binary message, skipping"),
              Message::Text(msg) => {
                let res = parse_block(msg.as_str())
                  .map(|block| tx_events.send(ListenerEvent::NewBlock(block)));
                if let Err(err) = res {
                  error!("Error parsing/sending block: {:?}", err);
                }
              }
              _ => todo!(),
            }
          }
          Err(err) => {
            error!("Error receiving message: {:?}", err);
            try_send_event(&tx_events, ListenerEvent::Disconnect);
            ws = connect(endpoint.clone()).await;
            try_send_event(&tx_events, ListenerEvent::Reconnect);
          }
        }
      }
    }
  }
}

fn try_send_event(sender: &watch::Sender<ListenerEvent>, event: ListenerEvent) {
  sender.send(event).unwrap_or_else(|err| {
    error!("Error sending event: {:?}", err);
  });
}

async fn connect(endpoint: String) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
  let mut attempt = 0u32;
  loop {
    debug!("Connecting to {}, attempt {}", endpoint, attempt+1);
    match try_connect(endpoint.clone()).await {
      Ok(ws) => {
        return ws;
      }
      Err(err) => {
        error!("Error connecting to {}: {:?}", endpoint, err);
        tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
        attempt += 1;
      }
    }
  }
}

async fn try_connect(endpoint: String) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>> {
  let (mut ws, res) = connect_async_tls_with_config(
    endpoint.clone(),
    None,
    false,
    None,
  ).await?;

  if res.body().is_some() {
    error!("Unexpected body in response: {:?}", res.body().as_ref().unwrap());
    return Err(StargridError::InvalidWSMessage("Unexpected body in connection response".into()));
  }

  debug!("Connected to {}", endpoint);

  // subscribe to NewBlock events
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

  Ok(ws)
}

/// Listener API to simplify interacting with the Listener async/thread
pub struct Listener {
  tx_commands: mpsc::Sender<ListenerCommand>,
  rx_events: watch::Receiver<ListenerEvent>,
}

impl Listener {
  #[allow(dead_code)]
  pub async fn close(&self) -> Result<()> {
    self.tx_commands
      .send(ListenerCommand::Close)
      .await
      .map_err(|err|
        StargridError::Generic(format!("Error sending close command: {:?}", err))
      )
  }

  pub fn subscribe(&self, mut callback: impl FnMut(ListenerEvent) + 'static + Send) {
    let mut rx_events = self.rx_events.clone();
    tokio::spawn(async move {
      let mut closing = false;
      while !closing {
        let event = rx_events.borrow_and_update();
        if let ListenerEvent::Close = *event {
          closing = true;
        }
        callback(event.to_owned());
      }
    });
  }
}

trait LogError {
  fn log_error(self);
}

impl<E> LogError for std::result::Result<(), E>
where E: std::fmt::Debug
{
  fn log_error(self) {
    match self {
      Err(err) => {
        error!("Error: {:?}", err);
      }
      _ => (),
    }
  }
}
