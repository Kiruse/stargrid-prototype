use std::{borrow::Cow, net::SocketAddr};

use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde_json::json;
use log::*;
use tokio::{net::{TcpListener, TcpStream}, select, sync::mpsc, task::JoinHandle};
use tokio_tungstenite::{accept_async, WebSocketStream};
use tungstenite::{protocol::{frame::coding::CloseCode, CloseFrame}, Message};

use crate::{msg::{Broadcast, RepeaterMessage}, util::LogError, Result, StargridError};

const MAX_MESSAGE_SIZE: usize = 4000;

pub enum RepeaterCommand {
  Broadcast(Broadcast),
  Close,
}

pub async fn repeat(endpoint: String) -> Result<Repeater> {
  let (tx_commands, rx_commands) = mpsc::channel(32);

  let join_handle = tokio::spawn(main_loop(endpoint, rx_commands));

  Ok(Repeater {
    join_handle,
    tx_commands,
  })
}

async fn main_loop(
  endpoint: String,
  mut rx_commands: mpsc::Receiver<RepeaterCommand>,
) {
  let listener = TcpListener::bind(endpoint.clone()).await.expect(format!("Failed to listen on {}", endpoint).as_str());
  info!("Listening on {}", endpoint);

  loop {
    select! {
      Ok((stream, _)) = listener.accept() => {
        tokio::spawn(accept_connection(stream));
      }
      Some(cmd) = rx_commands.recv() => {
        use RepeaterCommand::*;
        use crate::msg::Broadcast::*;
        match cmd {
          Broadcast(Block(block)) => {
            info!("Broadcasting block {}", block.height);
          }
          Broadcast(Tx(tx)) => {
            info!("Broadcasting tx {}", tx.txhash);
          }
          Close => {
            info!("Closing repeater");
            break;
          }
        }
      }
    }
  }
}

async fn accept_connection(
  stream: TcpStream,
  // tx_subscription: mpsc::Sender<RepeaterSubscription>,
  // rx_broadcast: watch::Receiver<Broadcast>,
) {
  let peer = stream.peer_addr().expect("connected streams should have a peer address");
  info!("Peer {} connected", peer);

  let ws = accept_async(stream).await.expect("Failed to accept");
  let (mut write, mut read) = ws.split();

  // TODO: heartbeat ping/pong

  while let Some(msg) = read.next().await {
    if let Err(err) = handle_message(peer, &mut write, msg).await {
      error!("Error reading from peer {}: {:?} - disconnecting.", peer, err);
      write.send(Message::Close(Some(CloseFrame {
        code: CloseCode::Abnormal,
        reason: Cow::from("Error reading from peer (you)"),
      }))).await.log_error();
      write.close().await.log_error();
      break;
    }
  }
}

async fn handle_message(
  peer: SocketAddr,
  write: &mut SplitSink<WebSocketStream<TcpStream>, Message>,
  msg: std::result::Result<Message, tungstenite::Error>,
) -> Result<()> {
  let msg = msg?;

  match msg {
    Message::Close(_) => {
      info!("Peer {} disconnected", peer);
      write.close().await.log_error();
    }
    Message::Binary(_) => {
      write.send(Message::text(
        json!({
          "error": "Binary messages not supported",
        }).to_string()
      )).await.log_error();
    }
    Message::Ping(_) => {
      write.send(Message::Pong(vec![])).await.log_error();
    }
    Message::Text(msg) => {
      if msg.len() > MAX_MESSAGE_SIZE {
        write.send(Message::text(
          json!({
            "error": "Message too large (>4000 characters)",
          }).to_string()
        )).await.log_error();
        return Ok(());
      }

      let msg: RepeaterMessage = serde_json::from_str(&msg)
        .map_err(|err| StargridError::InvalidFormat(format!("{}", err)))?;
      todo!();
      // match msg {
      //   RepeaterMessage::Subscribe(msg) => {

      //   }
      // }
    }
    _ => unreachable!(),
  }

  Ok(())
}

pub struct Repeater {
  pub join_handle: JoinHandle<()>,
  tx_commands: mpsc::Sender<RepeaterCommand>,
}

#[allow(dead_code)]
impl Repeater {
  pub async fn close(&self) -> Result<()> {
    cmd_close(&self.tx_commands).await
  }

  pub async fn broadcast(&self, broadcast: Broadcast) -> Result<()> {
    cmd_broadcast(&self.tx_commands, broadcast).await
  }

  pub fn commands(&self) -> mpsc::Sender<RepeaterCommand> {
    self.tx_commands.clone()
  }
}

pub struct RepeaterCommands(pub mpsc::Sender<RepeaterCommand>);

async fn cmd_close(sender: &mpsc::Sender<RepeaterCommand>) -> Result<()> {
  sender
    .send(RepeaterCommand::Close)
    .await
    .map_err(|err|
      StargridError::Generic(format!("Failed to send close repeater command: {}", err))
    )?;
  Ok(())
}

async fn cmd_broadcast(sender: &mpsc::Sender<RepeaterCommand>, broadcast: Broadcast) -> Result<()> {
  sender
    .send(RepeaterCommand::Broadcast(broadcast))
    .await
    .map_err(|err|
      StargridError::Generic(format!("Failed to send broadcast repeater command: {}", err))
    )?;
  Ok(())
}

#[allow(dead_code)]
impl RepeaterCommands {
  pub async fn close(&self) -> Result<()> {
    cmd_close(&self.0).await
  }

  pub async fn broadcast(&self, broadcast: Broadcast) -> Result<()> {
    cmd_broadcast(&self.0, broadcast).await
  }
}
