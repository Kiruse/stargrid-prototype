use log::*;

mod listener;
mod data;
mod error;
mod repeater;

type Result<T> = std::result::Result<T, error::StargridError>;
pub use error::StargridError;

// use repeater::Repeater;

#[tokio::main]
async fn main() -> Result<()> {
	simplelog::TermLogger::init(
		LevelFilter::Debug,
		simplelog::Config::default(),
		simplelog::TerminalMode::Mixed,
		simplelog::ColorChoice::Auto,
	).unwrap();

	let listener = listener::listen("wss://terra-rpc.publicnode.com:443/websocket".to_string()).await?;
	let _repeater = repeater::repeat("127.0.0.1:27043".to_string()).await?;

	listener.subscribe(|event| {
		use listener::ListenerEvent::*;
		match event {
			NewBlock(block) => info!("Block {} at {}", block.height, block.time),
			_ => info!("Event: {:?}", event),
		}
	});

	Ok(())
}
