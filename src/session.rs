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

use futures::{channel::mpsc, prelude::*, stream::iter};
use log::{debug, error, trace};
use std::collections::{HashMap, HashSet, VecDeque};
use std::{
    io::{self, ErrorKind},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::prelude::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Framed, FramedParts, LengthDelimitedCodec};

use crate::{
    channel::{mpsc as priority_mpsc, mpsc::Priority},
    context::SessionContext,
    error::{HandshakeErrorKind, ProtocolHandleErrorKind, TransportErrorKind},
    multiaddr::Multiaddr,
    protocol_handle_stream::{ServiceProtocolEvent, SessionProtocolEvent},
    protocol_select::{client_select, server_select, ProtocolInfo},
    secio::PublicKey,
    service::{
        config::{Meta, SessionConfig},
        future_task::BoxedFutureTask,
        SessionType, BUF_SHRINK_THRESHOLD, DELAY_TIME, RECEIVED_BUFFER_SIZE, RECEIVED_SIZE,
        SEND_SIZE,
    },
    substream::{ProtocolEvent, SubstreamBuilder},
    transports::MultiIncoming,
    yamux::{Session as YamuxSession, StreamHandle},
    ProtocolId, SessionId, StreamId,
};

pub trait AsyncRW: AsyncWrite + AsyncRead {}

impl<T: AsyncRead + AsyncWrite> AsyncRW for T {}

/// Event generated/received by the Session
pub(crate) enum SessionEvent {
    /// Session close event
    SessionClose {
        /// Session id
        id: SessionId,
    },
    ListenStart {
        listen_address: Multiaddr,
        incoming: MultiIncoming,
    },
    HandshakeSuccess {
        /// In order to be compatible with multiple underlying connection abstractions,
        /// the dyn trait needs to be used here
        handle: Box<dyn AsyncRW + Send + Unpin + 'static>,
        /// Remote Public key
        public_key: Option<PublicKey>,
        /// Remote address
        address: Multiaddr,
        /// Session type
        ty: SessionType,
        /// listen addr
        listen_address: Option<Multiaddr>,
    },
    HandshakeError {
        /// remote address
        address: Multiaddr,
        /// Session type
        ty: SessionType,
        /// error
        error: HandshakeErrorKind,
    },
    DialError {
        /// remote address
        address: Multiaddr,
        /// error
        error: TransportErrorKind,
    },
    ListenError {
        /// listen address
        address: Multiaddr,
        /// error
        error: TransportErrorKind,
    },
    /// Protocol data
    ProtocolMessage {
        /// Session id
        id: SessionId,
        /// Protocol id
        proto_id: ProtocolId,
        /// Data
        data: bytes::Bytes,
    },
    /// Protocol open event
    ProtocolOpen {
        /// Session id
        id: SessionId,
        /// Protocol id
        proto_id: ProtocolId,
        /// Protocol version
        version: String,
    },
    /// Protocol close event
    ProtocolClose {
        /// Session id
        id: SessionId,
        /// Protocol id
        proto_id: ProtocolId,
    },
    ProtocolSelectError {
        /// Session id
        id: SessionId,
        /// proto_name
        proto_name: Option<String>,
    },
    SessionTimeout {
        /// Session id
        id: SessionId,
    },
    /// Codec error
    ProtocolError {
        /// Session id
        id: SessionId,
        /// Protocol id
        proto_id: ProtocolId,
        /// Codec error
        error: std::io::Error,
    },
    MuxerError {
        id: SessionId,
        error: std::io::Error,
    },
    /// Protocol handle error, will cause memory leaks/abnormal CPU usage
    ProtocolHandleError {
        /// Error message
        error: ProtocolHandleErrorKind,
        /// Protocol id
        proto_id: ProtocolId,
    },
}

/// Wrapper for real data streams, such as TCP stream
pub(crate) struct Session<T> {
    socket: YamuxSession<T>,

    protocol_configs_by_name: HashMap<String, Arc<Meta>>,
    protocol_configs_by_id: HashMap<ProtocolId, Arc<Meta>>,

    config: SessionConfig,

    timeout: Duration,

    event: HashSet<ProtocolId>,

    keep_buffer: bool,

    state: SessionState,

    context: Arc<SessionContext>,

    next_stream: StreamId,

    /// Sub streams maps a stream id to a sender of sub stream
    sub_streams: HashMap<StreamId, priority_mpsc::Sender<ProtocolEvent>>,
    proto_streams: HashMap<ProtocolId, StreamId>,
    /// The buffer will be prioritized for distribute to sub streams
    high_write_buf: VecDeque<(ProtocolId, ProtocolEvent)>,
    /// The buffer which will distribute to sub streams
    write_buf: VecDeque<(ProtocolId, ProtocolEvent)>,
    /// The buffer which will send to service
    read_buf: VecDeque<SessionEvent>,

    /// Clone to new sub stream
    proto_event_sender: mpsc::Sender<ProtocolEvent>,
    /// Receive events from sub streams
    proto_event_receiver: mpsc::Receiver<ProtocolEvent>,

    /// Send events to service
    service_sender: mpsc::Sender<SessionEvent>,
    /// Receive event from service
    service_receiver: priority_mpsc::Receiver<SessionEvent>,

    service_proto_senders: HashMap<ProtocolId, mpsc::Sender<ServiceProtocolEvent>>,
    session_proto_senders: HashMap<ProtocolId, mpsc::Sender<SessionProtocolEvent>>,

    /// Delay notify with abnormally poor machines
    delay: Arc<AtomicBool>,

    last_sent: Instant,
    future_task_sender: mpsc::Sender<BoxedFutureTask>,
    wait_handle: Vec<(
        Option<futures::channel::oneshot::Sender<()>>,
        tokio::task::JoinHandle<()>,
    )>,
}

impl<T> Session<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    /// New a session
    pub fn new(
        socket: T,
        service_sender: mpsc::Sender<SessionEvent>,
        service_receiver: priority_mpsc::Receiver<SessionEvent>,
        meta: SessionMeta,
        future_task_sender: mpsc::Sender<BoxedFutureTask>,
    ) -> Self {
        let socket = YamuxSession::new(socket, meta.config.yamux_config, meta.context.ty.into());
        let (proto_event_sender, proto_event_receiver) = mpsc::channel(RECEIVED_SIZE);
        let mut interval = proto_event_sender.clone();

        // NOTE: A Interval/Delay will block tokio runtime from gracefully shutdown.
        //       So we spawn it in FutureTaskManager
        let mut future_task_sender_ = future_task_sender.clone();
        let timeout = meta.timeout;
        tokio::spawn(async move {
            tokio::time::delay_until(tokio::time::Instant::now() + timeout).await;
            let task = Box::pin(async move {
                if interval.send(ProtocolEvent::TimeoutCheck).await.is_err() {
                    trace!("timeout check send err")
                }
            });
            if future_task_sender_.send(task).await.is_err() {
                trace!("timeout check task send err")
            }
        });

        Session {
            socket,
            protocol_configs_by_name: meta.protocol_configs_by_name,
            protocol_configs_by_id: meta.protocol_configs_by_id,
            config: meta.config,
            timeout: meta.timeout,
            context: meta.context,
            keep_buffer: meta.keep_buffer,
            next_stream: 0,
            sub_streams: HashMap::default(),
            proto_streams: HashMap::default(),
            high_write_buf: VecDeque::default(),
            write_buf: VecDeque::default(),
            read_buf: VecDeque::default(),
            proto_event_sender,
            proto_event_receiver,
            service_sender,
            service_receiver,
            service_proto_senders: meta.service_proto_senders,
            session_proto_senders: meta.session_proto_senders,
            delay: Arc::new(AtomicBool::new(false)),
            state: SessionState::Normal,
            event: meta.event,
            last_sent: Instant::now(),
            future_task_sender,
            wait_handle: meta.session_proto_handles,
        }
    }

    /// select procedure
    #[inline(always)]
    fn select_procedure(
        &mut self,
        procedure: impl Future<
                Output = Result<
                    (
                        Framed<StreamHandle, LengthDelimitedCodec>,
                        String,
                        Option<String>,
                    ),
                    io::Error,
                >,
            > + Send
            + 'static,
    ) {
        let mut event_sender = self.proto_event_sender.clone();
        let timeout = self.timeout;

        // NOTE: A Interval/Delay will block tokio runtime from gracefully shutdown.
        //       So we spawn it in FutureTaskManager
        let task = Box::pin(async move {
            let event = match tokio::time::timeout(timeout, procedure).await {
                Ok(res) => match res {
                    Ok((handle, name, version)) => match version {
                        Some(version) => ProtocolEvent::Open {
                            sub_stream: Box::new(handle),
                            proto_name: name,
                            version,
                        },
                        None => {
                            debug!("Negotiation to open the protocol {} failed", name);
                            ProtocolEvent::SelectError {
                                proto_name: Some(name),
                            }
                        }
                    },
                    Err(err) => {
                        debug!("stream protocol select err: {:?}", err);
                        ProtocolEvent::SelectError { proto_name: None }
                    }
                },
                Err(err) => {
                    debug!("stream protocol select err: {:?}", err);
                    ProtocolEvent::SelectError { proto_name: None }
                }
            };
            if let Err(err) = event_sender.send(event).await {
                debug!("select result send back error: {:?}", err);
            }
        }) as BoxedFutureTask;

        let mut future_task_sender = self.future_task_sender.clone();
        tokio::spawn(async move {
            if future_task_sender.send(task).await.is_err() {
                trace!("select procedure send err")
            }
        });
    }

    /// After the session is established, the client is requested to open some custom protocol sub stream.
    pub fn open_proto_stream(&mut self, proto_name: &str) {
        let handle = match self.socket.open_stream() {
            Ok(handle) => handle,
            Err(e) => {
                debug!("session {} open stream error: {}", self.context.id, e);
                return;
            }
        };
        debug!("try open proto, {}", proto_name);
        let versions = self.protocol_configs_by_name[proto_name]
            .support_versions
            .clone();
        let proto_info = ProtocolInfo::new(&proto_name, versions);

        let task = client_select(handle, proto_info);
        self.select_procedure(task);
    }

    /// Push the generated event to the Service
    #[inline]
    fn event_output(&mut self, cx: &mut Context, event: SessionEvent) {
        self.read_buf.push_back(event);
        self.output(cx);
    }

    #[inline]
    fn output(&mut self, cx: &mut Context) {
        while let Some(event) = self.read_buf.pop_front() {
            if let Err(e) = self.service_sender.try_send(event) {
                if e.is_full() {
                    self.read_buf.push_front(e.into_inner());
                    self.set_delay(cx);
                    return;
                } else {
                    error!("session send to service error: {}", e);
                    self.read_buf.clear();
                    return;
                }
            }
        }
    }

    fn push_back(&mut self, priority: Priority, id: ProtocolId, event: ProtocolEvent) {
        if priority.is_high() {
            self.high_write_buf.push_back((id, event));
        } else {
            self.write_buf.push_back((id, event));
        }
    }

    #[inline(always)]
    fn distribute_to_substream_process<D: Iterator<Item = (ProtocolId, ProtocolEvent)>>(
        &mut self,
        cx: &mut Context,
        data: D,
        priority: Priority,
        block_substreams: &mut HashSet<ProtocolId>,
    ) {
        for (proto_id, event) in data {
            // Guarantee the order in which messages are sent
            if block_substreams.contains(&proto_id) {
                self.push_back(priority, proto_id, event);
                continue;
            }
            if let Some(stream_id) = self.proto_streams.get(&proto_id) {
                if let Some(sender) = self.sub_streams.get_mut(&stream_id) {
                    if let Err(e) = sender.try_send(event) {
                        if e.is_full() {
                            self.push_back(priority, proto_id, e.into_inner());
                            self.set_delay(cx);
                            block_substreams.insert(proto_id);
                        } else {
                            debug!("session send to sub stream error: {}", e);
                        }
                    } else {
                        self.last_sent = Instant::now();
                    }
                };
            }
        }
    }

    #[inline]
    fn distribute_to_substream(&mut self, cx: &mut Context) {
        let mut block_substreams = HashSet::new();

        let high = self.high_write_buf.split_off(0).into_iter();
        self.distribute_to_substream_process(cx, high, Priority::High, &mut block_substreams);

        if self.sub_streams.len() > block_substreams.len() {
            let normal = self.write_buf.split_off(0).into_iter();
            self.distribute_to_substream_process(
                cx,
                normal,
                Priority::Normal,
                &mut block_substreams,
            );
        }

        if self.write_buf.capacity() > BUF_SHRINK_THRESHOLD {
            self.write_buf.shrink_to_fit();
        }

        if self.high_write_buf.capacity() > BUF_SHRINK_THRESHOLD {
            self.high_write_buf.shrink_to_fit();
        }
    }

    /// Handling client-initiated open protocol sub stream requests
    fn handle_sub_stream(&mut self, sub_stream: StreamHandle) {
        let proto_metas = self
            .protocol_configs_by_name
            .values()
            .map(|proto_meta| {
                let name = (proto_meta.name)(proto_meta.id);
                let proto_info = ProtocolInfo::new(&name, proto_meta.support_versions.clone());
                let select_fn = (proto_meta.select_version)();
                (name, (proto_info, select_fn))
            })
            .collect();

        let task = server_select(sub_stream, proto_metas);
        self.select_procedure(task);
    }

    fn open_protocol(
        &mut self,
        cx: &mut Context,
        name: String,
        version: String,
        sub_stream: Box<Framed<StreamHandle, LengthDelimitedCodec>>,
    ) {
        let proto = match self.protocol_configs_by_name.get(&name) {
            Some(proto) => proto,
            None => {
                // if the server intentionally returns malicious protocol data with arbitrary
                // protocol names, close the connection and feedback error
                self.state = SessionState::Abnormal;
                self.event_output(
                    cx,
                    SessionEvent::ProtocolSelectError {
                        id: self.context.id,
                        proto_name: None,
                    },
                );
                return;
            }
        };

        let proto_id = proto.id;
        // open twice at the same protocol, ignore it
        if self.proto_streams.contains_key(&proto_id) {
            return;
        }
        let before_receive_fn = (proto.before_receive)();
        let raw_part = sub_stream.into_parts();
        let mut part = FramedParts::new(raw_part.io, (proto.codec)());
        // Replace buffered data
        part.read_buf = raw_part.read_buf;
        part.write_buf = raw_part.write_buf;
        let frame = Framed::from_parts(part);
        let (session_to_proto_sender, session_to_proto_receiver) =
            priority_mpsc::channel(SEND_SIZE);

        let mut proto_stream = SubstreamBuilder::new(
            self.proto_event_sender.clone(),
            session_to_proto_receiver,
            self.context.clone(),
        )
        .proto_id(proto_id)
        .stream_id(self.next_stream)
        .config(self.config)
        .service_proto_sender(self.service_proto_senders.get(&proto_id).cloned())
        .session_proto_sender(self.session_proto_senders.get(&proto_id).cloned())
        .keep_buffer(self.keep_buffer)
        .event(self.event.contains(&proto_id))
        .before_receive(before_receive_fn)
        .build(frame);

        self.sub_streams
            .insert(self.next_stream, session_to_proto_sender);
        self.proto_streams.insert(proto_id, self.next_stream);

        proto_stream.proto_open(version.clone());

        if self.event.contains(&proto_id) {
            self.event_output(
                cx,
                SessionEvent::ProtocolOpen {
                    id: self.context.id,
                    proto_id,
                    version,
                },
            );
        }

        self.next_stream += 1;

        debug!("session [{}] proto [{}] open", self.context.id, proto_id);
        tokio::spawn(proto_stream.for_each(|_| future::ready(())));
    }

    /// Handling events uploaded by the protocol stream
    fn handle_stream_event(&mut self, cx: &mut Context, event: ProtocolEvent) {
        match event {
            ProtocolEvent::Open {
                proto_name,
                sub_stream,
                version,
            } => {
                self.open_protocol(cx, proto_name, version, sub_stream);
            }
            ProtocolEvent::Close { id, proto_id } => {
                debug!("session [{}] proto [{}] closed", self.context.id, proto_id);
                if self.sub_streams.remove(&id).is_some() {
                    self.proto_streams.remove(&proto_id);
                    if self.event.contains(&proto_id) {
                        self.event_output(
                            cx,
                            SessionEvent::ProtocolClose {
                                id: self.context.id,
                                proto_id,
                            },
                        );
                    }
                }
            }
            ProtocolEvent::Message { data, proto_id, .. } => {
                debug!("get proto [{}] data len: {}", proto_id, data.len());
                if self.state == SessionState::RemoteClose && !self.keep_buffer {
                    return;
                }
                self.event_output(
                    cx,
                    SessionEvent::ProtocolMessage {
                        id: self.context.id,
                        proto_id,
                        data,
                    },
                )
            }
            ProtocolEvent::SelectError { proto_name } => self.event_output(
                cx,
                SessionEvent::ProtocolSelectError {
                    id: self.context.id,
                    proto_name,
                },
            ),
            ProtocolEvent::Error {
                proto_id, error, ..
            } => {
                debug!("Codec error: {:?}", error);
                self.event_output(
                    cx,
                    SessionEvent::ProtocolError {
                        id: self.context.id,
                        proto_id,
                        error,
                    },
                )
            }
            ProtocolEvent::TimeoutCheck => {
                if self.sub_streams.is_empty() {
                    self.event_output(
                        cx,
                        SessionEvent::SessionTimeout {
                            id: self.context.id,
                        },
                    );
                    self.state = SessionState::LocalClose;
                }
            }
        }
    }

    /// Handling events send by the service
    #[allow(clippy::map_entry)]
    fn handle_session_event(&mut self, cx: &mut Context, event: SessionEvent, priority: Priority) {
        match event {
            SessionEvent::ProtocolMessage { proto_id, data, .. } => {
                if let Some(stream_id) = self.proto_streams.get(&proto_id) {
                    let event = ProtocolEvent::Message {
                        id: *stream_id,
                        proto_id,
                        data,
                    };
                    self.push_back(priority, proto_id, event);
                } else {
                    trace!("protocol {} not ready", proto_id);
                }
            }
            SessionEvent::SessionClose { .. } => {
                if self.sub_streams.is_empty() {
                    // if no proto open, just close session
                    self.close_session(cx);
                } else {
                    self.state = SessionState::LocalClose;
                    self.close_all_proto(cx);
                }
            }
            SessionEvent::ProtocolOpen { proto_id, .. } => {
                if self.proto_streams.contains_key(&proto_id) {
                    debug!("proto [{}] has been open", proto_id);
                } else if let Some(name) = self
                    .protocol_configs_by_id
                    .get(&proto_id)
                    .map(|meta| (meta.name)(meta.id))
                {
                    self.open_proto_stream(&name)
                } else {
                    debug!("This protocol [{}] is not supported", proto_id)
                }
            }
            SessionEvent::ProtocolClose { proto_id, .. } => {
                if let Some(stream_id) = self.proto_streams.get(&proto_id) {
                    self.write_buf.push_back((
                        proto_id,
                        ProtocolEvent::Close {
                            id: *stream_id,
                            proto_id,
                        },
                    ));
                } else {
                    debug!("proto [{}] has been closed", proto_id);
                }
            }
            _ => (),
        }
        self.distribute_to_substream(cx);
    }

    fn poll_inner_socket(&mut self, cx: &mut Context) {
        loop {
            if !self.state.is_normal() {
                break;
            }
            match Pin::new(&mut self.socket).as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(sub_stream))) => self.handle_sub_stream(sub_stream),
                Poll::Ready(None) => {
                    self.state = SessionState::RemoteClose;
                    break;
                }
                Poll::Pending => {
                    break;
                }
                Poll::Ready(Some(Err(err))) => {
                    debug!("session poll error: {:?}", err);
                    self.write_buf.clear();
                    self.high_write_buf.clear();
                    if !self.keep_buffer {
                        self.read_buf.clear()
                    }

                    match err.kind() {
                        ErrorKind::BrokenPipe
                        | ErrorKind::ConnectionAborted
                        | ErrorKind::ConnectionReset
                        | ErrorKind::NotConnected
                        | ErrorKind::UnexpectedEof => self.state = SessionState::RemoteClose,
                        _ => {
                            debug!("MuxerError: {:?}", err);
                            self.event_output(
                                cx,
                                SessionEvent::MuxerError {
                                    id: self.context.id,
                                    error: err,
                                },
                            );
                            self.state = SessionState::Abnormal;
                        }
                    }

                    break;
                }
            }
        }
    }

    fn recv_substreams(&mut self, cx: &mut Context) {
        loop {
            if self.read_buf.len() > self.config.recv_event_size() {
                break;
            }

            match Pin::new(&mut self.proto_event_receiver)
                .as_mut()
                .poll_next(cx)
            {
                Poll::Ready(Some(event)) => {
                    // Local close means user doesn't want any message from this session
                    // But when remote close, we should try my best to accept all data as much as possible
                    if self.state.is_local_close() {
                        continue;
                    }
                    self.handle_stream_event(cx, event)
                }
                Poll::Ready(None) => {
                    // Drop by self
                    self.state = SessionState::LocalClose;
                    return;
                }
                Poll::Pending => {
                    break;
                }
            }
        }
    }

    fn recv_service(&mut self, cx: &mut Context) {
        loop {
            if self.high_write_buf.len() > RECEIVED_BUFFER_SIZE
                && self.write_buf.len() > RECEIVED_BUFFER_SIZE
            {
                break;
            }

            match Pin::new(&mut self.service_receiver).as_mut().poll_next(cx) {
                Poll::Ready(Some((priority, event))) => {
                    if !self.state.is_normal() {
                        break;
                    } else {
                        self.handle_session_event(cx, event, priority)
                    }
                }
                Poll::Ready(None) => {
                    // Must drop by service
                    self.state = SessionState::LocalClose;
                    self.clean(cx);
                    break;
                }
                Poll::Pending => break,
            }
        }
    }

    /// Try close all protocol
    #[inline]
    fn close_all_proto(&mut self, cx: &mut Context) {
        if self.context.closed.load(Ordering::SeqCst) {
            self.close_session(cx)
        }
        self.context.closed.store(true, Ordering::SeqCst);
        self.set_delay(cx);
    }

    /// Close session
    fn close_session(&mut self, cx: &mut Context) {
        self.context.closed.store(true, Ordering::SeqCst);

        self.read_buf.push_back(SessionEvent::SessionClose {
            id: self.context.id,
        });
        let events = self.read_buf.split_off(0);
        let mut sender = self.service_sender.clone();

        tokio::spawn(async move {
            let mut iter = iter(events).map(Ok);
            if let Err(e) = sender.send_all(&mut iter).await {
                debug!("session close event send to service error: {:?}", e)
            }
        });
        self.clean(cx);
    }

    fn wait_handle_poll(&mut self, cx: &mut Context) -> Poll<Option<()>> {
        for (sender, mut handle) in self.wait_handle.split_off(0) {
            if let Some(sender) = sender {
                // don't care about it
                let _ignore = sender.send(());
            }
            match handle.poll_unpin(cx) {
                Poll::Pending => {
                    self.wait_handle.push((None, handle));
                }
                Poll::Ready(_) => (),
            }
        }
        if self.wait_handle.is_empty() {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }

    /// Clean env
    fn clean(&mut self, cx: &mut Context) {
        self.sub_streams.clear();
        self.service_receiver.close();
        self.proto_event_receiver.close();

        if let Err(e) = self.socket.shutdown(cx) {
            trace!("socket shutdown err: {}", e)
        }
    }

    #[inline]
    fn flush(&mut self, cx: &mut Context) {
        self.distribute_to_substream(cx);
        self.output(cx);
    }

    #[inline]
    fn set_delay(&mut self, cx: &mut Context) {
        // Why use `delay` instead of `notify`?
        //
        // In fact, on machines that can use multi-core normally, there is almost no problem with the `notify` behavior,
        // and even the efficiency will be higher.
        //
        // However, if you are on a single-core bully machine, `notify` may have a very amazing starvation behavior.
        //
        // Under a single-core machine, `notify` may fall into the loop of infinitely preemptive CPU, causing starvation.
        if !self.delay.load(Ordering::Acquire) {
            self.delay.store(true, Ordering::Release);
            let waker = cx.waker().clone();
            let delay = self.delay.clone();
            tokio::spawn(async move {
                tokio::time::delay_until(tokio::time::Instant::now() + DELAY_TIME).await;
                waker.wake();
                delay.store(false, Ordering::Release);
            });
        }
    }
}

impl<T> Stream for Session<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        debug!(
            "session [{}], [{:?}], proto count [{}], state: {:?} ,\
             read buf: {}, write buf: {}, high_write_buf: {}",
            self.context.id,
            self.context.ty,
            self.sub_streams.len(),
            self.state,
            self.read_buf.len(),
            self.write_buf.len(),
            self.high_write_buf.len()
        );

        // double check here
        if self.state.is_local_close() {
            debug!(
                "Session({:?}) finished, self.state.is_local_close()",
                self.context.id
            );
            return Poll::Ready(None);
        }

        if !self.read_buf.is_empty()
            || !self.write_buf.is_empty()
            || !self.high_write_buf.is_empty()
        {
            self.flush(cx);
        }

        self.poll_inner_socket(cx);

        self.recv_substreams(cx);

        self.recv_service(cx);

        match self.state {
            SessionState::LocalClose | SessionState::Abnormal => {
                debug!(
                    "Session({:?}) finished, LocalClose||Abnormal",
                    self.context.id
                );
                let id = self.context.id;
                let protos = ::std::mem::take(&mut self.proto_streams);
                for (proto_id, _) in protos {
                    // make sure close protocol is early than close session
                    if self.event.contains(&proto_id) {
                        self.read_buf
                            .push_back(SessionEvent::ProtocolClose { id, proto_id });
                    }
                }
                self.close_session(cx);
                return self.wait_handle_poll(cx);
            }
            SessionState::RemoteClose => {
                // try close all protocol stream, and then close session
                if self.proto_streams.is_empty() {
                    debug!("Session({:?}) finished, RemoteClose", self.context.id);
                    self.close_session(cx);
                    return self.wait_handle_poll(cx);
                } else {
                    self.close_all_proto(cx);
                }
            }
            SessionState::Normal => (),
        }

        Poll::Pending
    }
}

pub(crate) struct SessionMeta {
    config: SessionConfig,
    protocol_configs_by_name: HashMap<String, Arc<Meta>>,
    protocol_configs_by_id: HashMap<ProtocolId, Arc<Meta>>,
    context: Arc<SessionContext>,
    timeout: Duration,
    keep_buffer: bool,
    service_proto_senders: HashMap<ProtocolId, mpsc::Sender<ServiceProtocolEvent>>,
    session_proto_senders: HashMap<ProtocolId, mpsc::Sender<SessionProtocolEvent>>,
    event: HashSet<ProtocolId>,
    session_proto_handles: Vec<(
        Option<futures::channel::oneshot::Sender<()>>,
        tokio::task::JoinHandle<()>,
    )>,
}

impl SessionMeta {
    pub fn new(timeout: Duration, context: Arc<SessionContext>) -> Self {
        SessionMeta {
            config: SessionConfig::default(),
            protocol_configs_by_name: HashMap::new(),
            protocol_configs_by_id: HashMap::new(),
            context,
            timeout,
            keep_buffer: false,
            service_proto_senders: HashMap::default(),
            session_proto_senders: HashMap::default(),
            event: HashSet::new(),
            session_proto_handles: Vec::new(),
        }
    }

    pub fn protocol_by_name(mut self, config: HashMap<String, Arc<Meta>>) -> Self {
        self.protocol_configs_by_name = config;
        self
    }

    pub fn protocol_by_id(mut self, config: HashMap<ProtocolId, Arc<Meta>>) -> Self {
        self.protocol_configs_by_id = config;
        self
    }

    pub fn config(mut self, config: SessionConfig) -> Self {
        self.config = config;
        self
    }

    pub fn keep_buffer(mut self, keep: bool) -> Self {
        self.keep_buffer = keep;
        self
    }

    pub fn service_proto_senders(
        mut self,
        senders: HashMap<ProtocolId, mpsc::Sender<ServiceProtocolEvent>>,
    ) -> Self {
        self.service_proto_senders = senders;
        self
    }

    pub fn session_senders(
        mut self,
        senders: HashMap<ProtocolId, mpsc::Sender<SessionProtocolEvent>>,
    ) -> Self {
        self.session_proto_senders = senders;
        self
    }

    pub fn session_proto_handles(
        mut self,
        handles: Vec<(
            Option<futures::channel::oneshot::Sender<()>>,
            tokio::task::JoinHandle<()>,
        )>,
    ) -> Self {
        self.session_proto_handles = handles;
        self
    }

    pub fn event(mut self, event: HashSet<ProtocolId>) -> Self {
        self.event = event;
        self
    }
}

/// Session state
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum SessionState {
    /// Close by remote, accept all data as much as possible
    RemoteClose,
    /// Close by self, don't receive any more
    LocalClose,
    /// Normal communication
    Normal,
    /// Abnormal state
    Abnormal,
}

impl SessionState {
    #[inline]
    fn is_local_close(self) -> bool {
        match self {
            SessionState::LocalClose => true,
            _ => false,
        }
    }

    #[inline]
    fn is_normal(self) -> bool {
        match self {
            SessionState::Normal => true,
            _ => false,
        }
    }
}
