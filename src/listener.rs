use futures_util::{SinkExt, StreamExt};
use log::*;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async_tls_with_config, WebSocketStream, MaybeTlsStream};
use tungstenite::Message;
use tungstenite::protocol::CloseFrame;

use crate::msg::Broadcast;
use crate::Result;
use crate::data::{parse_block, parse_tx, validate_jsonrpc, SUB_ID_BLOCK, SUB_ID_TX};
use crate::error::StargridError;
use crate::util::LogError;

pub enum ListenerCommand {
  Close,
}

#[derive(Debug, Clone)]
pub enum ListenerEvent {
  Initial,
  Connect,
  Reconnect,
  Broadcast(Broadcast),
  Disconnect,
  Close,
}

pub async fn listen(endpoint: String) -> Result<Listener> {
  let (tx_commands, rx_commands) = mpsc::channel(32);
  let (tx_events, rx_events) = watch::channel(ListenerEvent::Initial);

  let join_handle = tokio::spawn(main_loop(endpoint, rx_commands, tx_events));

  Ok(Listener {
    join_handle,
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
                match parse_text_msg(&msg) {
                  Ok(data) => {
                    try_send_event(&tx_events, ListenerEvent::Broadcast(data));
                  }
                  Err(err) => {
                    error!("Error parsing message: {:?}", err);
                  }
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
  sender.send(event).log_error();
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
    return Err(StargridError::InvalidListenerMessage("Unexpected body in connection response".into()));
  }

  debug!("Connected to {}", endpoint);

  // subscribe to NewBlock events
  ws.send(Message::Text(msg_subscribe(SUB_ID_BLOCK, "NewBlock"))).await?;
  let resid = parse_subscription_response(&next_msg(&mut ws).await?)?;
  if resid != SUB_ID_BLOCK {
    return Err(StargridError::InvalidListenerMessage(format!("Invalid subscription ID {}, expected 1", resid)));
  }

  ws.send(Message::Text(msg_subscribe(SUB_ID_TX, "Tx"))).await?;
  let resid = parse_subscription_response(&next_msg(&mut ws).await?)?;
  if resid != SUB_ID_TX {
    return Err(StargridError::InvalidListenerMessage(format!("Invalid subscription ID {}, expected 2", resid)));
  }

  Ok(ws)
}

async fn next_msg(ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> Result<Message> {
  Ok(ws.next().await.ok_or(StargridError::InvalidListenerMessage("Unexpected EOF in connection response".into()))??)
}

/// Listener API to simplify interacting with the Listener async/thread
pub struct Listener {
  pub join_handle: JoinHandle<()>,
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

  pub fn events(&self) -> watch::Receiver<ListenerEvent> {
    self.rx_events.clone()
  }
}

fn parse_text_msg(msg: &str) -> Result<Broadcast> {
  let msg: serde_json::Value = serde_json::from_str(msg)?;
  validate_jsonrpc(&msg)?;

  match msg["id"].as_u64() {
    Some(SUB_ID_BLOCK) => {
      Ok(Broadcast::Block(parse_block(&msg)?))
    }
    Some(SUB_ID_TX) => {
      Ok(Broadcast::Tx(parse_tx(&msg)?))
    }
    _ => todo!(),
  }
}

fn msg_subscribe(id: u64, typ: impl Into<String>) -> String {
  serde_json::json!({
    "jsonrpc": "2.0",
    "id": id,
    "method": "subscribe",
    "params": [format!("tm.event='{}'", typ.into())],
  }).to_string()
}

fn parse_subscription_response(msg: &Message) -> Result<u64> {
  let Message::Text(msg) = msg else {
    return Err(StargridError::InvalidListenerMessage("Unexpected non-text subscription response".into()));
  };

  let response: serde_json::Value = serde_json::from_str(msg.as_str())?;

  if response["jsonrpc"] != "2.0" {
    return Err(StargridError::InvalidListenerMessage(format!("Invalid JSONRPC version (expected \"2.0\", got \"{}\")", response["jsonrpc"])));
  }
  Ok(response["id"].as_u64().ok_or(StargridError::InvalidListenerMessage("Missing subscription ID".into()))?)
}
