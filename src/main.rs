use log::LevelFilter;
use tokio::join;

mod listener;
mod data;
mod error;
mod repeater;

type Result<T> = std::result::Result<T, error::StargridError>;

#[tokio::main]
async fn main() -> Result<()> {
	simplelog::TermLogger::init(
		LevelFilter::Debug,
		simplelog::Config::default(),
		simplelog::TerminalMode::Mixed,
		simplelog::ColorChoice::Auto,
	).unwrap();

	let (listener_result, repeater_result,) = join!(
		tokio::spawn(run_listener("wss://terra-rpc.publicnode.com:443/websocket")),
		tokio::spawn(run_repeater("127.0.0.1:27043")),
	);
	listener_result.expect("critical failure in listener")?;
	repeater_result.expect("critical failure in repeater server")?;
	Ok(())
}

async fn run_listener(addr: &str) -> Result<()> {
	listener::Listener::new(addr).run().await
}

async fn run_repeater(addr: &str) -> Result<()> {
	repeater::RepeaterServer::new(addr).run().await
}
