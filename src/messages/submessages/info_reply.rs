use crate::structure::locator::LocatorList;
use speedy::{Readable, Writable};

/// This message is sent from an RTPS Reader to an RTPS Writer.
/// It contains explicit information on where to send a reply
/// to the Submessages that follow it within the same message.
#[derive(Debug, PartialEq, Clone, Readable, Writable)]
pub struct InfoReply {
  /// Indicates an alternative set of unicast addresses that
  /// the Writershould use to reach the Readers when
  /// replying to the Submessages that follow.
  pub unicast_locator_list: LocatorList,

  /// Indicates an alternative set of multicast addresses that the Writer
  /// should use to reach the Readers when replying to the Submessages that
  /// follow.
  ///
  /// Only present when the MulticastFlag is set.
  pub multicast_locator_list: Option<LocatorList>,
}
