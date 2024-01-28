use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::data::{Block, Tx};
use crate::error::StargridError;

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
  /// Necessary non-event for tokio watch channels
  None,
  /// Broadcast a block
  Block(Block),
  /// Broadcast a transaction
  Tx(Tx),
  /// Broadcast that the Stargrid server is terminating
  Close,
}

impl RepeaterTxSubscription {
  pub fn validate(&self) -> Result<()> {
    for (key, filter) in self.filters.iter() {
      key.split_once(".").ok_or(StargridError::InvalidFormat("Filter key must be in format <type>.<attribute>".into()))?;
      if let RepeaterTxFilter::Glob(_) = filter {
        return Err(StargridError::InvalidFormat("Glob filters are not supported yet".into()));
      }
    }
    Ok(())
  }

  pub fn matches(&self, tx: &Tx) -> bool {
    for (key, filter) in self.filters.iter() {
      use RepeaterTxFilter::*;
      let (event, attr) = key.split_once(".").unwrap();

      let event = tx.events
        .iter()
        .find(|item| item.name == event);
      let Some(event) = event else {
        return false;
      };

      let attr = event.attributes
        .iter()
        .find(|(key, _)| *key == attr);
      let Some(attr) = attr else {
        return false;
      };

      let Some(val) = attr.1.value.as_str() else {
        return false;
      };

      match filter {
        Exact(value) => {
          if val != value {
            return false;
          }
        }
        StartsWith(value) => {
          if !val.starts_with(value) {
            return false;
          }
        }
        EndsWith(value) => {
          if !val.ends_with(value) {
            return false;
          }
        }
        Contains(value) => {
          if !val.contains(value) {
            return false;
          }
        }
        Glob(_) => todo!(),
      }
    }
    true
  }
}
