use std::collections::HashMap;

use log::*;
use primitive_types::U256;
use serde::{de::Visitor, ser::SerializeMap};

use crate::data::AttributeValue;

pub trait LogError {
  fn log_error(self);
}

impl<E> LogError for std::result::Result<(), E>
where E: std::fmt::Debug
{
  fn log_error(self) {
    match self {
      Err(err) => {
        error!("Error: {:?}", err);
      }
      _ => (),
    }
  }
}

pub fn serialize_u256<S>(x: &U256, s: S) -> std::result::Result<S::Ok, S::Error>
where S: serde::Serializer
{
  s.serialize_str(x.to_string().as_str())
}

pub fn deserialize_u256<'de, D>(d: D) -> std::result::Result<U256, D::Error>
where D: serde::Deserializer<'de>
{
  struct U256Visitor;
  impl<'de> Visitor<'de> for U256Visitor {
    type Value = U256;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
      formatter.write_str("a string containing a U256")
    }

    fn visit_str<E>(self, s: &str) -> std::result::Result<Self::Value, E>
    where E: serde::de::Error
    {
      U256::from_dec_str(s).map_err(|err| E::custom(format!("{}", err)))
    }
  }

  d.deserialize_str(U256Visitor)
}

pub fn serialize_attrs<S>(x: &HashMap<String, AttributeValue>, s: S) -> std::result::Result<S::Ok, S::Error>
where S: serde::Serializer
{
  let mut map = s.serialize_map(Some(x.len()))?;
  for (k, v) in x {
    map.serialize_entry(k, v)?;
  }
  map.end()
}

pub fn deserialize_attrs<'de, D>(d: D) -> std::result::Result<HashMap<String, AttributeValue>, D::Error>
where D: serde::Deserializer<'de>
{
  struct AttrsVisitor;
  impl<'de> Visitor<'de> for AttrsVisitor {
    type Value = HashMap<String, AttributeValue>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
      formatter.write_str("a map containing attributes")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where A: serde::de::MapAccess<'de>
    {
      let mut attrs = HashMap::new();
      while let Some((k, v)) = map.next_entry()? {
        attrs.insert(k, v);
      }
      Ok(attrs)
    }
  }

  d.deserialize_map(AttrsVisitor)
}
