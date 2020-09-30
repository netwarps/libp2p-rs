use async_trait::async_trait;
use std::{fmt, io};
use futures::channel::mpsc;
use futures::SinkExt;

use libp2p_traits::{ReadEx, WriteEx};
use libp2p_core::muxing::StreamInfo;
use libp2p_core::upgrade::ProtocolName;
use crate::control::SwarmControlCmd;
use crate::connection::{ConnectionId, Direction};
use crate::ProtocolId;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use libp2p_core::Multiaddr;

/// The Id of sub stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId(usize);

#[derive(Debug, Default)]
pub struct SubstreamStats {
    pkt_sent: AtomicUsize,
    pkt_recv: AtomicUsize,
    byte_sent: AtomicUsize,
    byte_recv: AtomicUsize,
}

#[derive(Clone)]
pub struct Substream<TStream> {
    /// The inner sub stream, created by the StreamMuxer
    inner: TStream,
    /// The protocol of the sub stream
    protocol: ProtocolId,
    /// The direction of the sub stream
    dir: Direction,
    /// The connection ID of the sub stream
    /// It can be used to back track to the stream muxer
    cid: ConnectionId,
    /// The local multiaddr of the sub stream.
    la: Multiaddr,
    /// The remote multiaddr of the sub stream.
    ra: Multiaddr,
    /// The control channel for closing stream
    ctrl: Option<mpsc::Sender<SwarmControlCmd<Substream<TStream>>>>,
    /// The statistics of the substream
    stats: Arc<SubstreamStats>,
}

impl<TStream: fmt::Debug> fmt::Debug for Substream<TStream> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Substream")
            .field("inner", &self.inner)
            .field("protocol", &self.protocol.protocol_name_str())
            .field("dir", &self.dir)
            .field("cid", &self.cid)
            .finish()
    }
}

impl<TStream: StreamInfo> Substream<TStream> {
    pub(crate) fn new(inner: TStream,
                      dir: Direction,
                      protocol: ProtocolId,
                      cid: ConnectionId,
                      la: Multiaddr,
                      ra: Multiaddr,
                      ctrl: Option<mpsc::Sender<SwarmControlCmd<Substream<TStream>>>>,)
        -> Self {
        Self { inner, protocol, dir, cid, la, ra, ctrl, stats: Arc::new(SubstreamStats::default()) }
    }
    /// For internal test only
    #[allow(dead_code)]
    pub(crate) fn new_with_default(inner: TStream) -> Self {
        let protocol = b"/test";
        let dir = Direction::Outbound;
        let cid = ConnectionId::default();
        let la = Multiaddr::empty();
        let ra = Multiaddr::empty();
        Self { inner, protocol, dir, cid, la, ra, ctrl: None, stats: Arc::new(SubstreamStats::default()) }
    }
    /// Returns the protocol of the sub stream.
    pub fn protocol(&self) -> ProtocolId {
        self.protocol
    }
    /// Returns the connection id of the sub stream.
    pub fn cid(&self) -> ConnectionId {
        self.cid
    }
    /// Returns the sub stream Id.
    pub fn id(&self) -> StreamId {
        StreamId(self.inner.id())
    }
    /// Returns the remote multiaddr of the sub stream.
    pub fn remote_multiaddr(&self) -> Multiaddr {
        self.ra.clone()
    }
    /// Returns the remote multiaddr of the sub stream.
    pub fn local_multiaddr(&self) -> Multiaddr {
        self.la.clone()
    }
    /// Returns the statistics of the sub stream.
    pub fn stats(&self) -> &SubstreamStats {
        &self.stats
    }
}

#[async_trait]
impl<TStream: ReadEx + Send> ReadEx for Substream<TStream> {
    async fn read2(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        self.inner.read2(buf).await
            .map(|n|{
                self.stats.byte_recv.fetch_add(n, Ordering::SeqCst);
                self.stats.pkt_recv.fetch_add(1, Ordering::SeqCst);
                n
        })
    }
}

#[async_trait]
impl<TStream: StreamInfo + WriteEx + Send> WriteEx for Substream<TStream> {
    async fn write2(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.inner.write2(buf).await
            .map(|n|{
                self.stats.byte_sent.fetch_add(n, Ordering::SeqCst);
                self.stats.pkt_sent.fetch_add(1, Ordering::SeqCst);
                n
            })
    }

    async fn flush2(&mut self) -> Result<(), io::Error> {
        self.inner.flush2().await
    }

    // try to send a CloseStream command to Swarm, then close inner stream
    async fn close2(&mut self) -> Result<(), io::Error> {
        if let Some(mut cmd) = self.ctrl.take() {
            // to ask Swarm to remove myself
            let cid = self.cid;
            let sid = self.id();
            let _ = cmd.send(SwarmControlCmd::CloseStream(cid, sid)).await;
        }
        self.inner.close2().await
    }
}
