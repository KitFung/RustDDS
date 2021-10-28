use speedy::{Context, Readable, Reader, Writable, Writer};
use serde::{Serialize, Deserialize};
use crate::structure::parameter_id::ParameterId;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
pub struct VendorId {
  pub vendor_id: [u8; 2],
}

impl VendorId {
  pub const VENDOR_UNKNOWN: VendorId = VendorId {
    vendor_id: [0x00; 2],
  };

  /// assigned by OMG DDS SIG on 2020-11-21
  pub const ATOSTEK: VendorId = VendorId {
    vendor_id: [0x01, 0x12],
  };

  pub const THIS_IMPLEMENTATION: VendorId = VendorId::ATOSTEK;
}

impl Default for VendorId {
  fn default() -> Self {
    VendorId::VENDOR_UNKNOWN
  }
}

#[derive(Serialize, Deserialize)]
pub struct VendorIdData {
  parameter_id: ParameterId,
  parameter_length: u16,
  vendor_id: VendorId,
}

impl VendorIdData {
  pub fn from(vendor_id: VendorId) -> VendorIdData {
    VendorIdData {
      parameter_id: ParameterId::PID_VENDOR_ID,
      parameter_length: 4,
      vendor_id,
    }
  }
}

impl<'a, C: Context> Readable<'a, C> for VendorId {
  #[inline]
  fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
    let mut vendor_id = VendorId::default();
    for i in 0..vendor_id.vendor_id.len() {
      vendor_id.vendor_id[i] = reader.read_u8()?;
    }
    Ok(vendor_id)
  }

  #[inline]
  fn minimum_bytes_needed() -> usize {
    std::mem::size_of::<Self>()
  }
}

impl<C: Context> Writable<C> for VendorId {
  #[inline]
  fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
    for elem in &self.vendor_id {
      writer.write_u8(*elem)?
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use speedy::Endianness;

  #[test]
  fn minimum_bytes_needed() {
    assert_eq!(
      2,
      <VendorId as Readable<Endianness>>::minimum_bytes_needed()
    );
  }

  serialization_test!( type = VendorId,
  {
      vendor_unknown,
      VendorId::VENDOR_UNKNOWN,
      le = [0x00, 0x00],
      be = [0x00, 0x00]
  });
}
