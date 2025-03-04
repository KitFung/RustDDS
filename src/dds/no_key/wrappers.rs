use std::ops::Deref;

use bytes::Bytes;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};

use crate::{
  dds::traits::key::Keyed, dds::traits::serde_adapters::*,
  messages::submessages::submessages::RepresentationIdentifier,
  serialization::error::Result,
};

// This wrapper is used to convert NO_KEY types to WITH_KEY
// * inside the wrapper there is a NO_KEY type
// * the wrapper is good for WITH_KEY
// The wrapper introduces a dummy key of type (), which of course has an always known value ()
pub(crate) struct NoKeyWrapper<D> {
  pub(crate) d: D,
}

impl<D> NoKeyWrapper<D> {
  pub fn unwrap(self) -> D {
    self.d
  }
}

impl<D> From<D> for NoKeyWrapper<D> {
  fn from(d: D) -> Self {
    NoKeyWrapper { d }
  }
}

// implement Deref so that &NoKeyWrapper<D> is coercible to &D
impl<D> Deref for NoKeyWrapper<D> {
  type Target = D;
  fn deref(&self) -> &Self::Target {
    &self.d
  }
}

impl<D> Keyed for NoKeyWrapper<D> {
  type K = ();
  fn get_key(&self) {
    
  }
}

impl<'de, D> Deserialize<'de> for NoKeyWrapper<D>
where
  D: Deserialize<'de>,
{
  fn deserialize<R>(deserializer: R) -> std::result::Result<NoKeyWrapper<D>, R::Error>
  where
    R: Deserializer<'de>,
  {
    D::deserialize(deserializer).map(|d| NoKeyWrapper::<D> { d })
  }
}

impl<D> Serialize for NoKeyWrapper<D>
where
  D: Serialize,
{
  fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    self.d.serialize(serializer)
  }
}

// wrapper for SerializerAdapter
// * inside is NO_KEY
// * outside of wrapper is WITH_KEY
pub struct SAWrapper<SA> {
  no_key: SA,
}

// have to implement base trait first, just trivial passthrough
impl<D, SA> no_key::SerializerAdapter<NoKeyWrapper<D>> for SAWrapper<SA>
where
  D: Serialize, 
  SA: no_key::SerializerAdapter<D>,
{
  fn output_encoding() -> RepresentationIdentifier {
    SA::output_encoding()
  }

  fn to_Bytes(value: &NoKeyWrapper<D>) -> Result<Bytes> {
    SA::to_Bytes(&value.d)
  }
}

// This is the point of wrapping. Implement dummy key serialization
// Of course, this is never supposed to be actually called.
impl<D, SA> with_key::SerializerAdapter<NoKeyWrapper<D>> for SAWrapper<SA>
where
  D: Serialize, 
  SA: no_key::SerializerAdapter<D>,
{
  fn key_to_Bytes(_value: &() ) -> Result<Bytes> {
    Ok(Bytes::new())
  }
}

// wrapper for DeerializerAdapter
// * inside is NO_KEY
// * outside of wrapper is WITH_KEY
pub struct DAWrapper<DA> {
  no_key: DA,
}

// first, implement no_key DA
impl<D, DA> no_key::DeserializerAdapter<NoKeyWrapper<D>> for DAWrapper<DA>
where 
  D: DeserializeOwned,
  DA: no_key::DeserializerAdapter<D>,
{
  fn supported_encodings() -> &'static [RepresentationIdentifier] {
    DA::supported_encodings()
  }

  fn from_bytes(input_bytes: &[u8], encoding: RepresentationIdentifier) 
    -> Result<NoKeyWrapper<D>> 
  {
    DA::from_bytes(input_bytes, encoding).map(|d| NoKeyWrapper::<D> { d })
  }
}

// then, implement with_key DA
impl<D, DA> with_key::DeserializerAdapter<NoKeyWrapper<D>> for DAWrapper<DA>
where 
  D: DeserializeOwned,
  DA: no_key::DeserializerAdapter<D>,
{
  fn key_from_bytes(_input_bytes: &[u8], _encoding: RepresentationIdentifier) 
    -> Result< <NoKeyWrapper<D> as Keyed>::K > 
  {
    // also unreachable!() should work here, as this is not supposed to be used
    Ok( () ) 
  }
}


