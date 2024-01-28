use std::{borrow::Cow, net::SocketAddr};

use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde_json::json;
use log::*;
use tokio::{net::{TcpListener, TcpStream}, select, sync::{mpsc, watch}, task::JoinHandle};
use tokio_tungstenite::{accept_async, WebSocketStream};
use tungstenite::{protocol::{frame::coding::CloseCode, CloseFrame}, Message};

use crate::{data::Tx, msg::{Broadcast, RepeaterMessage, RepeaterSubscription}, util::LogError, Result, StargridError};

const MAX_MESSAGE_SIZE: usize = 4000;
const MAX_FREE_SUBSCRIPTIONS: usize = 20;

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

  let (tx_broadcast, rx_broadcast) = watch::channel(Broadcast::None);

  loop {
    select! {
      Ok((stream, _)) = listener.accept() => {
        let rx_broadcast = rx_broadcast.clone();
        tokio::spawn(async move {
          accept_connection(stream, rx_broadcast.clone()).await.log_error();
        });
      }
      Some(cmd) = rx_commands.recv() => {
        match cmd {
          RepeaterCommand::Broadcast(broadcast) => {
            match broadcast {
              // note: we don't broadcast close broadcasts, it's considered an internal event and is
              // not enforced when when emitted from the outside
              Broadcast::Block(block) => {
                info!("Broadcasting block {}", block.height);
                tx_broadcast.send(Broadcast::Block(block)).log_error();
              }
              Broadcast::Tx(tx) => {
                info!("Broadcasting tx {}", tx.txhash);
                tx_broadcast.send(Broadcast::Tx(tx)).log_error();
              }
              _ => {}
            }
          }
          RepeaterCommand::Close => {
            info!("Closing repeater");
            // close broadcast allows remote clients to close gracefully
            tx_broadcast.send(Broadcast::Close).log_error();
            break;
          }
        }
      }
    }
  }
}

async fn accept_connection(
  stream: TcpStream,
  mut rx_broadcast: watch::Receiver<Broadcast>,
) -> Result<()> {
  let peer = stream.peer_addr().expect("connected streams should have a peer address");
  info!("Peer {} connected", peer);

  let ws = accept_async(stream).await?;
  let (mut write, mut read) = ws.split();

  let mut subscriptions: Vec<RepeaterSubscription> = vec![];

  // TODO: heartbeat ping/pong

  loop {
    select! {
      Some(msg) = read.next() => {
        let msg = msg?;
        let ctx = ClientContext {
          peer,
          write: &mut write,
          subscriptions: &mut subscriptions,
        };

        if let Err(err) = handle_message(ctx, msg).await {
          write.send(Message::Text(
            json!({
              "error": format!("{}", err),
            }).to_string()
          )).await.log_error();
        }
      }
      Ok(()) = rx_broadcast.changed() => {
        let ctx = ClientContext {
          peer,
          write: &mut write,
          subscriptions: &mut subscriptions,
        };

        let broadcast = rx_broadcast.borrow_and_update().clone();
        match broadcast {
          Broadcast::Block(block) => {
            if ctx.has_block_subscription() {
              write.send(Message::text(
                json!({
                  "block": &block,
                }).to_string()
              )).await.log_error();
            }
          }
          Broadcast::Tx(tx) => {
            if ctx.has_tx_subscription(&tx) {
              debug!("Sending tx {} to {}", tx.txhash, peer);
              write.send(Message::text(
                json!({
                  "tx": &tx
                }).to_string()
              )).await.log_error();
            }
          }
          Broadcast::Close => {
            write.send(Message::Close(Some(CloseFrame {
              code: CloseCode::Normal,
              reason: Cow::from("Repeater closed"),
            }))).await.log_error();
            write.close().await.log_error();
            break;
          }
          _ => {}
        }
      }
    }
  }

  Ok(())
}

async fn handle_message<'a>(mut ctx: ClientContext<'a>, msg: Message) -> Result<()> {
  match msg {
    Message::Close(_) => {
      info!("Peer {} disconnected", ctx.peer);
      ctx.write.close().await.log_error();
    }
    Message::Binary(_) => {
      ctx.write.send(Message::text(
        json!({
          "error": "Binary messages not supported",
        }).to_string()
      )).await.log_error();
    }
    Message::Ping(_) => {
      ctx.write.send(Message::Pong(vec![])).await.log_error();
    }
    Message::Text(msg) => {
      if msg.len() > MAX_MESSAGE_SIZE {
        return Err(StargridError::Generic(format!("Message too large (>{} characters)", MAX_MESSAGE_SIZE)));
      }

      let msg: RepeaterMessage = serde_json::from_str(&msg)?;
      debug!("Received message from {}: {:?}", ctx.peer, msg);
      match msg {
        RepeaterMessage::Subscribe(subscription) => {
          match subscription {
            RepeaterSubscription::Blocks => {
              if !ctx.has_block_subscription() {
                ctx.push_subscription(subscription)?;
              }
            }
            RepeaterSubscription::Txs(ref tx) => {
              // TODO: dedupe tx subscriptions efficiently
              let id = tx.id;
              tx.validate()?;

              if ctx.has_tx_id(id) {
                return Err(StargridError::Generic(format!("Subscription ID {} already in use", id)));
              } else {
                ctx.push_subscription(subscription)?;
                ctx.write.send(Message::text(
                  json!({
                    "id": id,
                  }).to_string()
                )).await.log_error();
              }
            }
          }
        }
      }
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

struct ClientContext<'a> {
  peer: SocketAddr,
  write: &'a mut SplitSink<WebSocketStream<TcpStream>, Message>,
  subscriptions: &'a mut Vec<RepeaterSubscription>,
}

impl<'a> ClientContext<'a> {
  pub fn has_block_subscription(&self) -> bool {
    self.subscriptions.iter().find(|item| {
      if let RepeaterSubscription::Blocks = item { true } else { false }
    }).is_some()
  }

  /// find the first RepeaterSubscription that matches the given Tx - tho there can be more
  pub fn find_tx_subscription(&self, tx: &Tx) -> Option<&RepeaterSubscription> {
    self.subscriptions
      .iter()
      .find(|item| {
        if let RepeaterSubscription::Txs(subscription) = item {
          subscription.matches(tx)
        } else {
          false
        }
      })
  }

  pub fn has_tx_subscription(&self, tx: &Tx) -> bool {
    self.find_tx_subscription(tx).is_some()
  }

  pub fn has_tx_id(&self, id: u64) -> bool {
    self.subscriptions
      .iter()
      .find(|item| {
        if let RepeaterSubscription::Txs(subscription) = item {
          subscription.id == id
        } else {
          false
        }
      })
      .is_some()
  }

  pub fn push_subscription(&mut self, subscription: RepeaterSubscription) -> Result<()> {
    if self.subscriptions.len() >= MAX_FREE_SUBSCRIPTIONS {
      return Err(StargridError::BudgetError("Too many subscriptions".into()))
    }
    self.subscriptions.push(subscription);
    Ok(())
  }
}
