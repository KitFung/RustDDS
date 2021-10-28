
use std::time::Instant;
use crate::network::util::get_local_unicast_socket_address;
use crate::discovery::data_types::topic_data::WriterProxy;
use crate::discovery::data_types::topic_data::PublicationBuiltinTopicData;

use crate::discovery::data_types::topic_data::DiscoveredReaderData_Key;
use crate::discovery::data_types::topic_data::DiscoveredWriterData_Key;
use crate::discovery::data_types::topic_data::DiscoveredTopicData_Key;
use crate::discovery::data_types::topic_data::ReaderProxy;
use crate::network::util::get_local_multicast_locators;
use crate::dds::data_types::SubscriptionBuiltinTopicData;
use crate::dds::rtps_reader_proxy::RtpsReaderProxy;
#[allow(unused_imports)] use log::{debug, error, warn, info,trace};

use mio::{Ready, Poll, PollOpt, Events};
use mio_extras::timer::Timer;
use mio_extras::channel as mio_channel;

use std::{
  sync::{Arc, RwLock},
  sync::RwLockReadGuard,
  sync::RwLockWriteGuard,
  time::Duration as StdDuration,
};

use crate::{
  dds::{
    with_key::datareader::DataReader, with_key::datareader::DataReader_CDR,
    /*with_key::datawriter::DataWriter,*/ with_key::datawriter::DataWriter_CDR,
    topic::*,
    participant::{DomainParticipantWeak},
    Publisher, Subscriber,
    qos::{
      QosPolicies,
      policy::{
        Reliability, History, Durability, Presentation, PresentationAccessScope, Deadline,
        Ownership, Liveliness, TimeBasedFilter, DestinationOrder,
      },
    },
    readcondition::ReadCondition,
  },
  dds::values::result::{Error,Result},
  structure::entity::RTPSEntity,
  structure::guid::{ GUID, GuidPrefix, },
  dds::qos::QosPolicyBuilder,
};

use crate::discovery::{
  data_types::spdp_participant_data
    ::{SPDPDiscoveredParticipantData, SPDPDiscoveredParticipantData_Key},
  data_types::topic_data::{DiscoveredWriterData, DiscoveredReaderData},
  discovery_db::DiscoveryDB,
};

use crate::structure::{duration::Duration, guid::EntityId, time::Timestamp};

use crate::serialization::{pl_cdr_deserializer::PlCdrDeserializerAdapter};

use crate::network::constant::*;
use super::data_types::topic_data::{
  DiscoveredTopicData, ParticipantMessageData, ParticipantMessageDataKind,
};

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum DiscoveryCommand {
  STOP_DISCOVERY,
  REMOVE_LOCAL_WRITER { guid: GUID },
  REMOVE_LOCAL_READER { guid: GUID },
  MANUAL_ASSERT_LIVELINESS,
  ASSERT_TOPIC_LIVELINESS { writer_guid: GUID , manual_assertion: bool, },
}

pub struct LivelinessState {
  last_auto_update: Timestamp,
  last_manual_participant_update: Timestamp,
}

impl LivelinessState {
  pub fn new() -> LivelinessState {
    LivelinessState {
      last_auto_update: Timestamp::now(),
      last_manual_participant_update: Timestamp::now(),
    }
  }
}

pub(crate) struct Discovery {
  poll: Poll,
  domain_participant: DomainParticipantWeak,
  discovery_db: Arc<RwLock<DiscoveryDB>>,

  // Discovery started sender confirms to application thread that we are running
  discovery_started_sender: std::sync::mpsc::Sender<Result<()>>,
  // notification sender goes to dp_event_loop thread
  discovery_updated_sender: mio_channel::SyncSender<DiscoveryNotificationType>,
  // Discovery gets commands from dp_event_loop from this channel
  discovery_command_receiver: mio_channel::Receiver<DiscoveryCommand>,

  liveliness_state: LivelinessState,

  // DDS Subsciber and Publisher for Discovery
  discovery_subscriber: Subscriber,
  discovery_publisher: Publisher,

  // Handling of "DCPSParticipant" topic. This is the mother of all topics
  // where participants announce their presence and built-in readers and writers.
  dcps_participant_topic: Topic,
  dcps_participant_reader: DataReader<SPDPDiscoveredParticipantData,PlCdrDeserializerAdapter<SPDPDiscoveredParticipantData>>,
  dcps_participant_writer: DataWriter_CDR<SPDPDiscoveredParticipantData>,
  participant_cleanup_timer: Timer<()>,  // garbage collection timer for dead remote particiapnts
  participant_send_info_timer: Timer<()>, // timer to periodically announce our presence

  // Topic "DCPSSubscription" - announcing and detecting Readers
  dcps_subscription_topic: Topic,
  dcps_subscription_reader: DataReader<DiscoveredReaderData, PlCdrDeserializerAdapter<DiscoveredReaderData>>,
  dcps_subscription_writer: DataWriter_CDR<DiscoveredReaderData>,
  readers_send_info_timer: Timer<()>,

  // Topic "DCPSPublication" - announcing and detecting Writers
  dcps_publication_topic: Topic,
  dcps_publication_reader: DataReader<DiscoveredWriterData, PlCdrDeserializerAdapter<DiscoveredWriterData>>,
  dcps_publication_writer: DataWriter_CDR<DiscoveredWriterData>,
  writers_send_info_timer: Timer<()>,

  // Topic "DCPSTopic" - annoncing and detecting topics
  dcps_topic_topic: Topic,
  dcps_topic_reader: DataReader<DiscoveredTopicData, PlCdrDeserializerAdapter<DiscoveredTopicData>>,
  dcps_topic_writer: DataWriter_CDR<DiscoveredTopicData>,
  topic_info_send_timer: Timer<()>,
  topic_cleanup_timer: Timer<()>,

  // DCPSParticipantMessage - used by participants to communicate liveness
  participant_message_topic: Topic,
  dcps_participant_message_reader: DataReader_CDR<ParticipantMessageData>,
  dcps_participant_message_writer: DataWriter_CDR<ParticipantMessageData>,
  dcps_participant_message_timer: Timer<()>,
}

impl Discovery {
  const PARTICIPANT_CLEANUP_PERIOD: StdDuration = StdDuration::from_secs(2);
  const TOPIC_CLEANUP_PERIOD: StdDuration = StdDuration::from_secs(10); // timer for cleaning up inactive topics
  const SEND_PARTICIPANT_INFO_PERIOD: StdDuration = StdDuration::from_secs(2);
  const SEND_READERS_INFO_PERIOD: StdDuration = StdDuration::from_secs(2);
  const SEND_WRITERS_INFO_PERIOD: StdDuration = StdDuration::from_secs(2);
  const SEND_TOPIC_INFO_PERIOD: StdDuration = StdDuration::from_secs(20);
  const CHECK_PARTICIPANT_MESSAGES: StdDuration = StdDuration::from_secs(1);

  pub(crate) const PARTICIPANT_MESSAGE_QOS: QosPolicies = QosPolicies {
    durability: Some(Durability::TransientLocal),
    presentation: None,
    deadline: None,
    latency_budget: None,
    ownership: None,
    liveliness: None,
    time_based_filter: None,
    reliability: Some(Reliability::Reliable {
      max_blocking_time: Duration::DURATION_ZERO,
    }),
    destination_order: None,
    history: Some(History::KeepLast { depth: 1 }),
    resource_limits: None,
    lifespan: None,
  };

  pub fn new(
    domain_participant: DomainParticipantWeak,
    discovery_db: Arc<RwLock<DiscoveryDB>>,
    discovery_started_sender: std::sync::mpsc::Sender<Result<()>>,
    discovery_updated_sender: mio_channel::SyncSender<DiscoveryNotificationType>,
    discovery_command_receiver: mio_channel::Receiver<DiscoveryCommand>,
  ) -> Result<Discovery> {

    // helper macro to handle initialization failures.
    macro_rules! try_construct {
      ($constructor:expr, $msg:literal) => (
        match $constructor {
          Ok(r) => r,
          Err(e) => {
            error!($msg, e);
            discovery_started_sender.send(Err(Error::OutOfResources))
              .unwrap_or(()); // We are trying to quit. If send fails, just ignore it.
            return Err(Error::OutOfResources)
          }
        }
      )
    }

    let poll = try_construct!( mio::Poll::new() , "Failed to allocate discovery poll. {:?}");

    try_construct!( poll.register(
        &discovery_command_receiver,
        DISCOVERY_COMMAND_TOKEN,
        Ready::readable(),
        PollOpt::edge(),
      ),
      "Failed to register Discovery poll. {:?}");

    let discovery_subscriber_qos = Discovery::subscriber_qos();
    let discovery_publisher_qos = Discovery::publisher_qos();

    // Create DDS Publisher and Subscriber for Discovery.
    // These are needed to create DataWriter and DataReader objects
    let discovery_subscriber = try_construct!(
      domain_participant.create_subscriber(&discovery_subscriber_qos),
      "Unable to create Discovery Subscriber. {:?}"
    );
    let discovery_publisher = try_construct!(
      domain_participant.create_publisher(&discovery_publisher_qos) ,
      "Unable to create Discovery Publisher. {:?}");

    // Participant
    let dcps_participant_topic = try_construct!(
      domain_participant.create_topic(
        "DCPSParticipant".to_string(),
        "SPDPDiscoveredParticipantData".to_string(),
        &Discovery::create_spdp_patricipant_qos(),
        TopicKind::WithKey,
      ),
      "Unable to create DCPSParticipant topic. {:?}");
  
    let dcps_participant_reader = try_construct!( discovery_subscriber
      .create_datareader_with_entityid::<SPDPDiscoveredParticipantData,PlCdrDeserializerAdapter<SPDPDiscoveredParticipantData>>(
        dcps_participant_topic.clone(),
        EntityId::ENTITYID_SPDP_BUILTIN_PARTICIPANT_READER,
        None,
      ) ,"Unable to create DataReader for DCPSParticipant. {:?}");

    let dcps_participant_writer = try_construct!( discovery_publisher
      .create_datawriter_CDR_with_entityid::<SPDPDiscoveredParticipantData>(
        EntityId::ENTITYID_SPDP_BUILTIN_PARTICIPANT_WRITER,
        dcps_participant_topic.clone(),
        None,
      ) ,"Unable to create DataWriter for DCPSParticipant. {:?}");

    // register participant reader
    try_construct!( poll.register(
      &dcps_participant_reader,
      DISCOVERY_PARTICIPANT_DATA_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Failed to register participant reader to poll. {:?}");

    // create lease duration check timer
    let mut participant_cleanup_timer: Timer<()> = Timer::default();
    participant_cleanup_timer.set_timeout(Discovery::PARTICIPANT_CLEANUP_PERIOD, ());
    try_construct!( poll.register(
      &participant_cleanup_timer,
      DISCOVERY_PARTICIPANT_CLEANUP_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to create participant cleanup timer. {:?}");

    // creating timer for sending out own participant data
    let mut participant_send_info_timer: Timer<()> = Timer::default();
    participant_send_info_timer.set_timeout(Discovery::SEND_PARTICIPANT_INFO_PERIOD, ());

    try_construct!( poll.register(
      &participant_send_info_timer,
      DISCOVERY_SEND_PARTICIPANT_INFO_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to register participant info sender. {:?}");

    
    // Subscriptions: What are the Readers on the network and what are they subscribing to?

    let dcps_subscription_topic = try_construct!( domain_participant.create_topic(
      "DCPSSubscription".to_string(),
      "DiscoveredReaderData".to_string(),
      &discovery_subscriber_qos,
      TopicKind::WithKey,
    ) ,"Unable to create DCPSSubscription topic. {:?}");

    let dcps_subscription_reader = try_construct!( discovery_subscriber
      .create_datareader_with_entityid::<DiscoveredReaderData, PlCdrDeserializerAdapter<DiscoveredReaderData>>(
        dcps_subscription_topic.clone(),
        EntityId::ENTITYID_SEDP_BUILTIN_SUBSCRIPTIONS_READER,
        None,
      ) ,"Unable to create DataReader for DCPSSubscription. {:?}");

    try_construct!( poll.register(
      &dcps_subscription_reader,
      DISCOVERY_READER_DATA_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to register subscription reader. {:?}");

    let dcps_subscription_writer = try_construct!( discovery_publisher
      .create_datawriter_CDR_with_entityid::<DiscoveredReaderData>(
        EntityId::ENTITYID_SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
        dcps_subscription_topic.clone(),
        None,
      ) ,"Unable to create DataWriter for DCPSSubscription. {:?}");

    let mut readers_send_info_timer: Timer<()> = Timer::default();
    readers_send_info_timer.set_timeout(Discovery::SEND_READERS_INFO_PERIOD, ());
    try_construct!( poll.register(
      &readers_send_info_timer,
      DISCOVERY_SEND_READERS_INFO_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to register readers info sender. {:?}");



    // Publication : Who are thr Writers?

    let dcps_publication_topic = try_construct!( domain_participant.create_topic(
      "DCPSPublication".to_string(),
      "DiscoveredWriterData".to_string(),
      &discovery_publisher_qos,
      TopicKind::WithKey,
    ) ,"Unable to create DCPSPublication topic. {:?}");

    let dcps_publication_reader = try_construct!( discovery_subscriber
      .create_datareader_with_entityid::<DiscoveredWriterData, PlCdrDeserializerAdapter<DiscoveredWriterData>>(
        dcps_publication_topic.clone(),
        EntityId::ENTITYID_SEDP_BUILTIN_PUBLICATIONS_READER,
        None,
      ) ,"Unable to create DataReader for DCPSPublication. {:?}");

    try_construct!( poll.register(
      &dcps_publication_reader,
      DISCOVERY_WRITER_DATA_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to regiser writers info sender. {:?}");

    let dcps_publication_writer = try_construct!( discovery_publisher
      .create_datawriter_CDR_with_entityid::<DiscoveredWriterData>(
        EntityId::ENTITYID_SEDP_BUILTIN_PUBLICATIONS_WRITER,
        dcps_publication_topic.clone(),
        None,
      ) ,"Unable to create DataWriter for DCPSPublication. {:?}");

    let mut writers_send_info_timer: Timer<()> = Timer::default();
    writers_send_info_timer.set_timeout(Discovery::SEND_WRITERS_INFO_PERIOD, ());
    try_construct!( poll.register(
      &writers_send_info_timer,
      DISCOVERY_SEND_WRITERS_INFO_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to register readers info sender. {:?}");


    // Topic topic (not a typo)

    let dcps_topic_topic = try_construct!( domain_participant.create_topic(
      "DCPSTopic".to_string(),
      "DiscoveredTopicData".to_string(),
      &QosPolicyBuilder::new().build(), //TODO: check what this should be
      TopicKind::WithKey,
    ) ,"Unable to create DCPSTopic topic. {:?}");


    let dcps_topic_reader = try_construct!( discovery_subscriber
      .create_datareader_with_entityid::<DiscoveredTopicData, PlCdrDeserializerAdapter<DiscoveredTopicData>>(
        dcps_topic_topic.clone(),
        EntityId::ENTITYID_SEDP_BUILTIN_TOPIC_READER,
        None,
      ) ,"Unable to create DataReader for DCPSTopic. {:?}");

    try_construct!( poll.register(
      &dcps_topic_reader,
      DISCOVERY_TOPIC_DATA_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to register topic reader. {:?}");

    let dcps_topic_writer = try_construct!( discovery_publisher
      .create_datawriter_CDR_with_entityid::<DiscoveredTopicData>(
        EntityId::ENTITYID_SEDP_BUILTIN_TOPIC_WRITER,
        dcps_topic_topic.clone(),
        None,
      ) ,"Unable to create DataWriter for DCPSTopic. {:?}");

    let mut topic_info_send_timer: Timer<()> = Timer::default();
    topic_info_send_timer.set_timeout(Discovery::SEND_TOPIC_INFO_PERIOD, ());
    try_construct!( poll.register(
      &topic_info_send_timer,
      DISCOVERY_SEND_TOPIC_INFO_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to register topic info sender. {:?}");

    // create lease duration check timer
    let mut topic_cleanup_timer: Timer<()> = Timer::default();
    topic_cleanup_timer.set_timeout(Discovery::TOPIC_CLEANUP_PERIOD, ());
    try_construct!( poll.register(
      &topic_cleanup_timer,
      DISCOVERY_TOPIC_CLEANUP_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to register topic cleanup timer. {:?}");


    // Participant Message Data 8.4.13

    let participant_message_topic = try_construct!( domain_participant.create_topic(
      "DCPSParticipantMessage".to_string(),
      "ParticipantMessageData".to_string(),
      &Discovery::PARTICIPANT_MESSAGE_QOS,
      TopicKind::WithKey,
    ) ,"Unable to create DCPSParticipantMessage topic. {:?}");

    let dcps_participant_message_reader = try_construct!( discovery_subscriber
      .create_datareader_CDR_with_entityid::<ParticipantMessageData>(
        participant_message_topic.clone(),
        EntityId::ENTITYID_P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
        None,
      ) ,"Unable to create DCPSParticipantMessage reader. {:?}");

    try_construct!( poll.register(
      &dcps_participant_message_reader,
      DISCOVERY_PARTICIPANT_MESSAGE_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to register DCPSParticipantMessage reader. {:?}");

    let dcps_participant_message_writer = try_construct!( discovery_publisher
      .create_datawriter_CDR_with_entityid::<ParticipantMessageData>(
        EntityId::ENTITYID_P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
        participant_message_topic.clone(),
        None,
      ) ,"Unable to create DCPSParticipantMessage writer. {:?}");

    let mut dcps_participant_message_timer = mio_extras::timer::Timer::default();
    dcps_participant_message_timer.set_timeout(Discovery::CHECK_PARTICIPANT_MESSAGES, ());
    try_construct!( poll.register(
      &dcps_participant_message_timer,
      DISCOVERY_PARTICIPANT_MESSAGE_TIMER_TOKEN,
      Ready::readable(),
      PollOpt::edge(),
    ) ,"Unable to register DCPSParticipantMessage timer. {:?}");


    Ok(Discovery {
      poll,
      domain_participant,
      discovery_db,
      discovery_started_sender,
      discovery_updated_sender,
      discovery_command_receiver,

      liveliness_state: LivelinessState::new(),

      discovery_subscriber,
      discovery_publisher,

      dcps_participant_topic,
      dcps_participant_reader,
      dcps_participant_writer,
      participant_cleanup_timer, 
      participant_send_info_timer,

      dcps_subscription_topic,
      dcps_subscription_reader,
      dcps_subscription_writer,
      readers_send_info_timer,

      dcps_publication_topic,
      dcps_publication_reader,
      dcps_publication_writer,
      writers_send_info_timer,

      dcps_topic_topic,
      dcps_topic_reader,
      dcps_topic_writer,
      topic_info_send_timer,
      topic_cleanup_timer,

      participant_message_topic,
      dcps_participant_message_reader,
      dcps_participant_message_writer,
      dcps_participant_message_timer,
    })
  }


  pub fn discovery_event_loop(&mut self) {

    self.initialize_participant();

    // send out info about non-built-in Writers and Readers that we have.
    self.write_writers_info();
    self.write_readers_info();

    match self.discovery_started_sender.send(Ok(())) {
      Ok(_) => (),
      _ => return, // Participant has probably crashed at this point
    };

    loop {
      let mut events = Events::with_capacity(32);  // Should this be outside of the loop?
      match self.poll.poll(&mut events, None) {
        Ok(_) => (),
        Err(e) => {
          error!("Failed in waiting of poll in discovery. {:?}", e);
          return
        }
      }

      for event in events.into_iter() {
        match event.token() {
          DISCOVERY_COMMAND_TOKEN => {
            while let Ok(command) = self.discovery_command_receiver.try_recv() {
              match command {
                DiscoveryCommand::STOP_DISCOVERY => {
                  info!("Stopping Discovery");
                  // disposing readers
                  let db = self.discovery_db_read();
                  for reader in db.get_all_local_topic_readers() {
                    self.dcps_subscription_writer
                      .dispose(DiscoveredReaderData_Key(reader.reader_proxy.remote_reader_guid), None)
                      .unwrap_or(());
                  }

                  for writer in db.get_all_local_topic_writers() {
                    self.dcps_publication_writer
                      .dispose(DiscoveredWriterData_Key(writer.writer_proxy.remote_writer_guid), None)
                      .unwrap_or(());
                  }
                  // finally disposing the participant we have
                  self.dcps_participant_writer.dispose(
                    SPDPDiscoveredParticipantData_Key(self.domain_participant.get_guid()), 
                    None)
                    .unwrap_or(());
                  info!("Stopped Discovery");
                  return  // terminate event loop
                }
                DiscoveryCommand::REMOVE_LOCAL_WRITER { guid } => {
                  if guid == self.dcps_publication_writer.get_guid() {
                    continue
                  }
                  self.dcps_publication_writer.dispose(DiscoveredWriterData_Key(guid), None)
                    .unwrap_or(());

                  match self.discovery_db.write() {
                    Ok(mut db) => db.remove_local_topic_writer(guid),
                    Err(e) => { error!("DiscoveryDB is poisoned. {:?}", e); return }
                  }
                }
                DiscoveryCommand::REMOVE_LOCAL_READER { guid } => {
                  if guid == self.dcps_subscription_writer.get_guid() {
                    continue
                  }

                  self.dcps_subscription_writer.dispose(DiscoveredReaderData_Key(guid), None).unwrap_or(());

                  match self.discovery_db.write() {
                    Ok(mut db) => db.remove_local_topic_reader(guid),
                    Err(e) => { error!("DiscoveryDB is poisoned. {:?}", e); return } 
                  }
                }
                DiscoveryCommand::MANUAL_ASSERT_LIVELINESS => {
                  self.liveliness_state.last_manual_participant_update = Timestamp::now();
                }
                DiscoveryCommand::ASSERT_TOPIC_LIVELINESS { writer_guid  , manual_assertion } => {
                  self.send_discovery_notification(
                    DiscoveryNotificationType::AssertTopicLiveliness { writer_guid , manual_assertion },
                  );
                }
              };
            }
          }

          DISCOVERY_PARTICIPANT_DATA_TOKEN => {
            debug!("triggered participant reader");
            self.handle_participant_reader()
          }

          DISCOVERY_PARTICIPANT_CLEANUP_TOKEN => {
            self.participant_cleanup();
            // setting next cleanup timeout
            self.participant_cleanup_timer.set_timeout(Discovery::PARTICIPANT_CLEANUP_PERIOD, ());
          }

          DISCOVERY_SEND_PARTICIPANT_INFO_TOKEN => {
            let strong_dp = match self.domain_participant.clone().upgrade() {
              Some(dp) => dp,
              None => {
                error!("DomainParticipant doesn't exist anymore, exiting Discovery.");
                return
              }
            };

            // setting 5 times the duration so lease doesn't break if update fails once or twice
            let data = SPDPDiscoveredParticipantData::from_local_participant(
              &strong_dp,
              5.0 *  Duration::from(Discovery::SEND_PARTICIPANT_INFO_PERIOD),
            );

            self.dcps_participant_writer.write(data, None).unwrap_or(());
            // reschedule timer
            self.participant_send_info_timer.set_timeout(Discovery::SEND_PARTICIPANT_INFO_PERIOD, ());
          }
          DISCOVERY_READER_DATA_TOKEN => {
            self.handle_subscription_reader(None);
          }
          DISCOVERY_SEND_READERS_INFO_TOKEN => {
            if self.read_readers_info() {
              self.write_readers_info();
            }

            self.readers_send_info_timer.set_timeout(Discovery::SEND_READERS_INFO_PERIOD, ());
          }
          DISCOVERY_WRITER_DATA_TOKEN => {
            self.handle_publication_reader(None);
          }
          DISCOVERY_SEND_WRITERS_INFO_TOKEN => {
            if self.read_writers_info() {
              self.write_writers_info();
            }

            self.writers_send_info_timer.set_timeout(Discovery::SEND_WRITERS_INFO_PERIOD, ());
          }
          DISCOVERY_TOPIC_DATA_TOKEN => {
            self.handle_topic_reader(None);
          }
          DISCOVERY_TOPIC_CLEANUP_TOKEN => {
            self.topic_cleanup();

            self.topic_cleanup_timer.set_timeout(Discovery::TOPIC_CLEANUP_PERIOD, ());
          }
          DISCOVERY_SEND_TOPIC_INFO_TOKEN => {
            self.write_topic_info();
            self.topic_info_send_timer.set_timeout(Discovery::SEND_TOPIC_INFO_PERIOD, ());
          }
          DISCOVERY_PARTICIPANT_MESSAGE_TOKEN => {
            self.handle_participant_message_reader();
          }
          DISCOVERY_PARTICIPANT_MESSAGE_TIMER_TOKEN => {
            self.write_participant_message();
            self.dcps_participant_message_timer.set_timeout(Discovery::CHECK_PARTICIPANT_MESSAGES, ());
          }
          other_token => {
            error!("discovery event loop got token: {:?}", other_token);
          }
        } // match
      } // for 
    } // loop
  } // fn


  // Initialize our own particiapnt data into the Discovery DB.
  // That causes ReaderProxies and WriterProxies to be constructed and
  // and we also get our own local readers and writers connected, both
  // built-in and user-defined.
  // If we did not do this, the Readers and Writers in this participant could not find each other.
  fn initialize_participant(&self) {
    let dp = match self.domain_participant.clone().upgrade() {
      Some(dp) => dp, 
      None => {
        error!("Cannot get actual DomainParticipant in initialize_participant! Giving up.");
        return
      }
    };

    let mc_port = get_spdp_well_known_multicast_port( dp.domain_id() );
    let uc_port = get_spdp_well_known_unicast_port( dp.domain_id(), dp.participant_id() );

    let participant_data = 
      SPDPDiscoveredParticipantData::from_local_participant(&dp, Duration::DURATION_INFINITE);
  
    // Initialize our own particiapnt data into the Discovery DB, so we can talk to ourself.
    self.discovery_db_write()
      .update_participant(&participant_data);
  
    // This will read the participant from Discovery DB and construct
    // ReaderProxy and WriterProxy objects for built-in Readers and Writers
    self.send_discovery_notification(
      DiscoveryNotificationType::ParticipantUpdated {
        guid_prefix: dp.get_guid().guidPrefix
    });

    // insert a (fake) reader proxy as multicast address, so discovery notifications are sent somewhere
    let reader_guid = GUID::new_with_prefix_and_id(
      GuidPrefix::GUIDPREFIX_UNKNOWN, EntityId::ENTITYID_SPDP_BUILTIN_PARTICIPANT_READER);

    let mut reader_proxy = ReaderProxy::new(reader_guid);
    reader_proxy.multicast_locator_list = get_local_multicast_locators(mc_port);

    let sub_topic_data = SubscriptionBuiltinTopicData::new(
      reader_guid,
      String::from("DCPSParticipant"),
      String::from("SPDPDiscoveredParticipantData"),
      &Discovery::create_spdp_patricipant_qos(),
    );
    let drd = DiscoveredReaderData {
      reader_proxy,
      subscription_topic_data: sub_topic_data,
      content_filter: None,
    };

    let writer_guid = 
      GUID::new_with_prefix_and_id(
        dp.get_guid().guidPrefix, 
        EntityId::ENTITYID_SPDP_BUILTIN_PARTICIPANT_WRITER );

    let writer_proxy = WriterProxy::new(writer_guid, 
      get_local_unicast_socket_address(uc_port), 
      get_local_multicast_locators(mc_port));

    let pub_topic_data = PublicationBuiltinTopicData::new(
      writer_guid,
      dp.get_guid(),
      String::from("DCPSParticipant"),
      String::from("SPDPDiscoveredParticipantData"),
    );
    let dwd = DiscoveredWriterData {
      last_updated: Instant::now(),
      writer_proxy,
      publication_topic_data: pub_topic_data,
    };


    // Notify local Readers and Writers in dp_event_loop
    // so that they will create WriterProxies and ReaderProxies
    // and know to communicate with them.
    info!("Creating DCPSParticipant reader proxy.");
    self.send_discovery_notification(DiscoveryNotificationType::ReaderUpdated
      { rtps_reader_proxy:  RtpsReaderProxy::from_discovered_reader_data(&drd,vec![], vec![]),
        discovered_reader_data: drd,
        _needs_new_cache_change: true,
      });
    info!("Creating DCPSParticipant writer proxy for self.");
    self.send_discovery_notification(DiscoveryNotificationType::WriterUpdated
      { discovered_writer_data: dwd,
      });
  }


  pub fn handle_participant_reader(&mut self) {
    loop {
      let s = self.dcps_participant_reader.read_next_sample();
      debug!("handle_participant_reader read {:?}", &s);
      match s {
        Ok(Some(d)) => match d.value {
            Ok(participant_data) => {
              let participant_data = participant_data.clone(); // .clone() is necessary, because .read
              // returns references to within Reader, so we cannot operate on self until we clone.
              debug!("handle_participant_reader discovered {:?}", &participant_data);
              let was_new = self.discovery_db_write()
                .update_participant(&participant_data);
              let guid_prefix = participant_data.participant_guid.guidPrefix;
              self.send_discovery_notification(
                DiscoveryNotificationType::ParticipantUpdated { guid_prefix } );
              if was_new {
                // This may be a rediscovery of a previously seen participant that
                // was temporarily lost due to network outage. Check if we already know
                // what it has (readers, writers, topics).
                info!("Participant rediscovery start");
                self.handle_topic_reader(Some(guid_prefix));
                self.handle_subscription_reader(Some(guid_prefix));
                self.handle_publication_reader(Some(guid_prefix));
                info!("Participant rediscovery finished");
              }      
            },
            // Err means that DomainParticipant was disposed
            Err(guid) => {
              self.discovery_db_write().remove_participant(guid.0.guidPrefix);
              self.send_discovery_notification(
                DiscoveryNotificationType::ParticipantLost { guid_prefix: guid.0.guidPrefix });
            }
          },
        Ok(None) => {
          //debug!("handle_participant_reader: no more data");
          return
        } // no more data
        Err(e) => error!("{:?}",e),
      }
    } // loop
  }

  pub fn handle_subscription_reader(&mut self, read_history: Option<GuidPrefix>,) {
    let drds : Vec<std::result::Result<DiscoveredReaderData,DiscoveredReaderData_Key> > = 
      match self.dcps_subscription_reader.read(std::usize::MAX,
                        if read_history.is_some() { ReadCondition::any() } 
                        else { ReadCondition::not_read() } ) 
      {
        // a lot of cloning here, but we must copy the data out of the
        // reader before we can use self again, as .read() returns references to within
        // a reader and thus self
        Ok(ds) => ds.iter().map( |d| d.value.map(|o| o.clone()) ).collect(),
        Err(e) => { error!("handle_subscription_reader: {:?}",e); return }
      };

    for d in drds {
      match d {
        Ok(d) => {
          let mut db = self.discovery_db_write();
          trace!("handle_subscription_reader discovered {:?}", &d);
          if read_history
              .map( |e| e == d.reader_proxy.remote_reader_guid.guidPrefix )
              .unwrap_or(true) {
            if let Some( (drd,rtps_reader_proxy) )  = db.update_subscription(&d) {
              debug!("handle_subscription_reader - send_discovery_notification ReaderUpdated {:?} -- {:?}",
                &drd, &rtps_reader_proxy);
              self.send_discovery_notification(
                DiscoveryNotificationType::ReaderUpdated {
                  discovered_reader_data: drd, 
                  rtps_reader_proxy,
                  _needs_new_cache_change: true,
                });  
            } else {
              info!("handle_subscription_reader - DiscoveryDB already knows reader {:?}",
                d.reader_proxy.remote_reader_guid);
            }
            db.update_topic_data_drd(&d);
            if read_history.is_some() {
              info!("Rediscovered reader {:?} topic={:?}",
                d.reader_proxy.remote_reader_guid, d.subscription_topic_data.topic_name());
            }
          } else {
            // Skip, because we were asked to look for specific
            // GuidPrefx, but it did not match.
          }
        }
        Err(reader_key) => {
          debug!("Dispose Reader {:?}", reader_key);
          self.discovery_db_write()
            .remove_topic_reader(reader_key.0);
          self.send_discovery_notification(
              DiscoveryNotificationType::ReaderLost { reader_guid: reader_key.0 });
        }
      }      
    } // loop
  }

  pub fn handle_publication_reader( &mut self, read_history: Option<GuidPrefix>, ) {
    let dwds : Vec<std::result::Result<DiscoveredWriterData,DiscoveredWriterData_Key> > = 
      match self.dcps_publication_reader.read(std::usize::MAX,
                        if read_history.is_some() { ReadCondition::any() } 
                        else { ReadCondition::not_read() } ) 
      {
        // a lot of cloning here, but we must copy the data out of the
        // reader before we can use self again, as .read() returns references to within
        // a reader and thus self
        Ok(ds) => ds.iter().map( |d| d.value.map(|o| o.clone()) ).collect(),
        Err(e) => { error!("handle_publication_reader: {:?}",e); return }
      };

    for d in dwds {
      match d {
            Ok(dwd) => {
              trace!("handle_publication_reader discovered {:?}", &dwd);
              if let Some(discovered_writer_data) = 
                  self.discovery_db_write().update_publication(&dwd) 
              {
                self.send_discovery_notification(
                    DiscoveryNotificationType::WriterUpdated{ discovered_writer_data } );
              }
              self.discovery_db_write().update_topic_data_dwd(&dwd);
              debug!("Discovered Writer {:?}", &dwd);
            },
            Err(writer_key) => {
              self.discovery_db_write().remove_topic_writer(writer_key.0);
              self.send_discovery_notification(
                DiscoveryNotificationType::WriterLost { writer_guid: writer_key.0 });
              debug!("Disposed Writer {:?}", writer_key);
            }
          
      }
    } // loop
  }

  pub fn handle_topic_reader( &mut self, read_history: Option<GuidPrefix>,
  ) 
  {
    let ts : Vec<std::result::Result<DiscoveredTopicData,DiscoveredTopicData_Key> >= 
      match self.dcps_topic_reader.read(std::usize::MAX,
                        if read_history.is_some() { ReadCondition::any() } 
                        else { ReadCondition::not_read() } ) 
      {
        // a lot of cloning here, but we must copy the data out of the
        // reader before we can use self again, as .read() returns references to within
        // a reader and thus self
        Ok(ds) => ds.iter().map( |d| d.value.map(|o| o.clone()) ).collect(),
        Err(e) => { error!("handle_topic_reader: {:?}",e); return }
      };

    for t in ts {
      match t {
        Ok(topic_data) => {
          trace!("handle_topic_reader discovered {:?}", &topic_data);
          let updated = self.discovery_db_write()
            .update_topic_data(&topic_data);
          if updated {
            self.send_discovery_notification(DiscoveryNotificationType::TopicsInfoUpdated);
          }    
        },
        // Err means disposed
        Err(key) => {
          warn!("not implemented - Topic was disposed: {:?}", &key);
        }
      }
    } // loop
  }

  // These messages are for updating participant liveliness
  // The protocol distinguises between automatic (by DDS library) 
  // and manual (by by application, via DDS API call) liveness
  // TODO: rewrite this function according to the pattern above
  pub fn handle_participant_message_reader( &mut self) {
    let participant_messages: Option<Vec<ParticipantMessageData>> =
      match self.dcps_participant_message_reader.take(100, ReadCondition::any()) {
        Ok(msgs) => Some(
          msgs
            .into_iter()
            .map(|p| p.value().clone())
            .filter_map(std::result::Result::ok)
            .collect(),
        ),
        _ => None,
      };

    let msgs = match participant_messages {
      Some(d) => d,
      None => return,
    };

    let mut db = self.discovery_db_write();
    for msg in msgs.into_iter() {
      db.update_lease_duration(msg);
    }
  }

  // TODO: Explain what happens here and by what logic
  pub fn write_participant_message(&mut self) {
    let writer_liveliness: Vec<Liveliness> = self.discovery_db_read()
      .get_all_local_topic_writers()
      .filter_map(|p| {
        let liveliness = match p.publication_topic_data.liveliness {
          Some(lv) => lv,
          None => return None,
        };

        Some(liveliness)
      })
      .collect();

    let (automatic, manual): (Vec<&Liveliness>, Vec<&Liveliness>) =
      writer_liveliness.iter().partition(|p| match p {
        Liveliness::Automatic { lease_duration: _ } => true,
        Liveliness::ManualByParticipant { lease_duration: _ } => false,
        Liveliness::ManualByTopic { lease_duration: _ } => false,
      });

    let (manual_by_participant, _manual_by_topic): (Vec<&Liveliness>, Vec<&Liveliness>) =
      manual.iter().partition(|p| match p {
        Liveliness::Automatic { lease_duration: _ } => false,
        Liveliness::ManualByParticipant { lease_duration: _ } => true,
        Liveliness::ManualByTopic { lease_duration: _ } => false,
      });

    let inow = Timestamp::now();

    // Automatic
    {
      let current_duration =
        inow.duration_since(self.liveliness_state.last_auto_update) / 3;
      let min_automatic = automatic
        .iter()
        .map(|lv| match lv {
          Liveliness::Automatic { lease_duration }
          | Liveliness::ManualByParticipant { lease_duration }
          | Liveliness::ManualByTopic { lease_duration } => lease_duration,
        })
        .min();
      trace!(
        "Current auto duration {:?}. Min auto duration {:?}",
        current_duration, min_automatic
      );
      if let Some(&mm) = min_automatic {
        if current_duration > mm {
          let pp = ParticipantMessageData {
            guid: self.domain_participant.get_guid_prefix(),
            kind:
              ParticipantMessageDataKind::PARTICIPANT_MESSAGE_DATA_KIND_AUTOMATIC_LIVELINESS_UPDATE,
            data: Vec::new(),
          };
          match self.dcps_participant_message_writer.write(pp, None) {
            Ok(_) => (),
            Err(e) => {
              error!("Failed to write ParticipantMessageData auto. {:?}", e);
              return;
            }
          }
          self.liveliness_state.last_auto_update = inow;
        }
      };
    }

    // Manual By Participant
    {
      let current_duration =
        inow.duration_since(self.liveliness_state.last_manual_participant_update) / 3;
      let min_manual_participant = manual_by_participant
        .iter()
        .map(|lv| match lv {
          Liveliness::Automatic { lease_duration }
          | Liveliness::ManualByParticipant { lease_duration }
          | Liveliness::ManualByTopic { lease_duration } => lease_duration,
        })
        .min();
      if let Some(&dur) = min_manual_participant {
        if current_duration > dur {
          let pp = ParticipantMessageData {
            guid: self.domain_participant.get_guid_prefix(),
            kind:
              ParticipantMessageDataKind::PARTICIPANT_MESSAGE_DATA_KIND_MANUAL_LIVELINESS_UPDATE,
            data: Vec::new(),
          };
          match self.dcps_participant_message_writer.write(pp, None) {
            Ok(_) => (),
            Err(e) => {
              error!("Failed to writer ParticipantMessageData manual. {:?}", e);
            }
          }
        }
      };
    }
  }

  pub fn participant_cleanup(&self) {
    let removed_guid_prefixes = 
      self.discovery_db_write().participant_cleanup();
    for guid_prefix in removed_guid_prefixes {
      debug!("participant cleanup - timeout for {:?}", guid_prefix);
      self.send_discovery_notification(
                DiscoveryNotificationType::ParticipantLost { guid_prefix });
    }
  }

  pub fn topic_cleanup(&self) {
    self.discovery_db_write().topic_cleanup();
  }

  pub fn read_readers_info(&self) -> bool {
    let readers_info_updated = self.discovery_db_read().is_readers_updated();

    if readers_info_updated {
      self.discovery_db_write().readers_updated(false);
    }

    readers_info_updated
  }

  pub fn read_writers_info(&self) -> bool {
    let writers_info_updated = self.discovery_db_read().is_writers_updated();

    if writers_info_updated {
      self.discovery_db_write().writers_updated(false);
    }

    writers_info_updated
  }

  pub fn write_readers_info(&self) {
    let db = self.discovery_db_read();
    let datas = db.get_all_local_topic_readers();
    for data in datas
      // filtering out discoveries own readers
      .filter(|p| {
        let eid = p.reader_proxy.remote_reader_guid.entityId;
        eid != EntityId::ENTITYID_SPDP_BUILTIN_PARTICIPANT_READER
          && eid != EntityId::ENTITYID_SEDP_BUILTIN_SUBSCRIPTIONS_READER
          && eid != EntityId::ENTITYID_SEDP_BUILTIN_PUBLICATIONS_READER
          && eid != EntityId::ENTITYID_SEDP_BUILTIN_TOPIC_READER
          && eid != EntityId::ENTITYID_P2P_BUILTIN_PARTICIPANT_MESSAGE_READER
      })
    {
      match self.dcps_subscription_writer.write(data.clone(), None) {
        Ok(_) => (),
        Err(e) => error!("Unable to write new readers info. {:?}", e),
      }
    }
  }

  pub fn write_writers_info(&self) {
    let db = self.discovery_db_read();
    let datas = db.get_all_local_topic_writers();
    for data in datas.filter(|p| {
      let eid = p.writer_proxy.remote_writer_guid.entityId;

      eid != EntityId::ENTITYID_SPDP_BUILTIN_PARTICIPANT_WRITER
        && eid != EntityId::ENTITYID_SEDP_BUILTIN_SUBSCRIPTIONS_WRITER
        && eid != EntityId::ENTITYID_SEDP_BUILTIN_PUBLICATIONS_WRITER
        && eid != EntityId::ENTITYID_SEDP_BUILTIN_TOPIC_WRITER
        && eid != EntityId::ENTITYID_P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER
    }) {
      match self.dcps_publication_writer.write(data.clone(), None) {
        Ok(_) => (),
        _ => error!("Unable to write new readers info."),
      }
    }
  }

  pub fn write_topic_info(&self) {
    let db = self.discovery_db_read();
    let datas = db.get_all_topics();
    for data in datas {
      match self.dcps_topic_writer.write(data.clone(), None) {
        Ok(_) => (),
        Err(e) => error!("Unable to write new topic info: {:?}",e),
      }
    }
  }

  pub fn subscriber_qos() -> QosPolicies {
    QosPolicyBuilder::new()
      .durability(Durability::TransientLocal)
      .presentation(Presentation {
        access_scope: PresentationAccessScope::Topic,
        coherent_access: false,
        ordered_access: false,
      })
      .deadline(Deadline(Duration::DURATION_INFINITE))
      .ownership(Ownership::Shared)
      .liveliness(Liveliness::Automatic {
        lease_duration: Duration::DURATION_INFINITE,
      })
      .time_based_filter(TimeBasedFilter {
        minimum_separation: Duration::DURATION_ZERO,
      })
      .reliability(Reliability::Reliable {
        max_blocking_time: Duration::from_std(StdDuration::from_millis(100)),
      })
      .destination_order(DestinationOrder::ByReceptionTimestamp)
      .history(History::KeepLast { depth: 10 }) 
      // .resource_limits(ResourceLimits { // TODO: Maybe lower limits would suffice?
      //   max_instances: std::i32::MAX,
      //   max_samples: std::i32::MAX,
      //   max_samples_per_instance: std::i32::MAX,
      // })
      .build()
  }

  // TODO: Check if this definition is correct (spec?)
  pub fn publisher_qos() -> QosPolicies {
    QosPolicyBuilder::new()
      .durability(Durability::TransientLocal)
      .presentation(Presentation {
        access_scope: PresentationAccessScope::Topic,
        coherent_access: false,
        ordered_access: false,
      })
      .deadline(Deadline(Duration::DURATION_INFINITE))
      .ownership(Ownership::Shared)
      .liveliness(Liveliness::Automatic {
        lease_duration: Duration::DURATION_INFINITE,
      })
      .time_based_filter(TimeBasedFilter {
        minimum_separation: Duration::DURATION_ZERO,
      })
      .reliability(Reliability::Reliable {
        max_blocking_time: Duration::from_std(StdDuration::from_millis(100)),
      })
      .destination_order(DestinationOrder::ByReceptionTimestamp)
      .history(History::KeepLast { depth: 10 }) 
      // .resource_limits(ResourceLimits { // TODO: Maybe lower limits would suffice?
      //   max_instances: std::i32::MAX,
      //   max_samples: std::i32::MAX,
      //   max_samples_per_instance: std::i32::MAX,
      // })
      .build()
  }

  pub fn create_spdp_patricipant_qos() -> QosPolicies {
    QosPolicyBuilder::new()
      .reliability(Reliability::BestEffort)
      .history(History::KeepLast { depth: 1 })
      .build()
  }


  fn discovery_db_read(&self) -> RwLockReadGuard<DiscoveryDB> {
    match self.discovery_db.read() {
      Ok(db) => db,
      Err(e) => panic!("DiscoveryDB is poisoned {:?}.", e),
    }
  }

  fn discovery_db_write(&self) -> RwLockWriteGuard<DiscoveryDB> {
    match self.discovery_db.write() {
      Ok(db) => db,
      Err(e) => panic!("DiscoveryDB is poisoned {:?}.", e),
    }
  }

  fn send_discovery_notification(&self, dntype: DiscoveryNotificationType) {
    match self.discovery_updated_sender.send(dntype) {
      Ok(_) => (),
      Err(e) => error!("Failed to send DiscoveryNotification {:?}", e),
    }
  }
}


// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------
// -----------------------------------------------------------------------


#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    test::{
      shape_type::ShapeType,
      test_data::{
        spdp_subscription_msg, spdp_publication_msg, spdp_participant_msg_mod,
        create_rtps_data_message,
      },
    },
    network::{udp_listener::UDPListener, udp_sender::UDPSender},
    structure::{entity::RTPSEntity, locator::Locator},
    serialization::{cdr_serializer::to_bytes, cdr_deserializer::CDRDeserializerAdapter},
    messages::submessages::submessages::{InterpreterSubmessage, EntitySubmessage},
    messages::{
      submessages::submessage_elements::serialized_payload::{RepresentationIdentifier},
    },
  };
  use crate::{
    discovery::data_types::topic_data::TopicBuiltinTopicData,
    dds::{participant::DomainParticipant, traits::serde_adapters::no_key::DeserializerAdapter},
  };
  use crate::serialization::submessage::*;

  use std::{net::SocketAddr};
  use bytes::Bytes;
    use mio::Token;
  use speedy::{Writable, Endianness};
  

  #[test]
  fn discovery_participant_data_test() {
    let poll = Poll::new().unwrap();
    let mut udp_listener = UDPListener::new_unicast(Token(0), "127.0.0.1", 11000).unwrap();
    poll
      .register(
        udp_listener.mio_socket(),
        Token(0),
        Ready::readable(),
        PollOpt::edge(),
      )
      .unwrap();

    // sending participant data to discovery
    let udp_sender = UDPSender::new_with_random_port().expect("failed to create UDPSender");
    let addresses = vec![SocketAddr::new(
      "127.0.0.1".parse().unwrap(),
      get_spdp_well_known_unicast_port(0, 0),
    )];

    let tdata = spdp_participant_msg_mod(11000);
    let msg_data = tdata
      .write_to_vec_with_ctx(Endianness::LittleEndian)
      .expect("Failed to write msg data");

    udp_sender.send_to_all(&msg_data, &addresses);

    let mut events = Events::with_capacity(10);
    poll
      .poll(&mut events, Some(StdDuration::from_secs(1)))
      .unwrap();

    let _data2 = udp_listener.get_message();
    // TODO: we should have received our own participants info decoding the actual message might be good idea
  }

  #[test]
  fn discovery_reader_data_test() {
    let participant = DomainParticipant::new(0).expect("participant creation");

    let topic = participant
      .create_topic(
        "Square".to_string(),
        "ShapeType".to_string(),
        &QosPolicies::qos_none(),
        TopicKind::WithKey,
      )
      .unwrap();

    let publisher = participant
      .create_publisher(&QosPolicies::qos_none())
      .unwrap();
    let _writer = publisher
      .create_datawriter_CDR::<ShapeType>(topic.clone(), None,).unwrap();

    let subscriber = participant
      .create_subscriber(&QosPolicies::qos_none())
      .unwrap();
    let _reader = subscriber
      .create_datareader::<ShapeType, CDRDeserializerAdapter<ShapeType>>(topic, None);

    let poll = Poll::new().unwrap();
    let mut udp_listener = UDPListener::new_unicast(Token(0), "127.0.0.1", 11001).unwrap();
    poll
      .register(
        udp_listener.mio_socket(),
        Token(0),
        Ready::readable(),
        PollOpt::edge(),
      )
      .unwrap();

    let udp_sender = UDPSender::new_with_random_port().expect("failed to create UDPSender");
    let addresses = vec![SocketAddr::new(
      "127.0.0.1".parse().unwrap(),
      get_spdp_well_known_unicast_port(14, 0),
    )];

    let mut tdata = spdp_subscription_msg();
    let mut data;
    for submsg in tdata.submessages.iter_mut() {
      match &mut submsg.body {
        SubmessageBody::Entity(v) => match v {
          EntitySubmessage::Data(d, _) => {
            let mut drd: DiscoveredReaderData = PlCdrDeserializerAdapter::from_bytes(
              &d.serialized_payload.as_ref().unwrap().value,
              RepresentationIdentifier::PL_CDR_LE,
            )
            .unwrap();
            drd.reader_proxy.unicast_locator_list.clear();
            drd
              .reader_proxy
              .unicast_locator_list
              .push(Locator::from(SocketAddr::new(
                "127.0.0.1".parse().unwrap(),
                11001,
              )));
            drd.reader_proxy.multicast_locator_list.clear();

            data = Bytes::from(to_bytes::<DiscoveredReaderData, byteorder::LittleEndian>(&drd).unwrap());
            d.serialized_payload.as_mut().unwrap().value = data.clone();
          }
          _ => continue,
        },
        SubmessageBody::Interpreter(_) => (),
      }
    }

    let msg_data = tdata
      .write_to_vec_with_ctx(Endianness::LittleEndian)
      .expect("Failed to write msg dtaa");

    udp_sender.send_to_all(&msg_data, &addresses);

    let mut events = Events::with_capacity(10);
    poll
      .poll(&mut events, Some(StdDuration::from_secs(1)))
      .unwrap();

    let _data2 = udp_listener.get_message();
  }

  #[test]
  fn discovery_writer_data_test() {
    let participant = DomainParticipant::new(0).expect("Failed to create participant");

    let topic = participant
      .create_topic(
        "Square".to_string(),
        "ShapeType".to_string(),
        &QosPolicies::qos_none(),
        TopicKind::WithKey,
      )
      .unwrap();

    let publisher = participant
      .create_publisher(&QosPolicies::qos_none())
      .unwrap();
    let _writer = publisher
      .create_datawriter_CDR::<ShapeType>(topic.clone(), None,).unwrap();

    let subscriber = participant
      .create_subscriber(&QosPolicies::qos_none())
      .unwrap();
    let _reader = subscriber
      .create_datareader::<ShapeType, CDRDeserializerAdapter<ShapeType>>(topic, None);

    let poll = Poll::new().unwrap();
    let mut udp_listener = UDPListener::new_unicast(Token(0), "127.0.0.1", 0).unwrap();
    poll
      .register(
        udp_listener.mio_socket(),
        Token(0),
        Ready::readable(),
        PollOpt::edge(),
      )
      .unwrap();

    let udp_sender = UDPSender::new_with_random_port().expect("failed to create UDPSender");
    let addresses = vec![SocketAddr::new(
      "127.0.0.1".parse().unwrap(),
      get_spdp_well_known_unicast_port(15, 0),
    )];

    let mut tdata = spdp_publication_msg();
    for submsg in tdata.submessages.iter_mut() {
      match &mut submsg.body {
        SubmessageBody::Interpreter(v) => match v {
          InterpreterSubmessage::InfoDestination(dst, _flags) => {
            dst.guid_prefix = participant.get_guid_prefix();
          }
          _ => continue,
        },
        SubmessageBody::Entity(_) => (),
      }
    }

    let par_msg_data = spdp_participant_msg_mod(udp_listener.port())
      .write_to_vec_with_ctx(Endianness::LittleEndian)
      .expect("Failed to write participant data.");

    let msg_data = tdata
      .write_to_vec_with_ctx(Endianness::LittleEndian)
      .expect("Failed to write msg data");

    udp_sender.send_to_all(&par_msg_data, &addresses);
    udp_sender.send_to_all(&msg_data, &addresses);

    let mut events = Events::with_capacity(10);
    poll
      .poll(&mut events, Some(StdDuration::from_secs(1)))
      .unwrap();

    for _ in udp_listener.get_messages() {
      info!("Message received");
    }
  }

  #[test]
  fn discovery_topic_data_test() {
    let _participant = DomainParticipant::new(0);

    let topic_data = DiscoveredTopicData::new(TopicBuiltinTopicData {
      key: None,
      name: String::from("Square"),
      type_name: String::from("ShapeType"),
      durability: None,
      deadline: None,
      latency_budget: None,
      liveliness: None,
      reliability: None,
      lifespan: None,
      destination_order: None,
      presentation: None,
      history: None,
      resource_limits: None,
      ownership: None,
    });

    let rtps_message = create_rtps_data_message(
      topic_data,
      EntityId::ENTITYID_SEDP_BUILTIN_TOPIC_READER,
      EntityId::ENTITYID_SEDP_BUILTIN_TOPIC_WRITER,
    );

    let udp_sender = UDPSender::new_with_random_port().expect("failed to create UDPSender");
    let addresses = vec![SocketAddr::new(
      "127.0.0.1".parse().unwrap(),
      get_spdp_well_known_unicast_port(16, 0),
    )];

    let rr = rtps_message
      .write_to_vec_with_ctx(Endianness::LittleEndian)
      .unwrap();

    udp_sender.send_to_all(&rr, &addresses);
  }
}
