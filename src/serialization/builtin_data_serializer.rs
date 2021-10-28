// use crate::discovery::data_types::topic_data::PublicationBuiltinTopicDataKey;
// use crate::discovery::data_types::topic_data::SubscriptionBuiltinTopicDataKey;
use crate::discovery::data_types::topic_data::DiscoveredReaderDataKey;
use crate::discovery::data_types::topic_data::DiscoveredWriterDataKey;
use crate::{
  structure::{
    locator::{LocatorList, LocatorData},
    parameter_id::ParameterId,
    guid::{GUID, GUIDData},
    builtin_endpoint::{
      BuiltinEndpointQosData, BuiltinEndpointSetData, BuiltinEndpointSet, BuiltinEndpointQos,
    },
    duration::{Duration, DurationData},
    endpoint::ReliabilityKind,
  },
  discovery::{
    content_filter_property::{ContentFilterPropertyData, ContentFilterProperty},
    data_types::{
      topic_data::{
        SubscriptionBuiltinTopicData, ReaderProxy, DiscoveredReaderData, WriterProxy,
        PublicationBuiltinTopicData, DiscoveredWriterData, TopicBuiltinTopicData,
      },
      spdp_participant_data::SpdpDiscoveredParticipantData,
      spdp_participant_data::SpdpDiscoveredParticipantDataKey,
    },
  },
  messages::{
    vendor_id::{VendorId, VendorIdData},
    protocol_version::{ProtocolVersion, ProtocolVersionData},
  },
  dds::qos::policy::{
    Deadline, Durability, LatencyBudget, Liveliness, Reliability, Ownership, DestinationOrder,
    TimeBasedFilter, Presentation, Lifespan, History, ResourceLimits, QosData,
  },
};
use serde::{Serialize, Serializer, ser::SerializeStruct, Deserialize};
use std::time::Duration as StdDuration;

#[derive(Serialize, Deserialize)]
struct StringData {
  parameter_id: ParameterId,
  parameter_length: u16,
  string_data: String,
}

impl StringData {
  pub fn new(parameter_id: ParameterId, string_data: String) -> StringData {
    let parameter_length = string_data.len() as u16;
    let parameter_length = parameter_length + (4 - parameter_length % 4) + 4;
    StringData {
      parameter_id,
      parameter_length,
      string_data,
    }
  }
}

#[derive(Serialize, Deserialize)]
struct U32Data {
  parameter_id: ParameterId,
  parameter_length: u16,
  data: u32,
}

impl U32Data {
  pub fn new(parameter_id: ParameterId, data: u32) -> U32Data {
    U32Data {
      parameter_id,
      parameter_length: 4,
      data,
    }
  }
}

#[derive(Serialize, Deserialize)]
struct I32Data {
  parameter_id: ParameterId,
  parameter_length: u16,
  data: i32,
}

impl I32Data {
  pub fn new(parameter_id: ParameterId, data: i32) -> I32Data {
    I32Data {
      parameter_id,
      parameter_length: 4,
      data,
    }
  }
}

#[derive(Serialize, Deserialize)]
struct ExpectsInlineQos {
  parameter_id: ParameterId,
  parameter_length: u16,
  expects_inline_qos: bool,
}

#[derive(Serialize, Deserialize)]
struct ManualLivelinessCount {
  parameter_id: ParameterId,
  parameter_length: u16,
  manual_liveliness_count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct EntityName {
  parameter_id: ParameterId,
  parameter_length: u16,
  entity_name: String,
}

pub struct BuiltinDataSerializerKey {
  pub participant_guid: GUID,
}

impl BuiltinDataSerializerKey {
  pub fn from_data(participant_data: SpdpDiscoveredParticipantDataKey) -> BuiltinDataSerializerKey {
    BuiltinDataSerializerKey {
      participant_guid: participant_data.0,
    }
  }

  pub fn serialize<S: Serializer>(
    self,
    serializer: S,
    add_sentinel: bool,
  ) -> Result<S::Ok, S::Error> {
    let mut s = serializer
      .serialize_struct("SPDPParticipantData_Key", 1)
      .unwrap();

    self.add_participant_guid::<S>(&mut s);

    if add_sentinel {
      s.serialize_field("sentinel", &1_u32).unwrap();
    }

    s.end()
  }

  fn add_participant_guid<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    s.serialize_field(
      "participant_guid",
      &GUIDData::from(self.participant_guid, ParameterId::PID_PARTICIPANT_GUID),
    )
    .unwrap();
  }
}

pub struct BuiltinDataSerializer<'a> {
  // Participant Data
  pub protocol_version: Option<ProtocolVersion>,
  pub vendor_id: Option<VendorId>,
  pub expects_inline_qos: Option<bool>,
  pub participant_guid: Option<GUID>,
  pub metatraffic_unicast_locators: Option<&'a LocatorList>,
  pub metatraffic_multicast_locators: Option<&'a LocatorList>,
  pub default_unicast_locators: Option<&'a LocatorList>,
  pub default_multicast_locators: Option<&'a LocatorList>,
  pub available_builtin_endpoints: Option<BuiltinEndpointSet>,
  pub lease_duration: Option<Duration>,
  pub manual_liveliness_count: Option<i32>,
  pub builtin_endpoint_qos: Option<BuiltinEndpointQos>,
  pub entity_name: Option<&'a String>,

  pub endpoint_guid: Option<GUID>,

  // Reader Proxy
  pub unicast_locator_list: Option<&'a LocatorList>,
  pub multicast_locator_list: Option<&'a LocatorList>,

  // Writer Proxy
  pub data_max_size_serialized: Option<u32>,

  // topic data
  pub topic_name: Option<&'a String>,
  pub type_name: Option<&'a String>,
  pub durability: Option<Durability>,
  pub deadline: Option<Deadline>,
  pub latency_budget: Option<LatencyBudget>,
  pub liveliness: Option<Liveliness>,
  pub reliability: Option<Reliability>,
  pub ownership: Option<Ownership>,
  pub destination_order: Option<DestinationOrder>,
  pub time_based_filter: Option<TimeBasedFilter>,
  pub presentation: Option<Presentation>,
  pub lifespan: Option<Lifespan>,
  pub history: Option<History>,
  pub resource_limits: Option<ResourceLimits>,

  pub content_filter_property: Option<&'a ContentFilterProperty>,
}

impl<'a> BuiltinDataSerializer<'a> {
  pub fn merge(mut self, other: BuiltinDataSerializer<'a>) -> BuiltinDataSerializer<'a> {
    self.protocol_version = match other.protocol_version {
      Some(v) => Some(v),
      None => self.protocol_version,
    };
    self.vendor_id = match other.vendor_id {
      Some(v) => Some(v),
      None => self.vendor_id,
    };
    self.expects_inline_qos = match other.expects_inline_qos {
      Some(v) => Some(v),
      None => self.expects_inline_qos,
    };
    self.participant_guid = match other.participant_guid {
      Some(v) => Some(v),
      None => self.participant_guid,
    };
    self.metatraffic_unicast_locators = match other.metatraffic_unicast_locators {
      Some(v) => Some(v),
      None => self.metatraffic_unicast_locators,
    };
    self.metatraffic_multicast_locators = match other.metatraffic_multicast_locators {
      Some(v) => Some(v),
      None => self.metatraffic_multicast_locators,
    };
    self.default_unicast_locators = match other.default_unicast_locators {
      Some(v) => Some(v),
      None => self.default_unicast_locators,
    };
    self.default_multicast_locators = match other.default_multicast_locators {
      Some(v) => Some(v),
      None => self.default_multicast_locators,
    };
    self.available_builtin_endpoints = match other.available_builtin_endpoints {
      Some(v) => Some(v),
      None => self.available_builtin_endpoints,
    };
    self.lease_duration = match other.lease_duration {
      Some(v) => Some(v),
      None => self.lease_duration,
    };
    self.manual_liveliness_count = match other.manual_liveliness_count {
      Some(v) => Some(v),
      None => self.manual_liveliness_count,
    };
    self.builtin_endpoint_qos = match other.builtin_endpoint_qos {
      Some(v) => Some(v),
      None => self.builtin_endpoint_qos,
    };
    self.entity_name = match other.entity_name {
      Some(v) => Some(v),
      None => self.entity_name,
    };
    self.endpoint_guid = match other.endpoint_guid {
      Some(v) => Some(v),
      None => self.endpoint_guid,
    };
    self.unicast_locator_list = match other.unicast_locator_list {
      Some(v) => Some(v),
      None => self.unicast_locator_list,
    };
    self.multicast_locator_list = match other.multicast_locator_list {
      Some(v) => Some(v),
      None => self.multicast_locator_list,
    };
    self.data_max_size_serialized = match other.data_max_size_serialized {
      Some(v) => Some(v),
      None => self.data_max_size_serialized,
    };
    self.topic_name = match other.topic_name {
      Some(v) => Some(v),
      None => self.topic_name,
    };
    self.type_name = match other.type_name {
      Some(v) => Some(v),
      None => self.type_name,
    };
    self.durability = match other.durability {
      Some(v) => Some(v),
      None => self.durability,
    };
    self.deadline = match other.deadline {
      Some(v) => Some(v),
      None => self.deadline,
    };
    self.latency_budget = match other.latency_budget {
      Some(v) => Some(v),
      None => self.latency_budget,
    };
    self.liveliness = match other.liveliness {
      Some(v) => Some(v),
      None => self.liveliness,
    };
    self.reliability = match other.reliability {
      Some(v) => Some(v),
      None => self.reliability,
    };
    self.ownership = match other.ownership {
      Some(v) => Some(v),
      None => self.ownership,
    };
    self.destination_order = match other.destination_order {
      Some(v) => Some(v),
      None => self.destination_order,
    };
    self.time_based_filter = match other.time_based_filter {
      Some(v) => Some(v),
      None => self.time_based_filter,
    };
    self.presentation = match other.presentation {
      Some(v) => Some(v),
      None => self.presentation,
    };
    self.lifespan = match other.lifespan {
      Some(v) => Some(v),
      None => self.lifespan,
    };
    self.history = match other.history {
      Some(v) => Some(v),
      None => self.history,
    };
    self.resource_limits = match other.resource_limits {
      Some(v) => Some(v),
      None => self.resource_limits,
    };
    self.content_filter_property = match other.content_filter_property {
      Some(v) => Some(v),
      None => self.content_filter_property,
    };

    self
  }

  pub fn from_participant_data(
    participant_data: &'a SpdpDiscoveredParticipantData,
  ) -> BuiltinDataSerializer<'a> {
    BuiltinDataSerializer {
      protocol_version: Some(participant_data.protocol_version),
      vendor_id: Some(participant_data.vendor_id),
      expects_inline_qos: Some(participant_data.expects_inline_qos),
      participant_guid: Some(participant_data.participant_guid),
      metatraffic_unicast_locators: Some(&participant_data.metatraffic_unicast_locators),
      metatraffic_multicast_locators: Some(&participant_data.metatraffic_multicast_locators),
      default_unicast_locators: Some(&participant_data.default_unicast_locators),
      default_multicast_locators: Some(&participant_data.default_multicast_locators),
      available_builtin_endpoints: Some(participant_data.available_builtin_endpoints),
      lease_duration: participant_data.lease_duration,
      manual_liveliness_count: Some(participant_data.manual_liveliness_count),
      builtin_endpoint_qos: participant_data.builtin_endpoint_qos,
      entity_name: participant_data.entity_name.as_ref(),
      endpoint_guid: None,
      unicast_locator_list: None,
      multicast_locator_list: None,
      data_max_size_serialized: None,
      topic_name: None,
      type_name: None,
      durability: None,
      deadline: None,
      latency_budget: None,
      liveliness: None,
      reliability: None,
      ownership: None,
      destination_order: None,
      time_based_filter: None,
      presentation: None,
      lifespan: None,
      history: None,
      resource_limits: None,
      content_filter_property: None,
    }
  }

  pub fn from_reader_proxy(reader_proxy: &'a ReaderProxy) -> BuiltinDataSerializer<'a> {
    BuiltinDataSerializer {
      protocol_version: None,
      vendor_id: None,
      expects_inline_qos: Some(reader_proxy.expects_inline_qos),
      participant_guid: None,
      metatraffic_unicast_locators: None,
      metatraffic_multicast_locators: None,
      default_unicast_locators: None,
      default_multicast_locators: None,
      available_builtin_endpoints: None,
      lease_duration: None,
      manual_liveliness_count: None,
      builtin_endpoint_qos: None,
      entity_name: None,
      endpoint_guid: Some(reader_proxy.remote_reader_guid),
      unicast_locator_list: Some(&reader_proxy.unicast_locator_list),
      multicast_locator_list: Some(&reader_proxy.multicast_locator_list),
      data_max_size_serialized: None,
      topic_name: None,
      type_name: None,
      durability: None,
      deadline: None,
      latency_budget: None,
      liveliness: None,
      reliability: None,
      ownership: None,
      destination_order: None,
      time_based_filter: None,
      presentation: None,
      lifespan: None,
      history: None,
      resource_limits: None,
      content_filter_property: None,
    }
  }

  pub fn from_writer_proxy(writer_proxy: &'a WriterProxy) -> BuiltinDataSerializer<'a> {
    BuiltinDataSerializer {
      protocol_version: None,
      vendor_id: None,
      expects_inline_qos: None,
      participant_guid: None,
      metatraffic_unicast_locators: None,
      metatraffic_multicast_locators: None,
      default_unicast_locators: None,
      default_multicast_locators: None,
      available_builtin_endpoints: None,
      lease_duration: None,
      manual_liveliness_count: None,
      builtin_endpoint_qos: None,
      entity_name: None,
      endpoint_guid: Some(writer_proxy.remote_writer_guid),
      unicast_locator_list: Some(&writer_proxy.unicast_locator_list),
      multicast_locator_list: Some(&writer_proxy.multicast_locator_list),
      data_max_size_serialized: writer_proxy.data_max_size_serialized,
      topic_name: None,
      type_name: None,
      durability: None,
      deadline: None,
      latency_budget: None,
      liveliness: None,
      reliability: None,
      ownership: None,
      destination_order: None,
      time_based_filter: None,
      presentation: None,
      lifespan: None,
      history: None,
      resource_limits: None,
      content_filter_property: None,
    }
  }

  pub fn from_subscription_topic_data(
    subscription_topic_data: &'a SubscriptionBuiltinTopicData,
  ) -> BuiltinDataSerializer<'a> {
    BuiltinDataSerializer {
      protocol_version: None,
      vendor_id: None,
      expects_inline_qos: None,
      participant_guid: *subscription_topic_data.participant_key(),
      metatraffic_unicast_locators: None,
      metatraffic_multicast_locators: None,
      default_unicast_locators: None,
      default_multicast_locators: None,
      available_builtin_endpoints: None,
      lease_duration: None,
      manual_liveliness_count: None,
      builtin_endpoint_qos: None,
      entity_name: None,
      endpoint_guid: Some(subscription_topic_data.key()),
      unicast_locator_list: None,
      multicast_locator_list: None,
      data_max_size_serialized: None,
      topic_name: Some(subscription_topic_data.topic_name()),
      type_name: Some(subscription_topic_data.type_name()),
      durability: *subscription_topic_data.durability(),
      deadline: *subscription_topic_data.deadline(),
      latency_budget: *subscription_topic_data.latency_budget(),
      liveliness: *subscription_topic_data.liveliness(),
      reliability: *subscription_topic_data.reliability(),
      ownership: *subscription_topic_data.ownership(),
      destination_order: *subscription_topic_data.destination_order(),
      time_based_filter: *subscription_topic_data.time_based_filter(),
      presentation: *subscription_topic_data.presentation(),
      lifespan: *subscription_topic_data.lifespan(),
      history: None,
      resource_limits: None,
      content_filter_property: None,
    }
  }

  pub fn from_publication_topic_data(
    publication_topic_data: &'a PublicationBuiltinTopicData,
  ) -> BuiltinDataSerializer {
    BuiltinDataSerializer {
      protocol_version: None,
      vendor_id: None,
      expects_inline_qos: None,
      participant_guid: publication_topic_data.participant_key,
      metatraffic_unicast_locators: None,
      metatraffic_multicast_locators: None,
      default_unicast_locators: None,
      default_multicast_locators: None,
      available_builtin_endpoints: None,
      lease_duration: None,
      manual_liveliness_count: None,
      builtin_endpoint_qos: None,
      entity_name: None,
      endpoint_guid: Some(publication_topic_data.key),
      unicast_locator_list: None,
      multicast_locator_list: None,
      data_max_size_serialized: None,
      topic_name: Some(&publication_topic_data.topic_name),
      type_name: Some(&publication_topic_data.type_name),
      durability: publication_topic_data.durability,
      deadline: publication_topic_data.deadline,
      latency_budget: publication_topic_data.latency_budget,
      liveliness: publication_topic_data.liveliness,
      reliability: publication_topic_data.reliability,
      ownership: publication_topic_data.ownership,
      destination_order: publication_topic_data.destination_order,
      time_based_filter: publication_topic_data.time_based_filter,
      presentation: publication_topic_data.presentation,
      lifespan: publication_topic_data.lifespan,
      history: None,
      resource_limits: None,
      content_filter_property: None,
    }
  }

  pub fn from_endpoint_guid(guid: &'a GUID) -> BuiltinDataSerializer<'a> {
    BuiltinDataSerializer {
      protocol_version: None,
      vendor_id: None,
      expects_inline_qos: None,
      participant_guid: None,
      metatraffic_unicast_locators: None,
      metatraffic_multicast_locators: None,
      default_unicast_locators: None,
      default_multicast_locators: None,
      available_builtin_endpoints: None,
      lease_duration: None,
      manual_liveliness_count: None,
      builtin_endpoint_qos: None,
      entity_name: None,
      endpoint_guid: Some(*guid),
      unicast_locator_list: None,
      multicast_locator_list: None,
      data_max_size_serialized: None,
      topic_name: None,
      type_name: None,
      durability: None,
      deadline: None,
      latency_budget: None,
      liveliness: None,
      reliability: None,
      ownership: None,
      destination_order: None,
      time_based_filter: None,
      presentation: None,
      lifespan: None,
      history: None,
      resource_limits: None,
      content_filter_property: None,
    }
  }

  pub fn from_topic_data(topic_data: &'a TopicBuiltinTopicData) -> BuiltinDataSerializer<'a> {
    BuiltinDataSerializer {
      protocol_version: None,
      vendor_id: None,
      expects_inline_qos: None,
      participant_guid: None,
      metatraffic_unicast_locators: None,
      metatraffic_multicast_locators: None,
      default_unicast_locators: None,
      default_multicast_locators: None,
      available_builtin_endpoints: None,
      lease_duration: None,
      manual_liveliness_count: None,
      builtin_endpoint_qos: None,
      entity_name: None,
      endpoint_guid: topic_data.key,
      unicast_locator_list: None,
      multicast_locator_list: None,
      data_max_size_serialized: None,
      topic_name: Some(&topic_data.name),
      type_name: Some(&topic_data.type_name),
      durability: topic_data.durability,
      deadline: topic_data.deadline,
      latency_budget: topic_data.latency_budget,
      liveliness: topic_data.liveliness,
      reliability: topic_data.reliability,
      ownership: topic_data.ownership,
      destination_order: topic_data.destination_order,
      time_based_filter: None,
      presentation: topic_data.presentation,
      lifespan: topic_data.lifespan,
      history: topic_data.history,
      resource_limits: topic_data.resource_limits,
      content_filter_property: None,
    }
  }

  pub fn from_discovered_reader_data(
    discovered_reader_data: &'a DiscoveredReaderData,
  ) -> BuiltinDataSerializer<'a> {
    let bds_rp = BuiltinDataSerializer::from_reader_proxy(&discovered_reader_data.reader_proxy);
    let bds_std = BuiltinDataSerializer::from_subscription_topic_data(
      &discovered_reader_data.subscription_topic_data,
    );
    let mut bds_merged = bds_rp.merge(bds_std);
    bds_merged.content_filter_property = discovered_reader_data.content_filter.as_ref();
    bds_merged
  }

  pub fn from_discovered_writer_data(
    discovered_writer_data: &'a DiscoveredWriterData,
  ) -> BuiltinDataSerializer<'a> {
    let bds_wp = BuiltinDataSerializer::from_writer_proxy(&discovered_writer_data.writer_proxy);
    let bds_ptd = BuiltinDataSerializer::from_publication_topic_data(
      &discovered_writer_data.publication_topic_data,
    );
    bds_wp.merge(bds_ptd)
  }

  // -----------------------

  pub fn from_discovered_reader_data_key(
    discovered_reader_data: &'a DiscoveredReaderDataKey,
  ) -> BuiltinDataSerializer<'a> {
    BuiltinDataSerializer::from_endpoint_guid(&discovered_reader_data.0)
  }

  pub fn from_discovered_writer_data_key(
    discovered_writer_data: &'a DiscoveredWriterDataKey,
  ) -> BuiltinDataSerializer<'a> {
    BuiltinDataSerializer::from_endpoint_guid(&discovered_writer_data.0)
  }

  pub fn serialize<S: Serializer>(
    self,
    serializer: S,
    add_sentinel: bool,
  ) -> Result<S::Ok, S::Error> {
    let mut s = serializer
      .serialize_struct("SPDPParticipantData", self.fields_amount())
      .unwrap();

    self.add_protocol_version::<S>(&mut s);
    self.add_vendor_id::<S>(&mut s);
    self.add_expects_inline_qos::<S>(&mut s);
    self.add_participant_guid::<S>(&mut s);
    self.add_metatraffic_unicast_locators::<S>(&mut s);
    self.add_metatraffic_multicast_locators::<S>(&mut s);
    self.add_default_unicast_locators::<S>(&mut s);
    self.add_default_multicast_locators::<S>(&mut s);
    self.add_available_builtin_endpoint_set::<S>(&mut s);
    self.add_lease_duration::<S>(&mut s);
    self.add_manual_liveliness_count::<S>(&mut s);
    self.add_builtin_endpoint_qos::<S>(&mut s);
    self.add_entity_name::<S>(&mut s);

    self.add_endpoint_guid::<S>(&mut s);
    self.add_unicast_locator_list::<S>(&mut s);
    self.add_multicast_locator_list::<S>(&mut s);

    self.add_data_max_size_serialized::<S>(&mut s);

    self.add_topic_name::<S>(&mut s);
    self.add_type_name::<S>(&mut s);
    self.add_durability::<S>(&mut s);
    self.add_deadline::<S>(&mut s);
    self.add_latency_budget::<S>(&mut s);
    self.add_liveliness::<S>(&mut s);
    self.add_reliability::<S>(&mut s);
    self.add_ownership::<S>(&mut s);
    self.add_destination_order::<S>(&mut s);
    self.add_time_based_filter::<S>(&mut s);
    self.add_presentation::<S>(&mut s);
    self.add_lifespan::<S>(&mut s);
    self.add_history::<S>(&mut s);
    self.add_resource_limits::<S>(&mut s);

    self.add_content_filter_property::<S>(&mut s);

    if add_sentinel {
      s.serialize_field("sentinel", &1_u32).unwrap();
    }

    s.end()
  }

  pub fn serialize_key<S: Serializer>(
    self,
    serializer: S,
    add_sentinel: bool,
  ) -> Result<S::Ok, S::Error> {
    let mut s = serializer
      .serialize_struct("SPDPParticipantData", self.fields_amount())
      .unwrap();

    self.add_participant_guid::<S>(&mut s);
    self.add_endpoint_guid::<S>(&mut s);

    if add_sentinel {
      s.serialize_field("sentinel", &1_u32).unwrap();
    }

    s.end()
  }

  fn fields_amount(&self) -> usize {
    let mut count: usize = 0;

    let empty_ll = LocatorList::new();
    count += self.protocol_version.is_some() as usize;
    count += self.vendor_id.is_some() as usize;
    count += self.expects_inline_qos.is_some() as usize;
    count += self.participant_guid.is_some() as usize;
    count += self.metatraffic_unicast_locators.unwrap_or(&empty_ll).len();
    count += self
      .metatraffic_multicast_locators
      .unwrap_or(&empty_ll)
      .len();
    count += self.default_unicast_locators.unwrap_or(&empty_ll).len();
    count += self.default_multicast_locators.unwrap_or(&empty_ll).len();
    count += self.available_builtin_endpoints.is_some() as usize;
    count += self.lease_duration.is_some() as usize;
    count += self.manual_liveliness_count.is_some() as usize;
    count += self.builtin_endpoint_qos.is_some() as usize;
    count += self.entity_name.is_some() as usize;

    count += self.endpoint_guid.is_some() as usize;
    count += self.unicast_locator_list.unwrap_or(&empty_ll).len();
    count += self.multicast_locator_list.unwrap_or(&empty_ll).len();

    count += self.data_max_size_serialized.is_some() as usize;

    count += self.topic_name.is_some() as usize;
    count += self.type_name.is_some() as usize;
    count += self.durability.is_some() as usize;
    count += self.deadline.is_some() as usize;
    count += self.latency_budget.is_some() as usize;
    count += self.liveliness.is_some() as usize;
    count += self.reliability.is_some() as usize;
    count += self.ownership.is_some() as usize;
    count += self.destination_order.is_some() as usize;
    count += self.time_based_filter.is_some() as usize;
    count += self.presentation.is_some() as usize;
    count += self.lifespan.is_some() as usize;
    count += self.history.is_some() as usize;
    count += self.resource_limits.is_some() as usize;

    count += self.content_filter_property.is_some() as usize;

    count
  }

  fn add_protocol_version<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    match self.protocol_version {
      Some(pv) => {
        s.serialize_field("protocol_version", &ProtocolVersionData::from(pv))
          .unwrap();
      }
      None => s
        .serialize_field(
          "protocol_version",
          &ProtocolVersionData::from(ProtocolVersion::PROTOCOLVERSION_2_3),
        )
        .unwrap(),
    }
  }

  fn add_vendor_id<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    match self.vendor_id {
      Some(vid) => {
        s.serialize_field("vendor_id", &VendorIdData::from(vid))
          .unwrap();
      }
      None => s
        .serialize_field("vendor_id", &VendorIdData::from(VendorId::VENDOR_UNKNOWN))
        .unwrap(),
    }
  }

  fn add_expects_inline_qos<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(iqos) = self.expects_inline_qos {
      let iq = ExpectsInlineQos {
        parameter_id: ParameterId::PID_EXPECTS_INLINE_QOS,
        parameter_length: 4,
        expects_inline_qos: iqos,
      };
      s.serialize_field("expects_inline_qos", &iq).unwrap();
    }
  }

  fn add_participant_guid<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(guid) = self.participant_guid {
      s.serialize_field(
        "participant_guid",
        &GUIDData::from(guid, ParameterId::PID_PARTICIPANT_GUID),
      )
      .unwrap();
    }
  }

  fn add_metatraffic_unicast_locators<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    let locators = match self.metatraffic_unicast_locators {
      Some(l) => l,
      None => return,
    };

    // TODO: make sure this works with multiple locators
    for locator in locators {
      s.serialize_field(
        "metatraffic_unicast_locators",
        &LocatorData::from(locator, ParameterId::PID_METATRAFFIC_UNICAST_LOCATOR),
      )
      .unwrap();
    }
  }

  fn add_metatraffic_multicast_locators<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    let locators = match self.metatraffic_multicast_locators {
      Some(l) => l,
      None => return,
    };

    // TODO: make sure this works with multiple locators
    for locator in locators {
      s.serialize_field(
        "metatraffic_multicast_locators",
        &LocatorData::from(locator, ParameterId::PID_METATRAFFIC_MULTICAST_LOCATOR),
      )
      .unwrap();
    }
  }

  fn add_default_unicast_locators<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    let locators = match self.default_unicast_locators {
      Some(l) => l,
      None => return,
    };

    // TODO: make sure this works with multiple locators
    for locator in locators {
      s.serialize_field(
        "default_unicast_locators",
        &LocatorData::from(locator, ParameterId::PID_DEFAULT_UNICAST_LOCATOR),
      )
      .unwrap();
    }
  }

  fn add_default_multicast_locators<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    let locators = match self.default_multicast_locators {
      Some(l) => l,
      None => return,
    };

    // TODO: make sure this works with multiple locators
    for locator in locators {
      s.serialize_field(
        "default_multicast_locators",
        &LocatorData::from(locator, ParameterId::PID_DEFAULT_MULTICAST_LOCATOR),
      )
      .unwrap();
    }
  }

  fn add_available_builtin_endpoint_set<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(eps) = self.available_builtin_endpoints {
      s.serialize_field(
        "available_builtin_endpoints",
        &BuiltinEndpointSetData::from(eps, ParameterId::PID_BUILTIN_ENDPOINT_SET),
      )
      .unwrap();
    }
  }

  fn add_lease_duration<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(dur) = self.lease_duration {
      s.serialize_field("lease_duration", &DurationData::from(dur))
        .unwrap();
    }
  }

  fn add_manual_liveliness_count<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(liv) = self.manual_liveliness_count {
      let mliv = ManualLivelinessCount {
        parameter_id: ParameterId::PID_PARTICIPANT_MANUAL_LIVELINESS_COUNT,
        parameter_length: 4,
        manual_liveliness_count: liv,
      };
      s.serialize_field("manual_liveliness_count", &mliv).unwrap();
    }
  }

  fn add_builtin_endpoint_qos<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(qos) = self.builtin_endpoint_qos {
      s.serialize_field("builtin_endpoint_qos", &BuiltinEndpointQosData::from(qos))
        .unwrap();
    }
  }

  fn add_entity_name<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(name) = self.entity_name.as_ref() {
      let ename = EntityName {
        parameter_id: ParameterId::PID_ENTITY_NAME,
        // adding 4 bytes for string lenght and 1 byte for terminate
        parameter_length: name.len() as u16 + 5,
        entity_name: name.to_string(),
      };
      s.serialize_field("entity_name", &ename).unwrap();
    }
  }

  fn add_endpoint_guid<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(guid) = self.endpoint_guid {
      s.serialize_field(
        "endpoint_guid",
        &GUIDData::from(guid, ParameterId::PID_ENDPOINT_GUID),
      )
      .unwrap();
    }
  }

  fn add_unicast_locator_list<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    let locators = match self.unicast_locator_list {
      Some(l) => l,
      None => return,
    };

    // TODO: make sure this works with multiple locators
    for locator in locators {
      s.serialize_field(
        "default_unicast_locators",
        &LocatorData::from(locator, ParameterId::PID_UNICAST_LOCATOR),
      )
      .unwrap();
    }
  }

  fn add_multicast_locator_list<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    let locators = match self.multicast_locator_list {
      Some(l) => l,
      None => return,
    };

    // TODO: make sure this works with multiple locators
    for locator in locators {
      s.serialize_field(
        "default_unicast_locators",
        &LocatorData::from(locator, ParameterId::PID_MULTICAST_LOCATOR),
      )
      .unwrap();
    }
  }

  fn add_topic_name<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(name) = self.topic_name {
      s.serialize_field(
        "topic_name",
        &StringData::new(ParameterId::PID_TOPIC_NAME, name.to_string()),
      )
      .unwrap();
    }
  }

  fn add_type_name<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(name) = self.type_name {
      s.serialize_field(
        "type_name",
        &StringData::new(ParameterId::PID_TYPE_NAME, name.to_string()),
      )
      .unwrap();
    }
  }

  fn add_durability<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(dur) = self.durability {
      s.serialize_field(
        "durability",
        &QosData::new(ParameterId::PID_DURABILITY, dur),
      )
      .unwrap();
    }
  }

  fn add_deadline<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(dl) = self.deadline {
      s.serialize_field("deadline", &QosData::new(ParameterId::PID_DEADLINE, dl))
        .unwrap();
    }
  }

  fn add_latency_budget<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(lb) = self.latency_budget {
      s.serialize_field(
        "latency_budget",
        &QosData::new(ParameterId::PID_LATENCY_BUDGET, lb),
      )
      .unwrap();
    }
  }

  fn add_liveliness<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    #[derive(Serialize, Clone, Copy)]
    #[repr(u32)]
    enum LivelinessKind {
      Automatic,
      ManualByParticipant,
      ManualbyTopic,
    }
    #[derive(Serialize, Clone, Copy)]
    struct LivelinessData {
      pub kind: LivelinessKind,
      pub lease_duration: Duration,
    }
    if let Some(l) = self.liveliness {
      let data = match l {
        Liveliness::Automatic { lease_duration } => QosData::new(
          ParameterId::PID_LIVELINESS,
          LivelinessData {
            kind: LivelinessKind::Automatic,
            lease_duration,
          },
        ),
        Liveliness::ManualByParticipant { lease_duration } => QosData::new(
          ParameterId::PID_LIVELINESS,
          LivelinessData {
            kind: LivelinessKind::ManualByParticipant,
            lease_duration,
          },
        ),
        Liveliness::ManualByTopic { lease_duration } => QosData::new(
          ParameterId::PID_LIVELINESS,
          LivelinessData {
            kind: LivelinessKind::ManualbyTopic,
            lease_duration,
          },
        ),
      };
      s.serialize_field("liveliness", &data).unwrap();
    }
  }

  fn add_reliability<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    #[derive(Serialize, Clone)]
    struct ReliabilityBestEffortData {
      pub reliability_kind: ReliabilityKind,
      pub max_blocking_time: Duration,
    }

    if let Some(rel) = self.reliability {
      match rel {
        Reliability::BestEffort => {
          let data = ReliabilityBestEffortData {
            reliability_kind: ReliabilityKind::BEST_EFFORT,
            max_blocking_time: Duration::from(StdDuration::from_secs(0)),
          };
          s.serialize_field(
            "reliability",
            &QosData::new(ParameterId::PID_RELIABILITY, &data),
          )
          .unwrap();
        }
        Reliability::Reliable { max_blocking_time } => {
          let data = ReliabilityBestEffortData {
            reliability_kind: ReliabilityKind::RELIABLE,
            max_blocking_time,
          };
          s.serialize_field(
            "reliability",
            &QosData::new(ParameterId::PID_RELIABILITY, &data),
          )
          .unwrap();
        }
      }
    }
  }

  fn add_ownership<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    #[derive(Serialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    enum OwnershipKind {
      Shared,
      Exclusive,
    }

    #[derive(Serialize)]
    struct OwnershipData {
      pub parameter_id: ParameterId,
      pub parameter_length: u16,
      pub kind: OwnershipKind,
    }
    if let Some(own) = self.ownership {
      match own {
        Ownership::Shared => {
          s.serialize_field(
            "ownership",
            &OwnershipData {
              parameter_id: ParameterId::PID_OWNERSHIP,
              parameter_length: 4,
              kind: OwnershipKind::Shared,
            },
          )
          .unwrap();
        }
        Ownership::Exclusive { strength } => {
          s.serialize_field(
            "ownership",
            &OwnershipData {
              parameter_id: ParameterId::PID_OWNERSHIP,
              parameter_length: 4,
              kind: OwnershipKind::Exclusive,
            },
          )
          .unwrap();
          s.serialize_field(
            "ownership_strength",
            &I32Data::new(ParameterId::PID_OWNERSHIP_STRENGTH, strength),
          )
          .unwrap();
        }
      }
    }
  }

  fn add_destination_order<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(deor) = self.destination_order {
      s.serialize_field(
        "destination_order",
        &QosData::new(ParameterId::PID_DESTINATION_ORDER, deor),
      )
      .unwrap();
    }
  }

  fn add_time_based_filter<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(tbf) = self.time_based_filter {
      s.serialize_field(
        "time_based_filter",
        &QosData::new(ParameterId::PID_TIME_BASED_FILTER, tbf),
      )
      .unwrap();
    }
  }

  fn add_presentation<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(p) = self.presentation {
      s.serialize_field(
        "presentation",
        &QosData::new(ParameterId::PID_PRESENTATION, p),
      )
      .unwrap();
    }
  }

  fn add_lifespan<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(ls) = self.lifespan {
      s.serialize_field("lifespan", &QosData::new(ParameterId::PID_LIFESPAN, ls))
        .unwrap();
    }
  }

  fn add_history<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    #[derive(Serialize, Clone)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    enum HistoryKind {
      KeepLast,
      KeepAll,
    }

    #[derive(Serialize, Clone)]
    struct HistoryData {
      pub kind: HistoryKind,
      pub depth: i32,
    }

    if let Some(hs) = self.history {
      let history_data = match hs {
        History::KeepLast { depth } => HistoryData {
          kind: HistoryKind::KeepLast,
          depth,
        },
        History::KeepAll => HistoryData {
          kind: HistoryKind::KeepAll,
          depth: 0,
        },
      };
      s.serialize_field(
        "history",
        &QosData::new(ParameterId::PID_HISTORY, &history_data),
      )
      .unwrap();
    }
  }

  fn add_resource_limits<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(rl) = self.resource_limits {
      s.serialize_field(
        "resource_limits",
        &QosData::new(ParameterId::PID_RESOURCE_LIMITS, rl),
      )
      .unwrap();
    }
  }

  fn add_content_filter_property<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(cfp) = self.content_filter_property {
      s.serialize_field(
        "content_filter_property",
        &ContentFilterPropertyData::new(cfp),
      )
      .unwrap();
    }
  }

  fn add_data_max_size_serialized<S: Serializer>(&self, s: &mut S::SerializeStruct) {
    if let Some(dmss) = self.data_max_size_serialized {
      s.serialize_field(
        "data_max_size_serialized",
        &U32Data::new(ParameterId::PID_TYPE_MAX_SIZE_SERIALIZED, dmss),
      )
      .unwrap();
    }
  }
}
