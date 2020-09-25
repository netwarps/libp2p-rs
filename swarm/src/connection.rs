use crate::{Multiaddr, PeerId, SwarmError, SwarmEvent, ping, open_stream};
use async_std::task::JoinHandle;
use smallvec::SmallVec;
use std::hash::Hash;
use std::{error::Error, fmt, io};
use std::time::Duration;
use futures::channel::mpsc;
use futures::prelude::*;
use async_std::task;
use libp2p_traits::{Read2, Write2};
use libp2p_core::identity::Keypair;
use libp2p_core::muxing::StreamMuxer;
use libp2p_core::secure_io::SecureInfo;
use libp2p_core::transport::TransportError;
use libp2p_core::PublicKey;
use crate::substream::StreamId;
use crate::ping::PING_PROTOCOL;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The direction of a peer-to-peer communication channel.
#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    /// The socket comes from a dialer.
    Outbound,
    /// The socket comes from a listener.
    Inbound,
}

/// Event generated by a [`Connection`].
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Event<T> {
    /// Event generated by the [`ConnectionHandler`].
    Handler(T),
    /// Address of the remote has changed.
    AddressChange(Multiaddr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(usize);

/// A multiplexed connection to a peer with associated `Substream`s.
#[allow(dead_code)]
pub struct Connection<TMuxer: StreamMuxer> {
    /// The unique ID for a connection
    id: ConnectionId,
    /// Node that handles the stream_muxer.
    stream_muxer: TMuxer,
    /// Handler that processes substreams.
    //pub(crate) substreams: SmallVec<[TMuxer::Substream; 8]>,
    substreams: SmallVec<[StreamId; 8]>,
    /// Direction of this connection
    dir: Direction,
    /// Indicates if Ping task is running.
    ping_running: Arc<AtomicBool>,
    /// Ping failure count.
    ping_failures: u32,
    /// The max allowed Ping failure.
    max_ping_failures: NonZeroU32,
    /// Identity service
    identity: Option<()>,    
    /// The task handle of this connection, returned by task::Spawn
    /// handle.await() when closing a connection
    handle: Option<JoinHandle<()>>,
    /// The task handle of the Ping service of this connection
    ping_handle: Option<JoinHandle<()>>,
}

impl<TMuxer: StreamMuxer> PartialEq for Connection<TMuxer> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<TMuxer> fmt::Debug for Connection<TMuxer>
where
    TMuxer: StreamMuxer + fmt::Debug,
    TMuxer::Substream: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("id", &self.id)
            .field("remote", &self.remote_peer())
            .field("muxer", &self.stream_muxer)
            .field("dir", &self.dir)
            .field("subs", &self.substreams)
            .finish()
    }
}

//impl<TMuxer> Unpin for Connection<TMuxer> where TMuxer: StreamMuxer {}

#[allow(dead_code)]
impl<TMuxer> Connection<TMuxer>
where
    TMuxer: StreamMuxer + SecureInfo,
{
    /// Builds a new `Connection` from the given substream multiplexer
    /// and connection handler.
    pub fn new(id: usize, stream_muxer: TMuxer, dir: Direction) -> Self {
        Connection {
            id: ConnectionId(id),
            stream_muxer,
            dir,
            ping_running: Arc::new(AtomicBool::new(false)),
            ping_failures: 0,
            substreams: Default::default(),
            handle: None,
            identity: None,
            ping_handle: None,
            max_ping_failures: NonZeroU32::new(3).expect("1 != 0")
        }
    }

    /// Returns the unique Id of the connection.
    pub fn id(&self) -> ConnectionId {
        self.id
    }

    /// Returns a reference of the stream_muxer.
    pub fn stream_muxer(&self) -> &TMuxer {
        &self.stream_muxer
    }

    /// Sets the task handle of the connection.
    pub fn set_handle(&mut self, handle: JoinHandle<()>) {
        self.handle = Some(handle);
    }

    /// Closes the inner stream_muxer. Spawn a task to avoid blocking.
    pub fn close(&self)
        where TMuxer: 'static,
    {
        log::trace!("closing {:?}", self);

        let mut stream_muxer = self.stream_muxer.clone();
        // spawns a task to close the stream_muxer, later connection will cleaned up
        // in 'handle_connection_closed'
        task::spawn(async move {
            let _ = stream_muxer.close().await;
        });
    }

    /// Waits for bg-task & accept-task.
    pub async fn wait(&mut self) -> Result<(), SwarmError> {
        // wait for accept-task and bg-task to exit
        if let Some(h) = self.handle.take() {
            h.await;
        }
        Ok(())
    }
    /// local_addr is the multiaddr on our side of the connection.
    pub fn local_addr(&self) -> Multiaddr {
        self.stream_muxer.local_multiaddr()
    }

    /// remote_addr is the multiaddr on the remote side of the connection.
    pub fn remote_addr(&self) -> Multiaddr {
        self.stream_muxer.remote_multiaddr()
    }

    /// local_peer is the Peer on our side of the connection.
    pub fn local_peer(&self) -> PeerId {
        self.stream_muxer.local_peer()
    }

    /// remote_peer is the Peer on the remote side.
    pub fn remote_peer(&self) -> PeerId {
        self.stream_muxer.remote_peer()
    }

    /// local_priv_key is the public key of the peer on this side.
    pub fn local_priv_key(&self) -> Keypair {
        self.stream_muxer.local_priv_key()
    }

    /// remote_pub_key is the public key of the peer on the remote side.
    pub fn remote_pub_key(&self) -> PublicKey {
        self.stream_muxer.remote_pub_key()
    }

    /// Adds a substream id to the list.
    pub(crate) fn add_stream(&mut self, sid: StreamId) {
        log::trace!("adding sub {:?} to {:?}", sid, self);
        self.substreams.push(sid);
    }
    /// Removes a substream id from the list.
    pub(crate) fn del_stream(&mut self, sid: StreamId) {
        log::trace!("removing sub {:?} from {:?}", sid, self);
        self.substreams.retain(|id| id != &sid);
    }

    /// Returns how many substreams in the list.
    pub(crate) fn num_streams(&self) -> usize {
        self.substreams.len()
    }

    /// Increases failure count, returns the increased count.
    pub(crate) fn inc_failure(&mut self) -> u32 {
        self.ping_failures += 1;
        self.ping_failures
    }

    /// Increases failure count, returns the increased count.
    pub(crate) fn reset_failure(&mut self) {
        self.ping_failures = 0;
    }

    ///
    pub(crate) fn start_ping(&mut self, timeout: Duration, interval: Duration, max_failures: NonZeroU32,
                             tx: mpsc::UnboundedSender<SwarmEvent<TMuxer>>)
    where
        TMuxer: 'static,
        TMuxer::Substream: Read2 + Write2 + Send + Unpin,
    {
        self.max_ping_failures = max_failures;

        self.ping_running.store(true, Ordering::Relaxed);

        let cid = self.id();
        let stream_muxer = self.stream_muxer.clone();
        let tx = tx.clone();
        let flag = self.ping_running.clone();
        let pids = vec![PING_PROTOCOL];

        let handle = task::spawn(async move {
            let mut tx2 = tx.clone();

            loop {
                if !flag.load(Ordering::Relaxed) {
                    log::error!("break");
                    break;
                }

                // sleep for the interval
                task::sleep(interval).await;

                let stream_muxer = stream_muxer.clone();
                let pids = pids.clone();
                let tx = tx.clone();

                let r = open_stream(cid, stream_muxer, pids, tx).await;
                let r = match r {
                    Ok(stream) => {
                        let cid = stream.cid();
                        let sid = stream.id();

                        let r = ping::ping(stream, timeout).await;
                        // generate a StreamClosed event so that substream can be removed from Connection
                        let _ = tx2
                            .send(SwarmEvent::StreamClosed {
                                dir: Direction::Outbound,
                                cid,
                                sid,
                            })
                            .await;
                        r.map_err(|e| e.into())
                    }
                    Err(err) => {
                        // looks like the peer doesn't support the protocol
                        log::warn!("Ping protocol not supported: {:?}", err);
                        Err(err)
                    }
                };
                let _ = tx2.send(SwarmEvent::PingResult { cid, result: r }).await;
            }

            log::trace!("ping task exiting...");
        });

        self.ping_handle = Some(handle);
    }

    pub(crate) async fn stop_ping(&mut self) {
        if let Some(h) =  self.ping_handle.take() {
            log::error!("store");
            self.ping_running.store(false, Ordering::Relaxed);
            log::error!("cancel");
            //h.cancel().await;
        }
    }
}

/// Information about a connection limit.
#[derive(Debug, Clone)]
pub struct ConnectionLimit {
    /// The maximum number of connections.
    pub limit: usize,
    /// The current number of connections.
    pub current: usize,
}

impl fmt::Display for ConnectionLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.current, self.limit)
    }
}

/// A `ConnectionLimit` can represent an error if it has been exceeded.
impl Error for ConnectionLimit {}

/// Errors that can occur in the context of an established `Connection`.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ConnectionError<THandlerErr> {
    /// An I/O error occurred on the connection.
    // TODO: Eventually this should also be a custom error?
    IO(io::Error),

    /// The connection handler produced an error.
    Handler(THandlerErr),
}

impl<THandlerErr> fmt::Display for ConnectionError<THandlerErr>
where
    THandlerErr: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionError::IO(err) => write!(f, "Connection error: I/O error: {}", err),
            ConnectionError::Handler(err) => write!(f, "Connection error: Handler error: {}", err),
        }
    }
}

impl<THandlerErr> std::error::Error for ConnectionError<THandlerErr>
where
    THandlerErr: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConnectionError::IO(err) => Some(err),
            ConnectionError::Handler(err) => Some(err),
        }
    }
}

/// Errors that can occur in the context of a pending `Connection`.
#[derive(Debug)]
#[allow(dead_code)]
pub enum PendingConnectionError {
    /// An error occurred while negotiating the transport protocol(s).
    Transport(TransportError),

    /// The peer identity obtained on the connection did not
    /// match the one that was expected or is otherwise invalid.
    InvalidPeerId,

    /// The connection was dropped because the connection limit
    /// for a peer has been reached.
    ConnectionLimit(ConnectionLimit),

    /// An I/O error occurred on the connection.
    // TODO: Eventually this should also be a custom error?
    IO(io::Error),
}

impl fmt::Display for PendingConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PendingConnectionError::IO(err) => write!(f, "Pending connection: I/O error: {}", err),
            PendingConnectionError::Transport(err) => {
                write!(f, "Pending connection: Transport error: {}", err)
            }
            PendingConnectionError::InvalidPeerId => {
                write!(f, "Pending connection: Invalid peer ID.")
            }
            PendingConnectionError::ConnectionLimit(l) => {
                write!(f, "Connection error: Connection limit: {}.", l)
            }
        }
    }
}

impl std::error::Error for PendingConnectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PendingConnectionError::IO(err) => Some(err),
            PendingConnectionError::Transport(err) => Some(err),
            PendingConnectionError::InvalidPeerId => None,
            PendingConnectionError::ConnectionLimit(..) => None,
        }
    }
}
