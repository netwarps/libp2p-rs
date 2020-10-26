// Copyright 2020 Netwarps Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! Communication channel to the remote peer.
//!
//! The Swarm [`Connection`] is the multiplexed connection, which can be used to open or accept
//! new substreams. Furthermore, a raw substream opened by the StreamMuxer has to be upgraded to
//! the Swarm [`Substream`] via multistream select procedure.
//!

use smallvec::SmallVec;
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::{error::Error, fmt};

use futures::channel::mpsc;
use futures::prelude::*;

use async_std::task;
use async_std::task::JoinHandle;

use libp2prs_core::identity::Keypair;
use libp2prs_core::multistream::Negotiator;
use libp2prs_core::muxing::IStreamMuxer;
use libp2prs_core::transport::TransportError;
use libp2prs_core::upgrade::ProtocolName;
use libp2prs_core::PublicKey;

use crate::control::SwarmControlCmd;
use crate::identify::{IdentifyInfo, IDENTIFY_PROTOCOL, IDENTIFY_PUSH_PROTOCOL};
use crate::ping::PING_PROTOCOL;
use crate::substream::{StreamId, Substream};
use crate::{identify, ping, Multiaddr, PeerId, ProtocolId, SwarmError, SwarmEvent};

/// The direction of a peer-to-peer communication channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    /// The socket comes from a dialer.
    Outbound,
    /// The socket comes from a listener.
    Inbound,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(usize);

/// A multiplexed connection to a peer with associated `Substream`s.
#[allow(dead_code)]
pub struct Connection {
    /// The unique ID for a connection
    id: ConnectionId,
    /// Node that handles the stream_muxer.
    stream_muxer: IStreamMuxer,
    /// The tx channel, to send Connection events to Swarm
    tx: mpsc::UnboundedSender<SwarmEvent>,
    /// The ctrl tx channel.
    ctrl: mpsc::Sender<SwarmControlCmd>,
    /// Handler that processes substreams.
    substreams: SmallVec<[Substream; 8]>,
    //substreams: SmallVec<[StreamId; 8]>,
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

impl PartialEq for Connection {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connection")
            .field("id", &self.id)
            .field("muxer", &self.stream_muxer)
            .field("dir", &self.dir)
            .field("subs", &self.substreams)
            .finish()
    }
}

//impl Unpin for Connection where TMuxer: StreamMuxer {}

#[allow(dead_code)]
impl Connection {
    /// Builds a new `Connection` from the given substream multiplexer
    /// and a tx channel which will used to send events to Swarm.
    pub(crate) fn new(
        id: usize,
        stream_muxer: IStreamMuxer,
        dir: Direction,
        tx: mpsc::UnboundedSender<SwarmEvent>,
        ctrl: mpsc::Sender<SwarmControlCmd>,
    ) -> Self {
        Connection {
            id: ConnectionId(id),
            stream_muxer,
            tx,
            ctrl,
            dir,
            substreams: Default::default(),
            handle: None,
            ping_running: Arc::new(AtomicBool::new(false)),
            ping_failures: 0,
            ping_handle: None,
            identity: None,
            identify_handle: None,
            identify_push_handle: None,
        }
    }

    /// Returns the unique Id of the connection.
    pub(crate) fn id(&self) -> ConnectionId {
        self.id
    }

    /// Returns a reference of the stream_muxer.
    pub(crate) fn stream_muxer(&self) -> &IStreamMuxer {
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
        f: impl FnOnce(Result<Substream, TransportError>) -> T + Send + 'static,
    ) -> JoinHandle<T> {
        let cid = self.id();
        let stream_muxer = self.stream_muxer().clone();
        let mut tx = self.tx.clone();
        let ctrl = self.ctrl.clone();

        task::spawn(async move {
            let result = open_stream_internal(cid, stream_muxer, pids, ctrl).await;

            // TODO: how to extract the error from TransportError, ??? it doesn't implement 'Clone'
            // So, at this moment, make a new 'TransportError::Internal'
            let nr = result.as_ref().map(|s| s.clone()).map_err(|_| TransportError::Internal);
            match nr {
                Ok(sub_stream) => {
                    let _ = tx.send(SwarmEvent::StreamOpened { sub_stream }).await;
                }
                Err(err) => {
                    let _ = tx.send(SwarmEvent::StreamError { cid, error: err }).await;
                }
            }

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
    pub(crate) fn add_stream(&mut self, sub_stream: Substream) {
        log::trace!("adding sub {:?} to {:?}", sub_stream, self);
        self.substreams.push(sub_stream);
    }
    /// Removes a substream id from the list.
    pub(crate) fn del_stream(&mut self, sid: StreamId) {
        log::trace!("removing sub {:?} from {:?}", sid, self);
        self.substreams.retain(|s| s.id() != sid);
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
    ///
    /// Note that we don't generate StreamOpened/Closed event for Ping/Identify outbound
    /// simply because it doesn't make much sense doing so for a transient outgoing
    /// stream.
    pub(crate) fn start_ping(&mut self, timeout: Duration, interval: Duration) {
        self.ping_running.store(true, Ordering::Relaxed);

        let cid = self.id();
        let stream_muxer = self.stream_muxer.clone();
        let mut tx = self.tx.clone();
        let flag = self.ping_running.clone();
        let pids = vec![PING_PROTOCOL];
        let ctrl = self.ctrl.clone();

        let handle = task::spawn(async move {
            loop {
                if !flag.load(Ordering::Relaxed) {
                    break;
                }

                // sleep for the interval
                task::sleep(interval).await;

                //recheck, in case ping service has been terminated already
                if !flag.load(Ordering::Relaxed) {
                    break;
                }

                let stream_muxer = stream_muxer.clone();
                let pids = pids.clone();

                let ctrl2 = ctrl.clone();
                let r = open_stream_internal(cid, stream_muxer, pids, ctrl2).await;
                let r = match r {
                    Ok(stream) => {
                        let sub_stream = stream.clone();
                        let _ = tx.send(SwarmEvent::StreamOpened { sub_stream }).await;
                        ping::ping(stream, timeout).await.map_err(|e| e)
                    }
                    Err(err) => {
                        // looks like the peer doesn't support the protocol
                        log::warn!("Ping protocol not supported: {:?}", err);
                        Err(err)
                    }
                };
                let _ = tx
                    .send(SwarmEvent::PingResult {
                        cid,
                        result: r.map_err(|e| e.into()),
                    })
                    .await;
            }

            log::trace!("ping task exiting...");
        });

        self.ping_handle = Some(handle);
    }

    /// Stops the Ping service on this connection
    pub(crate) async fn stop_ping(&mut self) {
        if let Some(h) = self.ping_handle.take() {
            log::debug!("stopping Ping service...");
            self.ping_running.store(false, Ordering::Relaxed);
            h.await;
            //h.cancel().await;
        }
    }

    /// Starts the Identify service on this connection.
    pub(crate) fn start_identify(&mut self) {
        let cid = self.id();
        let stream_muxer = self.stream_muxer.clone();
        let mut tx = self.tx.clone();
        let ctrl = self.ctrl.clone();
        let pids = vec![IDENTIFY_PROTOCOL];

        let handle = task::spawn(async move {
            let r = open_stream_internal(cid, stream_muxer, pids, ctrl).await;
            let r = match r {
                Ok(stream) => {
                    let sub_stream = stream.clone();
                    let _ = tx.send(SwarmEvent::StreamOpened { sub_stream }).await;
                    identify::consume_message(stream).await
                }
                Err(err) => {
                    // looks like the peer doesn't support the protocol
                    log::warn!("Identify protocol not supported: {:?}", err);
                    Err(err)
                }
            };
            let _ = tx
                .send(SwarmEvent::IdentifyResult {
                    cid,
                    result: r.map_err(TransportError::into),
                })
                .await;

            log::trace!("identify task exiting...");
        });

        self.identify_handle = Some(handle);
    }

    pub(crate) async fn stop_identify(&mut self) {
        if let Some(h) = self.identify_handle.take() {
            log::debug!("stopping Identify service...");
            h.cancel().await;
        }
    }

    /// Starts the Identify service on this connection.
    pub(crate) fn start_identify_push(&mut self, k: PublicKey) {
        let cid = self.id();
        let stream_muxer = self.stream_muxer.clone();
        let pids = vec![IDENTIFY_PUSH_PROTOCOL];
        let ctrl = self.ctrl.clone();

        let info = IdentifyInfo {
            public_key: k,
            protocol_version: "".to_string(),
            agent_version: "".to_string(),
            listen_addrs: vec![],
            protocols: vec!["/p1".to_string(), "/p1".to_string()],
        };

        let mut tx = self.tx.clone();

        let handle = task::spawn(async move {
            let r = open_stream_internal(cid, stream_muxer, pids, ctrl).await;
            match r {
                Ok(stream) => {
                    let sub_stream = stream.clone();
                    let _ = tx.send(SwarmEvent::StreamOpened { sub_stream }).await;
                    // ignore the error
                    let _ = identify::produce_message(stream, info).await;
                }
                Err(err) => {
                    // looks like the peer doesn't support the protocol
                    log::warn!("Identify push protocol not supported: {:?}", err);
                    //Err(err)
                }
            }

            log::trace!("identify push task exiting...");
        });

        self.identify_push_handle = Some(handle);
    }
    pub(crate) async fn stop_identify_push(&mut self) {
        if let Some(h) = self.identify_push_handle.take() {
            log::debug!("stopping Identify Push service...");
            h.cancel().await;
        }
    }

    pub(crate) fn info(&self) -> ConnectionInfo {
        // calculate inbound
        let num_inbound_streams = self.substreams.iter().fold(0usize, |mut acc, s| {
            if s.dir() == Direction::Inbound {
                acc += 1;
            }
            acc
        });
        let num_outbound_streams = self.substreams.len() - num_inbound_streams;
        ConnectionInfo {
            la: self.local_addr(),
            ra: self.remote_addr(),
            local_peer_id: self.local_peer(),
            remote_peer_id: self.remote_peer(),
            num_inbound_streams,
            num_outbound_streams,
        }
    }
}

async fn open_stream_internal(
    cid: ConnectionId,
    mut stream_muxer: IStreamMuxer,
    pids: Vec<ProtocolId>,
    ctrl: mpsc::Sender<SwarmControlCmd>,
) -> Result<Substream, TransportError> {
    let raw_stream = stream_muxer.open_stream().await?;
    let la = stream_muxer.local_multiaddr();
    let ra = stream_muxer.remote_multiaddr();

    // now it's time to do protocol multiplexing for sub stream
    let negotiator = Negotiator::new_with_protocols(pids);
    let result = negotiator.select_one(raw_stream).await;

    match result {
        Ok((proto, raw_stream)) => {
            log::debug!("selected outbound {:?} {:?}", cid, proto.protocol_name_str());
            let stream = Substream::new(raw_stream, Direction::Outbound, proto, cid, la, ra, ctrl);
            Ok(stream)
        }
        Err(err) => {
            log::info!("failed outbound protocol selection {:?} {:?}", cid, err);
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

/// Information about the network obtained by [`Network::info()`].
#[derive(Debug)]
pub struct ConnectionInfo {
    /// The local multiaddr of this connection.
    pub la: Multiaddr,
    /// The remote multiaddr of this connection.
    pub ra: Multiaddr,
    /// The local peer ID.
    pub local_peer_id: PeerId,
    /// The remote peer ID.
    pub remote_peer_id: PeerId,
    /// The total number of inbound sub streams.
    pub num_inbound_streams: usize,
    /// The total number of outbound sub streams.
    pub num_outbound_streams: usize,
    // /// The Sub-streams.
    // pub streams: Vec<StreamStats>,
}
