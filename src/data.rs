use log::*;
use primitive_types::U256;

use crate::Result;

#[derive(Debug, Eq, PartialEq)]
pub struct Block {
  height: U256,
}

pub fn parse_block(data: &str) -> Result<Block> {
  let data = json::parse(data)?;
  info!("block data: {:?}", data);
  todo!();
}
