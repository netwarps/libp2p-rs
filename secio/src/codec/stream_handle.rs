use futures::{
    channel::mpsc::{Receiver, Sender},
    stream::FusedStream,
    Stream,
};
use tokio::prelude::{AsyncRead, AsyncWrite};

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
// default yamux stream window size
const BUF_SHRINK_THRESHOLD: usize = 1024 * 256 * 20;

/// Stream handle
#[derive(Debug)]
pub struct StreamHandle {
    read_buf: Vec<u8>,

    frame_receiver: Receiver<StreamEvent>,

    event_sender: Sender<StreamEvent>,
}

impl StreamHandle {
    pub(crate) fn new(
        frame_receiver: Receiver<StreamEvent>,
        event_sender: Sender<StreamEvent>,
    ) -> Self {
        StreamHandle {
            frame_receiver,
            event_sender,
            read_buf: Vec::default(),
        }
    }

    fn handle_event(&mut self, event: StreamEvent) -> Result<(), io::Error> {
        match event {
            StreamEvent::Frame(mut frame) => self.read_buf.append(&mut frame),
            StreamEvent::Close => {
                if let Poll::Ready(Err(e)) = self.shutdown() {
                    return Err(e);
                }
            }
            _ => (),
        }
        Ok(())
    }

    /// Receive frames from secure stream
    fn recv_frames(&mut self, cx: &mut Context) -> Result<(), io::Error> {
        loop {
            if self.frame_receiver.is_terminated() {
                break;
            }
            match Pin::new(&mut self.frame_receiver).poll_next(cx) {
                Poll::Ready(Some(event)) => self.handle_event(event)?,
                Poll::Ready(None) => {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
                Poll::Pending => break,
            }
        }
        Ok(())
    }

    fn shutdown(&mut self) -> Poll<io::Result<()>> {
        if let Err(e) = self.event_sender.try_send(StreamEvent::Close) {
            if e.is_full() {
                return Poll::Pending;
            } else {
                return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
            }
        }
        Poll::Ready(Ok(()))
    }
}

impl AsyncRead for StreamHandle {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if let Err(e) = self.recv_frames(cx) {
            if self.read_buf.is_empty() {
                return Poll::Ready(Err(e));
            }
        }

        let n = ::std::cmp::min(buf.len(), self.read_buf.len());

        if n == 0 {
            return Poll::Pending;
        }

        buf[..n].copy_from_slice(&self.read_buf[..n]);
        self.read_buf.drain(..n);
        if self.read_buf.capacity() > BUF_SHRINK_THRESHOLD {
            self.read_buf.shrink_to_fit();
        }
        Poll::Ready(Ok(n))
    }
}

impl AsyncWrite for StreamHandle {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.recv_frames(cx)?;

        match self.event_sender.try_send(StreamEvent::Frame(buf.to_vec())) {
            Ok(_) => Poll::Ready(Ok(buf.len())),
            Err(e) => {
                if e.is_full() {
                    Poll::Pending
                } else {
                    Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.recv_frames(cx)?;

        match self.event_sender.try_send(StreamEvent::Flush) {
            Ok(_) => Poll::Ready(Ok(())),
            Err(e) => {
                if e.is_full() {
                    Poll::Pending
                } else {
                    Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
                }
            }
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context) -> Poll<io::Result<()>> {
        self.shutdown()
    }
}

#[derive(Debug)]
pub(crate) enum StreamEvent {
    Frame(Vec<u8>),
    Close,
    Flush,
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        // when handle drop, shutdown this stream
        if let Poll::Ready(Err(e)) = self.shutdown() {
            log::trace!("stream handle drop err: {}", e)
        }
        self.frame_receiver.close();
    }
}
