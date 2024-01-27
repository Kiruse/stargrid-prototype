use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::data::{Block, Tx};

#[derive(Deserialize, Serialize, Debug)]
pub enum RepeaterMessage {
  Subscribe(RepeaterSubscription),
}

#[derive(Deserialize, Serialize, Debug)]
pub enum RepeaterSubscription {
  /// Subscribe to whole blocks
  Blocks,
  /// Subscribe to specific transactions by log events
  Txs(RepeaterTxSubscription),
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RepeaterTxSubscription {
  /// Filters for one or more event types and/or attributes. Cannot be empty. All filters must apply.
  filters: HashMap<String, RepeaterTxFilter>,
}

#[derive(Deserialize, Serialize, Debug)]
pub enum RepeaterTxFilter {
  /// Filter event type or attribute by exact match
  Exact(String),
  /// Filter event type or attribute by prefix match
  StartsWith(String),
  /// Filter event type or attribute by suffix match
  EndsWith(String),
  /// Filter event type or attribute by substring inclusion
  Contains(String),
  /// Filter event type or attribute by simple glob pattern
  Glob(String),
}

#[derive(Clone, Serialize, Debug)]
pub enum Broadcast {
  Block(Block),
  Tx(Tx),
}
