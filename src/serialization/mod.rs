pub(crate) mod builtin_data_deserializer;
pub(crate) mod builtin_data_serializer;
pub(crate) mod cdr_deserializer;
pub(crate) mod cdr_serializer;
pub(crate) mod error;
pub(crate) mod pl_cdr_deserializer;
pub(crate) mod visitors;

pub(crate) mod message;
pub(crate) mod submessage;

// crate exports
pub(crate) use message::*;
pub(crate) use submessage::*;

// public exports
pub use cdr_serializer::{CDRSerializerAdapter};
pub use cdr_deserializer::{CDRDeserializerAdapter};
pub use crate::dds::traits::serde_adapters::{with_key, no_key};

pub use byteorder::{LittleEndian, BigEndian};
