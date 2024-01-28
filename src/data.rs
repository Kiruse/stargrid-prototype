use std::{collections::HashMap, ops::Index};

use base64::Engine;
use iso8601_timestamp::Timestamp;
use primitive_types::U256;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Result, StargridError};

pub const SUB_ID_BLOCK: u64 = 1;
pub const SUB_ID_TX: u64 = 2;

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
pub struct Block {
  pub raw: String,
  #[serde(serialize_with = "crate::util::serialize_u256", deserialize_with = "crate::util::deserialize_u256")]
  pub height: U256,
  pub hash: String,
  pub chain_id: String,
  pub time: Timestamp,
  pub events: Vec<Event>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
pub struct Tx {
  pub raw: String,
  pub error: Option<TxError>,
  #[serde(serialize_with = "crate::util::serialize_u256", deserialize_with = "crate::util::deserialize_u256")]
  pub height: U256,
  pub tx: String,
  pub txhash: String,
  pub events: Vec<Event>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
pub struct Event {
  pub name: String,
  #[serde(serialize_with = "crate::util::serialize_attrs", deserialize_with = "crate::util::deserialize_attrs")]
  pub attributes: HashMap<String, AttributeValue>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
pub struct AttributeValue {
  pub indexed: bool,
  pub value: Value,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq, Clone)]
pub struct TxError {
  code: u64,
  codespace: String,
  message: String,
}

pub fn validate_jsonrpc(data: &Value) -> Result<()> {
  if data["jsonrpc"] != "2.0" {
    Err(StargridError::unexpected_value("2.0", &data["jsonrpc"]))
  } else {
    Ok(())
  }
}

pub fn parse_block(data: &Value) -> Result<Block> {
  validate_jsonrpc(data)?;
  if data["id"].as_u64() != Some(SUB_ID_BLOCK) {
    return Err(StargridError::unexpected_value(Some(SUB_ID_BLOCK), &data["id"]));
  }

  let payload = block_payload(&data)?;

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
    raw: payload.to_string(),
    height: get_block_height(&payload)?,
    hash: get_block_hash(&payload)?,
    chain_id: get_chain_id(&payload)?,
    time: get_block_timestamp(&payload)?,
    events,
  })
}

pub fn parse_tx(data: & Value) -> Result<Tx> {
  validate_jsonrpc(data)?;
  if data["id"].as_u64() != Some(SUB_ID_TX) {
    return Err(StargridError::unexpected_value(Some(SUB_ID_TX), data["id"].as_u64()));
  }

  let payload = tx_payload(&data)?;
  let tx = payload["tx"].as_str().ok_or(StargridError::InvalidFormat("Missing tx bytes".into()))?;
  let txhash = get_tx_hash(tx)?;
  // TODO: compute tx hash, steal from feather.js

  let events = parse_events(&get_path(&payload, &["result", "events"], "Missing events")?)?;

  Ok(Tx {
    raw: payload.to_string(),
    error: get_tx_error(&payload)?,
    height: get_tx_height(&payload)?,
    tx: tx.to_string(),
    txhash,
    events,
  })
}

fn block_payload(data: &Value) -> Result<Value> {
  let data = get_path(&data, &["result", "data"], "Expected .result.data")?;
  if data["type"] != "tendermint/event/NewBlock" {
    return Err(StargridError::InvalidFormat("Expected NewBlock event".into()));
  }
  Ok(data)
}

fn tx_payload(data: &Value) -> Result<Value> {
  let data = get_path(&data, &["result", "data"], "Expected .result.data")?;
  if data["type"] != "tendermint/event/Tx" {
    return Err(StargridError::InvalidFormat("Expected Tx event".into()));
  }
  Ok(get_path(&data, &["value", "TxResult"], "Expected .result.data.value.TxResult")?)
}

fn get_tx_hash(bytes: &str) -> Result<String> {
  let bytes = base64::prelude::BASE64_STANDARD.decode(bytes)
    .map_err(|err| StargridError::ParseError("Failed to decode tx bytes".into()))?;
  Ok(sha256::digest(bytes).to_uppercase())
}

fn get_block_height(payload: &Value) -> Result<U256> {
  let height = get_path(
    &payload,
    &["value", "block", "header", "height"],
    "Missing block height",
  )?;
  let height = height.as_str().ok_or(StargridError::InvalidFormat("Invalid block height".into()))?;
  Ok(U256::from_dec_str(height).map_err(|_| StargridError::ParseError("Failed to parse block height".into()))?)
}

fn get_tx_height(payload: &Value) -> Result<U256> {
  Ok(
    U256::from_dec_str(
      payload["height"].as_str().ok_or(StargridError::InvalidFormat("Invalid tx height".into()))?
    )
    .map_err(|err| StargridError::ParseError(format!("Failed to parse tx height: {}", err)))?
  )
}

fn get_block_hash(payload: &Value) -> Result<String> {
  let hash = get_path(&payload, &["value", "block", "header", "app_hash"], "Missing block hash")?;
  hash
    .as_str()
    .ok_or(StargridError::InvalidFormat("Invalid block hash".into()))
    .map(|s| s.to_string())
}

fn get_chain_id(payload: &Value) -> Result<String> {
  let chain_id = get_path(&payload, &["value", "block", "header", "chain_id"], "Missing chain ID")?;
  chain_id
    .as_str()
    .ok_or(StargridError::InvalidFormat("Invalid chain ID".into()))
    .map(|s| s.to_string())
}

fn get_block_timestamp(payload: &Value) -> Result<Timestamp> {
  let timestamp = get_path(&payload, &["value", "block", "header", "time"], "Missing block timestamp")?;
  let timestamp = timestamp
    .as_str()
    .ok_or(StargridError::InvalidFormat("Invalid block timestamp".into()))?;
  let timestamp = Timestamp::parse(timestamp);
  timestamp.map_or(
    Err(StargridError::ParseError("Failed to parse block timestamp".into())),
    |timestamp| Ok(timestamp),
  )
}

fn get_tx_error(payload: &Value) -> Result<Option<TxError>> {
  let code = try_get_path(&payload, &["result", "code"]);
  if code.is_null() {
    return Ok(None);
  }

  let code = code.as_u64().ok_or(StargridError::InvalidFormat("Invalid tx error code".into()))?;
  let codespace = try_get_path(&payload, &["result", "codespace"]);
  let codespace = codespace.as_str().ok_or(StargridError::InvalidFormat("Invalid tx error codespace".into()))?;
  let message = try_get_path(&payload, &["result", "log"]);
  let message = message.as_str().ok_or(StargridError::InvalidFormat("Invalid tx error message".into()))?;

  Ok(Some(TxError {
    code,
    codespace: codespace.to_string(),
    message: message.to_string(),
  }))
}

fn parse_events(events: &Value) -> Result<Vec<Event>> {
  let events = events.as_array().ok_or(StargridError::InvalidFormat("Invalid events".into()))?;

  let mut result: Vec<Event> = vec![];
  for event in events {
    result.push(parse_event(event)?);
  }
  Ok(result)
}

fn parse_event(event: &Value) -> Result<Event> {
  if !event["type"].is_string() || !event["attributes"].is_array() {
    return Err(StargridError::InvalidFormat("Invalid event format".into()));
  }
  Ok(Event {
    name: event["type"].as_str().unwrap().to_string(),
    attributes: parse_attributes(&event["attributes"])?,
  })
}

fn parse_attributes(attrs: &Value) -> Result<HashMap<String, AttributeValue>> {
  let mut mapping = HashMap::<String, AttributeValue>::new();

  let attrs = attrs.as_array().ok_or(StargridError::InvalidFormat("Invalid attributes".into()))?;

  for elem in attrs {
    let key = elem["key"].as_str().ok_or(StargridError::InvalidFormat("Invalid attribute key".into()))?;
    let value = elem["value"].clone();
    let indexed = elem["index"].as_bool().unwrap_or(false);
    mapping.insert(key.to_string(), AttributeValue { indexed, value });
  }

  Ok(mapping)
}

fn try_get_path(value: &Value, path: &[&str]) -> Value {
  path
    .iter()
    .fold(value, |acc, key| &acc[key])
    .clone()
}

fn get_path(value: &Value, path: &[&str], errmsg: impl Into<String>) -> Result<Value> {
  let val = try_get_path(value, path);
  if val.is_null() {
    Err(StargridError::InvalidFormat(errmsg.into()))
  } else {
    Ok(val)
  }
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
