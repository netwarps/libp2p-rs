use crate::identify::{IDENTIFY_PROTOCOL, IDENTIFY_PUSH_PROTOCOL, IdentifyInfo};
use crate::ping::PING_PROTOCOL;
use crate::substream::{StreamId, Substream};
use crate::{identify, ping, Multiaddr, PeerId, ProtocolId, SwarmError, SwarmEvent};
use async_std::task;
use async_std::task::JoinHandle;
use futures::channel::mpsc;
use futures::prelude::*;
use libp2p_core::identity::Keypair;
use libp2p_core::multistream::Negotiator;
use libp2p_core::muxing::StreamMuxer;
use libp2p_core::secure_io::SecureInfo;
use libp2p_core::transport::TransportError;
use libp2p_core::upgrade::ProtocolName;
use libp2p_core::PublicKey;
use libp2p_traits::{ReadEx, WriteEx};
use smallvec::SmallVec;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{error::Error, fmt};

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

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(usize);

/// A multiplexed connection to a peer with associated `Substream`s.
#[allow(dead_code)]
pub struct Connection<TMuxer: StreamMuxer> {
    /// The unique ID for a connection
    id: ConnectionId,
    /// Node that handles the stream_muxer.
    stream_muxer: TMuxer,
    /// The tx channel, to send Connection events to Swarm
    tx: mpsc::UnboundedSender<SwarmEvent<TMuxer>>,
    /// Handler that processes substreams.
    //pub(crate) substreams: SmallVec<[TMuxer::Substream; 8]>,
    substreams: SmallVec<[StreamId; 8]>,
    /// Direction of this connection
    dir: Direction,
    /// Indicates if Ping task is running.
    ping_running: Arc<AtomicBool>,
    /// Ping failure count.
    ping_failures: u32,
    /// Identity service
    identity: Option<()>,
    /// The task handle of this connection, returned by task::Spawn
    /// handle.await() when closing a connection
    handle: Option<JoinHandle<()>>,
    /// The task handle of the Ping service of this connection
    ping_handle: Option<JoinHandle<()>>,
    /// The task handle of the Identify service of this connection
    identify_handle: Option<JoinHandle<()>>,
    /// The task handle of the Identify Push service of this connection
    identify_push_handle: Option<JoinHandle<()>>,
}

impl<TMuxer: StreamMuxer> PartialEq for Connection<TMuxer> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<TMuxer> fmt::Debug for Connection<TMuxer>
where
    TMuxer: StreamMuxer + SecureInfo + 'static,
    TMuxer::Substream: ReadEx + WriteEx + Send + Unpin,
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
    TMuxer: StreamMuxer + SecureInfo + 'static,
    TMuxer::Substream: ReadEx + WriteEx + Send + Unpin,
{
    /// Builds a new `Connection` from the given substream multiplexer
    /// and a tx channel which will used to send events to Swarm.
    pub(crate) fn new(id: usize, stream_muxer: TMuxer, dir: Direction, tx: mpsc::UnboundedSender<SwarmEvent<TMuxer>>) -> Self {
        Connection {
            id: ConnectionId(id),
            stream_muxer,
            tx,
            dir,
            substreams: Default::default(),
            handle: None,
            ping_running: Arc::new(AtomicBool::new(false)),
            ping_failures: 0,
            ping_handle: None,
            identity: None,
            identify_handle: None,
            identify_push_handle: None
        }
    }

    /// Returns the unique Id of the connection.
    pub(crate) fn id(&self) -> ConnectionId {
        self.id
    }

    /// Returns a reference of the stream_muxer.
    pub(crate) fn stream_muxer(&self) -> &TMuxer {
        &self.stream_muxer
    }

    /// Sets the task handle of the connection.
    pub(crate) fn set_handle(&mut self, handle: JoinHandle<()>) {
        self.handle = Some(handle);
    }

    /// Opens a sub stream with the protocols specified
    pub(crate) fn open_stream<T: Send + 'static>(
        &mut self,
        pids: Vec<ProtocolId>,
        f: impl FnOnce(Result<Substream<TMuxer::Substream>, TransportError>) -> T + Send + 'static,
    ) -> JoinHandle<T> {
        let cid = self.id();
        let stream_muxer = self.stream_muxer().clone();
        let tx = self.tx.clone();

        task::spawn(async move {
            let result = open_stream_internal(cid, stream_muxer, pids, tx).await;
            f(result)
        })
    }

    /// Closes the inner stream_muxer. Spawn a task to avoid blocking.
    pub(crate) fn close(&self) {
        log::trace!("closing {:?}", self);

        let mut stream_muxer = self.stream_muxer.clone();
        // spawns a task to close the stream_muxer, later connection will cleaned up
        // in 'handle_connection_closed'
        task::spawn(async move {
            let _ = stream_muxer.close().await;
        });
    }

    /// Waits for bg-task & accept-task.
    pub(crate) async fn wait(&mut self) -> Result<(), SwarmError> {
        // wait for accept-task and bg-task to exit
        if let Some(h) = self.handle.take() {
            h.await;
        }
        Ok(())
    }
    /// local_addr is the multiaddr on our side of the connection.
    pub(crate) fn local_addr(&self) -> Multiaddr {
        self.stream_muxer.local_multiaddr()
    }

    /// remote_addr is the multiaddr on the remote side of the connection.
    pub(crate) fn remote_addr(&self) -> Multiaddr {
        self.stream_muxer.remote_multiaddr()
    }

    /// local_peer is the Peer on our side of the connection.
    pub(crate) fn local_peer(&self) -> PeerId {
        self.stream_muxer.local_peer()
    }

    /// remote_peer is the Peer on the remote side.
    pub(crate) fn remote_peer(&self) -> PeerId {
        self.stream_muxer.remote_peer()
    }

    /// local_priv_key is the public key of the peer on this side.
    pub(crate) fn local_priv_key(&self) -> Keypair {
        self.stream_muxer.local_priv_key()
    }

    /// remote_pub_key is the public key of the peer on the remote side.
    pub(crate) fn remote_pub_key(&self) -> PublicKey {
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
    pub(crate) fn handle_failure(&mut self, allowed_max_failures: u32) {
        self.ping_failures += 1;
        if self.ping_failures >= allowed_max_failures {
            // close the connection
            log::info!("reach the max ping failure count, closing {:?}", self);
            self.close();
        }
    }

    /// Increases failure count, returns the increased count.
    pub(crate) fn reset_failure(&mut self) {
        self.ping_failures = 0;
    }

    /// Starts the Ping service on this connection. The task handle will be tracked
    /// by the connection for later closing the Ping service
    pub(crate) fn start_ping(&mut self, timeout: Duration, interval: Duration)
    {
        self.ping_running.store(true, Ordering::Relaxed);

        let cid = self.id();
        let stream_muxer = self.stream_muxer.clone();
        let tx = self.tx.clone();
        let flag = self.ping_running.clone();
        let pids = vec![PING_PROTOCOL];

        let handle = task::spawn(async move {
            let mut tx2 = tx.clone();

            loop {
                if !flag.load(Ordering::Relaxed) {
                    break;
                }

                // sleep for the interval
                task::sleep(interval).await;

                let stream_muxer = stream_muxer.clone();
                let pids = pids.clone();
                let tx = tx.clone();

                let r = open_stream_internal(cid, stream_muxer, pids, tx).await;
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
                let _ = tx2.send(SwarmEvent::PingResult { cid, result: r.map_err(|e|e.into()) }).await;
            }

            log::trace!("ping task exiting...");
        });

        self.ping_handle = Some(handle);
    }

    pub(crate) async fn stop_ping(&mut self) {
        if let Some(h) =  self.ping_handle.take() {
            log::debug!("stopping Ping service...");
            self.ping_running.store(false, Ordering::Relaxed);
            h.await;
            //h.cancel().await;
        }
    }

    /// Starts the Identify service on this connection.
    pub(crate) fn start_identify(&mut self)
    {
        let cid = self.id();
        let stream_muxer = self.stream_muxer.clone();
        let tx = self.tx.clone();
        let pids = vec![IDENTIFY_PROTOCOL];

        let mut tx2 = tx.clone();

        let handle = task::spawn(async move {

            let r = open_stream_internal(cid, stream_muxer, pids, tx).await;
            let r = match r {
                Ok(stream) => {
                    let cid = stream.cid();
                    let sid = stream.id();

                    // generate a StreamClosed event so that substream can be removed from Connection
                    let _ = tx2
                        .send(SwarmEvent::StreamClosed {
                            dir: Direction::Outbound,
                            cid,
                            sid,
                        })
                        .await;

                    identify::consume_message(stream).await
                }
                Err(err) => {
                    // looks like the peer doesn't support the protocol
                    log::warn!("Identify protocol not supported: {:?}", err);
                    Err(err)
                }
            };
            let _ = tx2.send(SwarmEvent::IdentifyResult { cid, result: r.map_err(|e|e.into()) }).await;

            log::trace!("identify task exiting...");
        });

        self.identify_handle = Some(handle);
    }

    pub(crate) async fn stop_identify(&mut self) {
        if let Some(h) =  self.identify_handle.take() {
            log::debug!("stopping Identify service...");
            h.cancel().await;
        }
    }


    /// Starts the Identify service on this connection.
    pub(crate) fn start_identify_push(&mut self)
    {
        let cid = self.id();
        let stream_muxer = self.stream_muxer.clone();
        let tx = self.tx.clone();
        let pids = vec![IDENTIFY_PUSH_PROTOCOL];

        let mut tx2 = tx.clone();

        let info = IdentifyInfo {
            public_key: Keypair::generate_ed25519_fixed().public(),
            protocol_version: "".to_string(),
            agent_version: "".to_string(),
            listen_addrs: vec![],
            protocols: vec!["/p1".to_string(), "/p1".to_string()],
        };

        let handle = task::spawn(async move {

            let r = open_stream_internal(cid, stream_muxer, pids, tx).await;
            match r {
                Ok(stream) => {
                    let cid = stream.cid();
                    let sid = stream.id();
                    // generate a StreamClosed event so that substream can be removed from Connection
                    let _ = tx2
                        .send(SwarmEvent::StreamClosed {
                            dir: Direction::Outbound,
                            cid,
                            sid,
                        })
                        .await;
                    // ignore the error
                    let _ = identify::produce_message(stream, info).await;
                }
                Err(err) => {
                    // looks like the peer doesn't support the protocol
                    log::warn!("Identify protocol not supported: {:?}", err);
                    //Err(err)
                }
            }

            log::trace!("identify push task exiting...");
        });

        self.identify_push_handle = Some(handle);
    }
    pub(crate) async fn stop_identify_push(&mut self) {
        if let Some(h) =  self.identify_push_handle.take() {
            log::debug!("stopping Identify Push service...");
            h.cancel().await;
        }
    }
}


async fn open_stream_internal<T>(
    cid: ConnectionId,
    mut stream_muxer: T,
    pids: Vec<ProtocolId>,
    mut tx: mpsc::UnboundedSender<SwarmEvent<T>>,
) -> Result<Substream<T::Substream>, TransportError>
    where
        T: StreamMuxer,
        T::Substream: ReadEx + WriteEx + Send + Unpin,
{
    let raw_stream = stream_muxer.open_stream().await?;

    // now it's time to do protocol multiplexing for sub stream
    let negotiator = Negotiator::new_with_protocols(pids);
    let result = negotiator.select_one(raw_stream).await;

    match result {
        Ok((proto, raw_stream)) => {
            log::info!("select_outbound {:?}", proto.protocol_name_str());

            let stream = Substream::new(raw_stream, Direction::Outbound, proto, cid);

            let sid = stream.id();
            let _ = tx
                .send(SwarmEvent::StreamOpened {
                    dir: Direction::Outbound,
                    cid,
                    sid,
                })
                .await;
            Ok(stream)
        }
        Err(err) => {
            let _ = tx
                .send(SwarmEvent::StreamError {
                    cid,
                    error: TransportError::Internal,
                })
                .await;
            Err(TransportError::NegotiationError(err))
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
