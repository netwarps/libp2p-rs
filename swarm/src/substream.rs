use async_trait::async_trait;
use futures::channel::mpsc;
use futures::SinkExt;
use std::{fmt, io};

use crate::connection::{ConnectionId, Direction};
use crate::control::SwarmControlCmd;
use crate::ProtocolId;
use libp2p_core::muxing::StreamInfo;
use libp2p_core::upgrade::ProtocolName;
use libp2p_core::Multiaddr;
use libp2p_traits::{ReadEx, WriteEx};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// The Id of sub stream
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId(usize);

#[derive(Debug, Default)]
pub struct SubstreamStats {
    /// The accumulative counter of packets sent.
    pkt_sent: AtomicUsize,
    /// The accumulative counter of packets received.
    pkt_recv: AtomicUsize,
    /// The accumulative counter of bytes sent.
    byte_sent: AtomicUsize,
    /// The accumulative counter of bytes received.
    byte_recv: AtomicUsize,
}

#[derive(Debug)]
pub struct SubstreamInfo {
    /// The protocol of the sub stream.
    protocol: ProtocolId,
    /// The direction of the sub stream.
    dir: Direction,
}


#[derive(Debug)]
struct SubstreamMeta {
    /// The protocol of the sub stream.
    protocol: ProtocolId,
    /// The direction of the sub stream.
    dir: Direction,
    /// The connection ID of the sub stream
    /// It can be used to back track to the stream muxer.
    cid: ConnectionId,
    /// The local multiaddr of the sub stream.
    la: Multiaddr,
    /// The remote multiaddr of the sub stream.
    ra: Multiaddr,
}

#[derive(Clone)]
pub struct Substream<TStream> {
    /// The inner sub stream, created by the StreamMuxer
    inner: TStream,
    /// The inner information of the sub-stream
    info: Arc<SubstreamMeta>,
    /// The control channel for closing stream
    ctrl: mpsc::Sender<SwarmControlCmd<Substream<TStream>>>,
    /// The statistics of the substream
    stats: Arc<SubstreamStats>,
}

impl<TStream: fmt::Debug> fmt::Debug for Substream<TStream> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Substream")
            .field("inner", &self.inner)
            .field("protocol", &self.info.protocol.protocol_name_str())
            .field("dir", &self.info.dir)
            .field("cid", &self.info.cid)
            .finish()
    }
}

impl<TStream: StreamInfo> Substream<TStream> {
    pub(crate) fn new(
        inner: TStream,
        dir: Direction,
        protocol: ProtocolId,
        cid: ConnectionId,
        la: Multiaddr,
        ra: Multiaddr,
        ctrl: mpsc::Sender<SwarmControlCmd<Substream<TStream>>>,
    ) -> Self {
        Self {
            inner,
            info: Arc::new(SubstreamMeta {
                protocol,
                dir,
                cid,
                la,
                ra,
            }),
            ctrl,
            stats: Arc::new(SubstreamStats::default()),
        }
    }
    /// For internal test only
    #[allow(dead_code)]
    pub(crate) fn new_with_default(inner: TStream) -> Self {
        let protocol = b"/test";
        let dir = Direction::Outbound;
        let cid = ConnectionId::default();
        let la = Multiaddr::empty();
        let ra = Multiaddr::empty();
        let (ctrl, _) = mpsc::channel(0);
        Self {
            inner,
            info: Arc::new(SubstreamMeta {
                protocol,
                dir,
                cid,
                la,
                ra,
            }),
            ctrl,
            stats: Arc::new(SubstreamStats::default()),
        }
    }
    /// Returns the protocol of the sub stream.
    pub fn protocol(&self) -> ProtocolId {
        self.info.protocol
    }
    /// Returns the direction of the sub stream.
    pub fn dir(&self) -> Direction {
        self.info.dir
    }
    /// Returns the connection id of the sub stream.
    pub fn cid(&self) -> ConnectionId {
        self.info.cid
    }
    /// Returns the sub stream Id.
    pub fn id(&self) -> StreamId {
        StreamId(self.inner.id())
    }
    /// Returns the remote multiaddr of the sub stream.
    pub fn remote_multiaddr(&self) -> Multiaddr {
        self.info.ra.clone()
    }
    /// Returns the remote multiaddr of the sub stream.
    pub fn local_multiaddr(&self) -> Multiaddr {
        self.info.la.clone()
    }
    /// Returns the statistics of the sub stream.
    pub fn stats(&self) -> &SubstreamStats {
        &self.stats
    }
    /// Returns the info of the sub stream.
    pub fn info(&self) -> SubstreamInfo {
        SubstreamInfo {
            protocol: self.protocol(),
            dir: self.dir()
        }
    }
}

#[async_trait]
impl<TStream: ReadEx + Send> ReadEx for Substream<TStream> {
    async fn read2(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        self.inner.read2(buf).await.map(|n| {
            self.stats.byte_recv.fetch_add(n, Ordering::SeqCst);
            self.stats.pkt_recv.fetch_add(1, Ordering::SeqCst);
            n
        })
    }
}

#[async_trait]
impl<TStream: StreamInfo + WriteEx + Send> WriteEx for Substream<TStream> {
    async fn write2(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.inner.write2(buf).await.map(|n| {
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
        // to ask Swarm to remove myself
        let cid = self.cid();
        let sid = self.id();
        let _ = self.ctrl.send(SwarmControlCmd::CloseStream(cid, sid)).await;
        self.inner.close2().await
    }
}
