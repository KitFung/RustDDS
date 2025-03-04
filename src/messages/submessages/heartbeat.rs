use crate::{
  serialization::SubMessage, serialization::SubmessageBody, structure::guid::EntityId,
  messages::submessages::submessages::SubmessageHeader,
};
use crate::structure::sequence_number::SequenceNumber;
use enumflags2::BitFlags;
use log::error;
use speedy::{Readable, Writable};

use super::{
  submessage::EntitySubmessage, submessage_flag::HEARTBEAT_Flags, submessage_kind::SubmessageKind,
};

/// This Submessage is sent from an RTPS Writer to an RTPS Reader and
/// indicates to the RTPS Reader that a range of sequence numbers
/// is no longer relevant. The set may be a contiguous range of
/// sequence numbers or a specific set of sequence numbers.
#[derive(Debug, PartialEq, Clone, Readable, Writable)]
pub struct Heartbeat {
  /// Identifies the Reader Entity that is being informed of the
  /// availability of a set of sequence numbers.
  ///
  /// Can be set to ENTITYID_UNKNOWN to indicate all readers
  /// for the writer that sent the message.
  pub reader_id: EntityId,

  /// Identifies the Writer Entity to which the range of sequence
  /// numbers applies.
  pub writer_id: EntityId,

  /// Identifies the first (lowest) sequence number that is available in
  /// the Writer.
  pub first_sn: SequenceNumber,

  /// Identifies the last (highest) sequence number that is available in
  /// the Writer.
  pub last_sn: SequenceNumber,

  /// A counter that is incremented each time a new Heartbeat
  /// message is sent.
  ///
  /// Provides the means for a Reader to detect duplicate Heartbeat
  /// messages that can result from the presence of redundant
  /// communication paths.
  pub count: i32,
  // Other present if GroupInfo flag is set
}

impl Heartbeat {
  pub fn create_submessage(self, flags: BitFlags<HEARTBEAT_Flags>) -> Option<SubMessage> {
    let submessage_len = match self.write_to_vec() {
      Ok(bytes) => bytes.len() as u16,
      Err(e) => {
        error!("Reader couldn't write acknack to bytes. Error: {}", e);
        return None;
      }
    };

    Some(SubMessage {
      header: SubmessageHeader {
        kind: SubmessageKind::HEARTBEAT,
        flags: flags.bits(),
        content_length: submessage_len,
      },
      body: SubmessageBody::Entity(EntitySubmessage::Heartbeat(self, flags)),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  serialization_test!( type = Heartbeat,
  {
      heartbeat,
      Heartbeat {
          reader_id: EntityId::ENTITYID_SEDP_BUILTIN_PUBLICATIONS_READER,
          writer_id: EntityId::ENTITYID_SEDP_BUILTIN_PUBLICATIONS_WRITER,
          first_sn: SequenceNumber::from(42),
          last_sn: SequenceNumber::from(7),
          count: 9,
      },
      le = [0x00, 0x00, 0x03, 0xC7,
            0x00, 0x00, 0x03, 0xC2,
            0x00, 0x00, 0x00, 0x00,
            0x2A, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x07, 0x00, 0x00, 0x00,
            0x09, 0x00, 0x00, 0x00],
      be = [0x00, 0x00, 0x03, 0xC7,
            0x00, 0x00, 0x03, 0xC2,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x2A,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x07,
            0x00, 0x00, 0x00, 0x09]
  });
}
