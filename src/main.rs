use listener::ListenerEvent;
use log::*;
use repeater::RepeaterCommands;

mod listener;
mod data;
mod error;
mod msg;
mod repeater;
mod util;

use crate::error::StargridError;
use crate::util::LogError;

type Result<T> = std::result::Result<T, error::StargridError>;

#[tokio::main]
async fn main() -> Result<()> {
	simplelog::TermLogger::init(
		LevelFilter::Debug,
		simplelog::Config::default(),
		simplelog::TerminalMode::Mixed,
		simplelog::ColorChoice::Auto,
	).unwrap();

	let listener = listener::listen("wss://terra-rpc.publicnode.com:443/websocket".to_string()).await?;
	let repeater = repeater::repeat("127.0.0.1:27043".to_string()).await?;

	let mut rx_events = listener.events();
	let repeater_commands = repeater.commands();

	// propagate blocks & txs to repeater
	tokio::spawn(async move {
		let repeater_commands = RepeaterCommands(repeater_commands);
		loop {
			if let Err(err) = rx_events.changed().await {
				error!("Failed to receive event: {}", err);
				break;
			}

			let event = rx_events.borrow_and_update().clone();
			match event {
				ListenerEvent::Broadcast(ref broadcast) => {
					repeater_commands.broadcast(broadcast.clone()).await.log_error();
				}
				ListenerEvent::Close => break,
				_ => {},
			}
		}
	}).await.log_error();

	listener.join_handle.await.log_error();
	repeater.join_handle.await.log_error();

	Ok(())
}
