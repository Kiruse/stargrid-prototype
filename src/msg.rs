use std::collections::HashMap;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::data::{AttributeValue, Block, Tx};

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub enum RepeaterMessage {
  Subscribe(RepeaterSubscription),
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub enum RepeaterSubscription {
  /// Subscribe to whole blocks
  Blocks,
  /// Subscribe to specific transactions by log events
  Txs(RepeaterTxSubscription),
}

#[derive(Deserialize, Serialize, Debug)]
pub struct RepeaterTxSubscription {
  /// ID of this subscription unique to the client. If the ID is already in use, responds with an error.
  pub id: u64,
  /// Filters for one or more event types and/or attributes. Cannot be empty. All filters must apply.
  pub filters: Vec<EventFilter>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct EventFilter {
  /// Name of the event to filter for
  name: String,
  /// Attribute filters to apply
  attributes: HashMap<String, AttributeFilter>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub enum AttributeFilter {
  /// At least one of the given sub-filters
  AnyOf(Vec<AttributeFilter>),
  /// All of the given sub-filters
  AllOf(Vec<AttributeFilter>),
  /// Invert the given sub-filter
  Not(Box<AttributeFilter>),
  /// Glob-like matching
  Match(String),
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all="camelCase")]
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
  pub fn matches(&self, tx: &Tx) -> bool {
    self.filters.iter().all(|filter| filter.matches(tx))
  }
}

impl EventFilter {
  pub fn matches(&self, tx: &Tx) -> bool {
    tx.events
      .iter()
      .filter(|e| e.name == self.name)
      .any(|e| {
        if self.attributes.is_empty() {
          return true;
        }

        self.attributes.iter().all(|(key, filter)| {
          e.attributes.iter().any(|(k, v)| {
            k == key && filter.matches(v)
          })
        })
      })
  }
}

impl AttributeFilter {
  pub fn matches(&self, av: &AttributeValue) -> bool {
    use AttributeFilter::*;
    let Some(value) = av.value.as_str() else { return false; };
    match self {
      Match(pattern) => {
        enum Mode {
          Normal,
          Escape,
        }

        let mut rxpat = String::with_capacity(pattern.len() * 2);
        let mut mode = Mode::Normal;

        rxpat.push_str("^");
        for c in pattern.chars() {
          match c {
            '*' => rxpat.push_str(".*"),
            '?' => rxpat.push_str("."),
            '\\' => {
              mode = Mode::Escape;
            }
            _ => {
              match mode {
                Mode::Normal => rxpat.push(c),
                Mode::Escape => {
                  rxpat.push('\\');
                  rxpat.push(c);
                  mode = Mode::Normal;
                }
              }
            }
          }
        }
        rxpat.push_str("$");

        let Ok(rx) = Regex::new(&rxpat) else { return false };
        // log::debug!("{} =~ {} == {}", value, rxpat, rx.is_match(value));
        rx.is_match(value)
      }
      AllOf(filters) =>
        filters.iter().all(|filter| filter.matches(av)),
      AnyOf(filters) =>
        filters.iter().any(|filter| filter.matches(av)),
      Not(filter) =>
        !filter.matches(av),
    }
  }
}
