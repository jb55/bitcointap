use crate::{
    AddrmanEvent, AddrmanMsg, ConnectionEvent, ConnectionMsg, Event, EventMsg, MempoolEvent,
    MempoolMsg, RuntimeError, TRACEPOINTS_ADDRMAN, TRACEPOINTS_MEMPOOL, TRACEPOINTS_NET_CONN,
    TRACEPOINTS_NET_MESSAGE, TRACEPOINTS_VALIDATION, ValidationEvent, ValidationMsg, tracing,
};
use libbpf_rs::skel::{OpenSkel, Skel, SkelBuilder};
use libbpf_rs::{Map, MapCore, Object, ProgramMut, RingBufferBuilder};
use shared::ctypes::{
    AddrmanInsertNew, AddrmanInsertTried, ClosedConnection, InboundConnection, MempoolAdded,
    MempoolRejected, MempoolRemoved, MempoolReplaced, MisbehavingConnection, OutboundConnection,
    P2PMessage, ValidationBlockConnected,
};
use shared::log::{self};
//use shared::simple_logger;
use shared::{addrman, mempool, net_msg};
use std::fs::File;
use std::io::{BufReader, Read};
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use std::time::SystemTime;

const RINGBUFF_CALLBACK_OK: i32 = 0;
const RINGBUFF_CALLBACK_SYSTEM_TIME_ERROR: i32 = -5;
const RINGBUFF_CALLBACK_PUBLISH_ERROR: i32 = -10;
const RINGBUFF_CALLBACK_UNABLE_TO_PARSE_P2P_MSG: i32 = -20;

const NO_EVENTS_ERROR_DURATION: Duration = Duration::from_secs(60 * 3);
const NO_EVENTS_WARN_DURATION: Duration = Duration::from_secs(60 * 1);

const DEFAULT_PID: i32 = -1;

pub struct BitcoinTap {
    pid_source: PidSource,

    /// The queue that you read events off of
    rx: Receiver<EventMsg>,

    /// All tapped events are sent over this channel
    tx: Sender<EventMsg>,

    /// Write to this to shut down the ringbuffer polling thread
    quit_tx: Option<Sender<()>>,

    /// Do we have libbpf debugging enabled ?
    debug: bool,

    /// bitcoind_path
    path: PathBuf,
}

/// Used to specify where to source bitcoind's `pid` from
pub enum PidSource {
    Manual(i32),
    File(PathBuf),
    DefaultPid,
}

impl Default for PidSource {
    fn default() -> Self {
        PidSource::DefaultPid
    }
}

impl Drop for BitcoinTap {
    fn drop(&mut self) {
        // send close signal to thread if we have one
        if let Some(quit_tx) = &self.quit_tx {
            let _ = quit_tx.send(());
        }
    }
}

impl BitcoinTap {
    /// Create a new bitcoin tap. Attach by calling [`Self::attach`]
    pub fn new(path: impl AsRef<Path>) -> Self {
        let (tx, rx) = mpsc::channel();
        let path: PathBuf = path.as_ref().to_owned();
        let debug = false;
        let pid_source = PidSource::default();
        let quit_tx = None;

        Self {
            tx,
            rx,
            path,
            debug,
            pid_source,
            quit_tx,
        }
    }

    /// Declare the pid source. This is used on [`Self::attach`]
    pub fn pid_source(mut self, source: PidSource) -> Self {
        self.pid_source = source;
        self
    }

    /// Enable libbpf debugging
    pub fn debug(mut self, enable: bool) -> Self {
        self.debug = enable;
        self
    }

    /// Attach to the process and start reading events
    pub fn attach(&mut self) -> Result<(), RuntimeError> {
        //let args = Args::parse();

        //simple_logger::init_with_level(args.log_level)?;

        let pid = bitcoind_pid(&self.pid_source)?;

        let mut skel_builder = tracing::TracingSkelBuilder::default();
        skel_builder.obj_builder.debug(self.debug);

        let mut uninit = MaybeUninit::uninit();
        log::info!("Opening BPF skeleton with debug={}..", self.debug);
        let open_skel: tracing::OpenTracingSkel = skel_builder.open(&mut uninit)?;
        log::info!("Loading BPF functions and maps into kernel..");
        let skel: tracing::TracingSkel = open_skel.load()?;
        let obj = skel.object();

        let mut active_tracepoints = vec![];
        let mut ringbuff_builder = RingBufferBuilder::new();

        // P2P net msgs tracepoints
        let map_net_msg_small = find_map(&obj, "net_msg_small")?;
        let map_net_msg_medium = find_map(&obj, "net_msg_medium")?;
        let map_net_msg_large = find_map(&obj, "net_msg_large")?;
        let map_net_msg_huge = find_map(&obj, "net_msg_huge")?;

        // net tracepoints
        {
            // TODO: selectively enable these
            active_tracepoints.extend(&TRACEPOINTS_NET_MESSAGE);

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_net_msg_small, move |data| {
                handle_net_message(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_net_msg_medium, move |data| {
                handle_net_message(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_net_msg_large, move |data| {
                handle_net_message(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_net_msg_huge, move |data| {
                handle_net_message(data, &mut tx2)
            })?;
        }

        // P2P connection tracepoints
        let map_net_conn_inbound = find_map(&obj, "net_conn_inbound")?;
        let map_net_conn_outbound = find_map(&obj, "net_conn_outbound")?;
        let map_net_conn_closed = find_map(&obj, "net_conn_closed")?;
        let map_net_conn_inbound_evicted = find_map(&obj, "net_conn_inbound_evicted")?;
        let map_net_conn_misbehaving = find_map(&obj, "net_conn_misbehaving")?;

        // connection tracepoints
        {
            // TODO: select individual connection tracepoints
            active_tracepoints.extend(&TRACEPOINTS_NET_CONN);

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_net_conn_inbound, move |data| {
                handle_net_conn_inbound(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_net_conn_outbound, move |data| {
                handle_net_conn_outbound(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_net_conn_closed, move |data| {
                handle_net_conn_closed(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_net_conn_inbound_evicted, move |data| {
                handle_net_conn_inbound_evicted(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_net_conn_misbehaving, move |data| {
                handle_net_conn_misbehaving(data, &mut tx2)
            })?;
        }

        // validation tracepoints
        let map_validation_block_connected = find_map(&obj, "validation_block_connected")?;
        {
            // TODO: select validation tracepoints
            active_tracepoints.extend(&TRACEPOINTS_VALIDATION);
            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_validation_block_connected, move |data| {
                handle_validation_block_connected(data, &mut tx2)
            })?;
        }

        // mempool tracepoints
        let map_mempool_added = find_map(&obj, "mempool_added")?;
        let map_mempool_removed = find_map(&obj, "mempool_removed")?;
        let map_mempool_rejected = find_map(&obj, "mempool_rejected")?;
        let map_mempool_replaced = find_map(&obj, "mempool_replaced")?;
        {
            // TODO: select mempool tracepoints
            active_tracepoints.extend(&TRACEPOINTS_MEMPOOL);

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_mempool_added, move |data| {
                handle_mempool_added(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_mempool_removed, move |data| {
                handle_mempool_removed(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_mempool_rejected, move |data| {
                handle_mempool_rejected(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_mempool_replaced, move |data| {
                handle_mempool_replaced(data, &mut tx2)
            })?;
        }

        // addrman tracepoints
        let map_addrman_insert_new = find_map(&obj, "addrman_insert_new")?;
        let map_addrman_insert_tried = find_map(&obj, "addrman_insert_tried")?;
        {
            // TODO: select addrman tracepoints
            active_tracepoints.extend(&TRACEPOINTS_ADDRMAN);

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_addrman_insert_new, move |data| {
                handle_addrman_new(data, &mut tx2)
            })?;

            let mut tx2 = self.tx.clone();
            ringbuff_builder.add(&map_addrman_insert_tried, move |data| {
                handle_addrman_tried(data, &mut tx2)
            })?;
        }

        if active_tracepoints.is_empty() {
            log::error!("No tracepoints enabled.");
            return Ok(());
        }

        // attach tracepoints
        let mut _links = Vec::new();
        for tracepoint in active_tracepoints {
            let prog = find_prog_mut(&obj, tracepoint.function)?;
            _links.push(prog.attach_usdt(pid, &self.path, tracepoint.context, tracepoint.name)?);
            log::info!(
                "hooked the BPF script function {} up to the tracepoint {}:{} of '{}' with PID={}",
                tracepoint.function,
                tracepoint.context,
                tracepoint.name,
                &self.path.display(),
                pid
            );
        }

        let ring_buffers = ringbuff_builder.build()?;
        log::info!(
            "Startup successful. Starting to extract events from '{}'..",
            self.path.display()
        );
        let mut last_event_timestamp = SystemTime::now();
        let mut has_warned_about_no_events = false;
        let (quit_tx, quit_rx) = mpsc::channel();
        self.quit_tx = Some(quit_tx);

        std::thread::spawn(move || {
            // TODO: epoll async somehow ?
            loop {
                if let Ok(()) = quit_rx.try_recv() {
                    break;
                }

                match ring_buffers.poll_raw(Duration::from_millis(100)) {
                    RINGBUFF_CALLBACK_OK => (),
                    RINGBUFF_CALLBACK_PUBLISH_ERROR => {
                        log::warn!("Could not publish to event queue.")
                    }
                    RINGBUFF_CALLBACK_UNABLE_TO_PARSE_P2P_MSG => {
                        log::warn!("Could not parse P2P message.")
                    }
                    RINGBUFF_CALLBACK_SYSTEM_TIME_ERROR => log::warn!("SystemTimeError"),
                    _other => {
                        // values >0 are the number of handled events
                        if _other <= 0 {
                            log::warn!("Unhandled ringbuffer callback error: {}", _other)
                        } else {
                            last_event_timestamp = SystemTime::now();
                            has_warned_about_no_events = false;
                            log::trace!(
                                "Extracted {} events from ring buffers and published them",
                                _other
                            );
                        }
                    }
                };
                let duration_since_last_event = SystemTime::now()
                    .duration_since(last_event_timestamp)
                    .expect("time went backwards");
                if duration_since_last_event >= NO_EVENTS_ERROR_DURATION {
                    log::error!(
                        "No events received in the last {:?}.",
                        NO_EVENTS_ERROR_DURATION
                    );
                    log::warn!(
                        "The bitcoind process might be down, has restarted and changed PIDs, or the network might be down."
                    );
                    log::warn!("The extractor will exit. Please restart it");
                } else if duration_since_last_event >= NO_EVENTS_WARN_DURATION
                    && !has_warned_about_no_events
                {
                    has_warned_about_no_events = true;
                    log::warn!(
                        "No events received in the last {:?}. Is bitcoind or the network down?",
                        NO_EVENTS_WARN_DURATION
                    );
                }
            }
        });

        Ok(())
    }
}

fn bitcoind_pid(method: &PidSource) -> Result<i32, RuntimeError> {
    match method {
        PidSource::Manual(pid) => {
            log::info!("Using bitcoind PID={} specified via option", pid);

            Ok(*pid)
        }

        PidSource::File(path) => {
            log::info!(
                "Reading bitcoind PID file '{}' specified via option",
                path.display()
            );

            let file = File::open(&path)?;
            let mut reader = BufReader::new(file);
            let mut content = String::new();
            reader.read_to_string(&mut content)?;
            let pid: i32 = content.trim().parse()?;

            log::info!("Using bitcoind PID={} read from {}", pid, path.display());

            Ok(pid)
        }

        PidSource::DefaultPid => Ok(DEFAULT_PID),
    }
}

fn handle_net_conn_closed(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let closed = ClosedConnection::from_bytes(data);
    tx.send(EventMsg::new(Event::Conn(ConnectionMsg {
        event: Some(ConnectionEvent::Closed(closed.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_net_conn_outbound(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let outbound = OutboundConnection::from_bytes(data);
    tx.send(EventMsg::new(Event::Conn(ConnectionMsg {
        event: Some(ConnectionEvent::Outbound(outbound.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_net_conn_inbound(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let inbound = InboundConnection::from_bytes(data);
    tx.send(EventMsg::new(Event::Conn(ConnectionMsg {
        event: Some(ConnectionEvent::Inbound(inbound.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_net_conn_inbound_evicted(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let evicted = ClosedConnection::from_bytes(data);
    tx.send(EventMsg::new(Event::Conn(ConnectionMsg {
        event: Some(ConnectionEvent::InboundEvicted(evicted.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_net_conn_misbehaving(data: &[u8], tx: &mpsc::Sender<EventMsg>) -> i32 {
    let misbehaving = MisbehavingConnection::from_bytes(data);
    tx.send(EventMsg::new(Event::Conn(ConnectionMsg {
        event: Some(ConnectionEvent::Misbehaving(misbehaving.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_net_message(data: &[u8], tx: &mpsc::Sender<EventMsg>) -> i32 {
    let message = P2PMessage::from_bytes(data);
    let protobuf_message = match message.decode_to_protobuf_network_message() {
        Ok(msg) => msg.into(),
        Err(e) => {
            log::warn!("Could not parse P2P msg with size={}: {}", data.len(), e);
            return RINGBUFF_CALLBACK_UNABLE_TO_PARSE_P2P_MSG;
        }
    };
    tx.send(EventMsg::new(Event::Msg(net_msg::Message {
        meta: message.meta.create_protobuf_metadata(),
        msg: Some(protobuf_message),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_addrman_new(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let new = AddrmanInsertNew::from_bytes(data);
    tx.send(EventMsg::new(Event::Addrman(AddrmanMsg {
        event: Some(AddrmanEvent::New(new.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_addrman_tried(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let tried = AddrmanInsertTried::from_bytes(data);
    tx.send(EventMsg::new(Event::Addrman(addrman::AddrmanEvent {
        event: Some(addrman::addrman_event::Event::Tried(tried.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_mempool_added(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let added = MempoolAdded::from_bytes(data);
    tx.send(EventMsg::new(Event::Mempool(MempoolMsg {
        event: Some(MempoolEvent::Added(added.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_mempool_removed(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let removed = MempoolRemoved::from_bytes(data);
    tx.send(EventMsg::new(Event::Mempool(MempoolMsg {
        event: Some(MempoolEvent::Removed(removed.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_mempool_replaced(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let replaced = MempoolReplaced::from_bytes(data);
    tx.send(EventMsg::new(Event::Mempool(mempool::MempoolEvent {
        event: Some(mempool::mempool_event::Event::Replaced(replaced.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_mempool_rejected(data: &[u8], tx: &mut Sender<EventMsg>) -> i32 {
    let rejected = MempoolRejected::from_bytes(data);
    tx.send(EventMsg::new(Event::Mempool(MempoolMsg {
        event: Some(MempoolEvent::Rejected(rejected.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

fn handle_validation_block_connected(data: &[u8], tx: &mut mpsc::Sender<EventMsg>) -> i32 {
    let connected = ValidationBlockConnected::from_bytes(data);
    tx.send(EventMsg::new(Event::Validation(ValidationMsg {
        event: Some(ValidationEvent::BlockConnected(connected.into())),
    })))
    .map_or_else(
        |_| RINGBUFF_CALLBACK_OK,
        |_| RINGBUFF_CALLBACK_PUBLISH_ERROR,
    )
}

/// Find the BPF program with the given name
pub fn find_prog_mut<'obj>(
    object: &'obj Object,
    name: &str,
) -> Result<ProgramMut<'obj>, RuntimeError> {
    match object.progs_mut().find(|prog| prog.name() == name) {
        Some(prog) => Ok(prog),
        None => Err(RuntimeError::NoSuchBPFProg(name.to_string())),
    }
}

/// Find the BPF map with the given name
pub fn find_map<'obj>(object: &'obj Object, name: &str) -> Result<Map<'obj>, RuntimeError> {
    match object.maps().find(|map| map.name() == name) {
        Some(map) => Ok(map),
        None => Err(RuntimeError::NoSuchBPFMap(name.to_string())),
    }
}
