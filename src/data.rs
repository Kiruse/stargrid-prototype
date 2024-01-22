use std::{collections::HashMap, ops::Index};

use iso8601_timestamp::Timestamp;
use json::JsonValue;
use primitive_types::U256;

use crate::{Result, StargridError};

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Block {
  pub raw: String,
  pub height: U256,
  pub hash: String,
  pub chain_id: String,
  pub time: Timestamp,
  pub events: Vec<Event>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Event {
  pub name: String,
  pub attributes: HashMap<String, AttributeValue>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct AttributeValue(pub bool, pub JsonValue);

pub fn parse_block(data: &str) -> Result<Block> {
  let raw = data.to_string();
  let data = json::parse(data)?;
  if !data.is_object() || data["jsonrpc"] != "2.0" || data["id"] != 1 {
    return Err(StargridError::InvalidBlockFormat("Invalid JSON-RPC response".into()));
  }

  let payload = get_payload(&data)?;

  let result_begin_block = get_path(
    &payload,
    &["value", "result_begin_block"],
    "Missing result_begin_block",
  )?;
  let result_end_block = get_path(
    &payload,
    &["value", "result_end_block"],
    "Missing result_end_block",
  )?;

  let block_begin_events = parse_events(&result_begin_block["events"])?;
  let block_end_events = parse_events(&result_end_block["events"])?;
  let events = [block_begin_events, block_end_events].concat();

  Ok(Block {
    raw,
    height: get_block_height(&payload)?,
    hash: get_block_hash(&payload)?,
    chain_id: get_chain_id(&payload)?,
    time: get_block_timestamp(&payload)?,
    events,
  })
}

fn get_payload(data: &JsonValue) -> Result<JsonValue> {
  let data = get_path(&data, &["result", "data"], "Expected .result.data")?;
  if data["type"] != "tendermint/event/NewBlock" {
    return Err(StargridError::InvalidBlockFormat("Expected NewBlock event".into()));
  }
  Ok(data)
}

fn get_block_height(payload: &JsonValue) -> Result<U256> {
  let height = get_path(
    &payload,
    &["value", "block", "header", "height"],
    "Missing block height",
  )?;
  let height = height.as_str().ok_or(StargridError::InvalidBlockFormat("Invalid block height".into()))?;
  Ok(U256::from_dec_str(height).map_err(|_| StargridError::InvalidBlockFormat("Failed to parse block height".into()))?)
}

fn get_block_hash(payload: &JsonValue) -> Result<String> {
  let hash = get_path(&payload, &["value", "block", "header", "app_hash"], "Missing block hash")?;
  hash
    .as_str()
    .ok_or(StargridError::InvalidBlockFormat("Invalid block hash".into()))
    .map(|s| s.to_string())
}

fn get_chain_id(payload: &JsonValue) -> Result<String> {
  let chain_id = get_path(&payload, &["value", "block", "header", "chain_id"], "Missing chain ID")?;
  chain_id
    .as_str()
    .ok_or(StargridError::InvalidBlockFormat("Invalid chain ID".into()))
    .map(|s| s.to_string())
}

fn get_block_timestamp(payload: &JsonValue) -> Result<Timestamp> {
  let timestamp = get_path(&payload, &["value", "block", "header", "time"], "Missing block timestamp")?;
  let timestamp = timestamp
    .as_str()
    .ok_or(StargridError::InvalidBlockFormat("Invalid block timestamp".into()))?;
  let timestamp = Timestamp::parse(timestamp);
  timestamp.map_or(
    Err(StargridError::InvalidBlockFormat("Failed to parse block timestamp".into())),
    |timestamp| Ok(timestamp),
  )
}

fn parse_events(events: &JsonValue) -> Result<Vec<Event>> {
  if !events.is_array() {
    return Err(StargridError::InvalidBlockFormat("Expected events to be an array".into()));
  }

  let mut result: Vec<Event> = vec![];
  for event in events.members() {
    result.push(parse_event(event)?);
  }
  Ok(result)
}

fn parse_event(event: &JsonValue) -> Result<Event> {
  if !event.is_object() || !event["type"].is_string() || !event["attributes"].is_array() {
    return Err(StargridError::InvalidBlockFormat("Invalid event format".into()));
  }
  Ok(Event {
    name: event["type"].as_str().unwrap().to_string(),
    attributes: parse_attributes(&event["attributes"])?,
  })
}

fn parse_attributes(attrs: &JsonValue) -> Result<HashMap<String, AttributeValue>> {
  let mut mapping = HashMap::<String, AttributeValue>::new();

  for attr in attrs.members() {
    let key = attr["key"]
      .as_str()
      .ok_or(StargridError::InvalidBlockFormat("Invalid attribute key".into()))?;
    let value = attr["value"].clone();
    let indexed = attr["index"].as_bool().unwrap_or(false);
    mapping.insert(key.to_string(), AttributeValue(indexed, value));
  }

  Ok(mapping)
}

fn try_get_path(value: &JsonValue, path: &[&str]) -> Option<JsonValue> {
  if path.len() == 0 {
    return None;
  }

  let mut curr = value;
  let has_key = |curr: &JsonValue, key: &str| curr.is_object() && curr.has_key(key);
  for key in path.iter().take(path.len() - 1) {
    if !has_key(curr, key) {
      return None;
    }
    curr = &curr[*key];
  }

  let last = path.last().unwrap();
  if has_key(curr, last) {
    Some(curr[*last].clone())
  } else {
    None
  }
}

fn get_path(value: &JsonValue, path: &[&str], errmsg: impl Into<String>) -> Result<JsonValue> {
  try_get_path(value, path).ok_or_else(|| StargridError::InvalidBlockFormat(errmsg.into()))
}

impl Index<&String> for Event {
  type Output = AttributeValue;

  fn index(&self, key: &String) -> &Self::Output {
    &self.attributes[key]
  }
}

impl Index<&str> for Event {
  type Output = AttributeValue;

  fn index(&self, key: &str) -> &Self::Output {
    &self.attributes[&key.to_string()]
  }
}
