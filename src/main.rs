use log::*;
use tokio::join;

mod listener;
mod data;
mod error;
mod publisher;
mod repeater;

type Result<T> = std::result::Result<T, error::StargridError>;
pub use error::StargridError;

use listener::Listener;
use repeater::RepeaterServer;

#[tokio::main]
async fn main() -> Result<()> {
	simplelog::TermLogger::init(
		LevelFilter::Debug,
		simplelog::Config::default(),
		simplelog::TerminalMode::Mixed,
		simplelog::ColorChoice::Auto,
	).unwrap();

	let mut listener = Box::new(Listener::new("wss://terra-rpc.publicnode.com:443/websocket"));
	let repeater = Box::new(RepeaterServer::new("127.0.0.1:27043"));

	listener.events().subscribe(|event| {
		use listener::ListenerEvents::*;
		match event {
			Connect => info!("Listener connected"),
			Reconnect => info!("Listener reconnected"),
			NewBlock(block) => debug!("Received block at height {}, time {} with {} events", block.height, block.time, block.events.len()),
			Disconnect => info!("Listener lost connection, attempting to reconnect..."),
			Close => info!("Listener closed"),
		}
		Ok(())
	});

	let (listener_result, repeater_result,) = join!(
		tokio::spawn(run_listener(listener)),
		tokio::spawn(run_repeater(repeater)),
	);
	listener_result.expect("critical failure in listener")?;
	repeater_result.expect("critical failure in repeater server")?;
	Ok(())
}

async fn run_listener(mut listener: Box<Listener>) -> Result<()> {
	listener.run().await
}

async fn run_repeater(mut repeater: Box<RepeaterServer>) -> Result<()> {
	repeater.run().await
}
