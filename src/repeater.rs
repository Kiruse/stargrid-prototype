use std::net::SocketAddr;

use futures_util::{SinkExt, StreamExt};
use log::*;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, WebSocketStream};
use tungstenite::Message;

use crate::Result;

/// repeater server. repeater clients can connect to this server to receive repeated txs & events.
pub struct RepeaterServer {
  addr: String,
  listener: Option<TcpListener>,
  clients: Vec<RepeaterHandler>,
}

impl RepeaterServer {
  pub  fn new(addr: &str) -> Self {
    Self {
      addr: addr.to_string(),
      listener: None,
      clients: vec![],
    }
  }

  pub async fn run(self: &mut Self) -> Result<()> {
    self.listener = Some(TcpListener::bind(self.addr.clone()).await.expect("RepeaterServer failed to bind"));
    let tcp = self.listener.as_ref().unwrap();

    info!("Listening on: {}", self.addr);
    while let Ok((stream, _)) = tcp.accept().await {
      let peer = stream.peer_addr().expect("connected streams should have a peer address");
      info!("Peer {} connected", peer);
      tokio::spawn(accept_connection(stream));
    }
    Ok(())
  }
}

async fn accept_connection(stream: TcpStream) {
  let peer = stream.peer_addr().expect("connected streams should have a peer address");
  if let Err(e) = handle_connection(stream).await {
    error!("Error with peer {}: {:?}", peer, e);
  }
}

async fn handle_connection(stream: TcpStream) -> Result<()> {
  let peer = stream.peer_addr()?;
  let ws = accept_async(stream).await?;
  info!("Peer {} successfully established WebSocket connection", peer);
  RepeaterHandler::new(ws).run().await?;
  Ok(())
}

/// server-side handler of repeater clients
struct RepeaterHandler {
  ws: WebSocketStream<TcpStream>,
}

impl RepeaterHandler {
  pub fn new(ws: WebSocketStream<TcpStream>) -> Self {
    Self {
      ws,
    }
  }

  pub async fn run(self: &mut Self) -> Result<()> {
    while let Some(msg) = self.ws.next().await {
      let msg = msg?;
      info!("RepeaterHandler received msg from {}: {:?}", self.peer_addr(), msg);
      // TODO: serde_json
      self.handle_message(msg).await?;
    }
    Ok(())
  }

  pub async fn handle_message(self: &Self, _msg: Message) -> Result<()> {
    panic!("not yet implemented");
  }

  pub fn peer_addr(self: &Self) -> SocketAddr {
    self.ws.get_ref().peer_addr().expect("connected streams should have a peer address")
  }
}
