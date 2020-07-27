use super::Result;
use futures::{future::ok, TryFutureExt};
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::net::{TcpListener, TcpStream};

use crate::{
    error::TransportErrorKind,
    multiaddr::Multiaddr,
    transports::Transport,
    utils::{dns::DNSResolver, multiaddr_to_socketaddr, socketaddr_to_multiaddr},
};

/// Tcp listen bind
async fn bind(
    address: impl Future<Output = Result<Multiaddr>>,
) -> Result<(Multiaddr, TcpListener)> {
    let addr = address.await?;
    match multiaddr_to_socketaddr(&addr) {
        Some(socket_address) => {
            let tcp = TcpListener::bind(&socket_address)
                .await
                .map_err(TransportErrorKind::Io)?;
            let listen_addr =
                socketaddr_to_multiaddr(tcp.local_addr().map_err(TransportErrorKind::Io)?);

            Ok((listen_addr, tcp))
        }
        None => Err(TransportErrorKind::NotSupported(addr)),
    }
}

/// Tcp connect
async fn connect(
    address: impl Future<Output = Result<Multiaddr>>,
    timeout: Duration,
    original: Option<Multiaddr>,
) -> Result<(Multiaddr, TcpStream)> {
    let addr = address.await?;
    match multiaddr_to_socketaddr(&addr) {
        Some(socket_address) => {
            match tokio::time::timeout(timeout, TcpStream::connect(&socket_address)).await {
                Err(_) => Err(TransportErrorKind::Io(io::ErrorKind::TimedOut.into())),
                Ok(res) => Ok((
                    original.unwrap_or(addr),
                    res.map_err(TransportErrorKind::Io)?,
                )),
            }
        }
        None => Err(TransportErrorKind::NotSupported(original.unwrap_or(addr))),
    }
}

/// Tcp transport
#[derive(Default)]
pub struct TcpTransport {
    timeout: Duration,
}

impl TcpTransport {
    pub fn new(timeout: Duration) -> Self {
        TcpTransport { timeout }
    }
}

impl Transport for TcpTransport {
    type ListenFuture = TcpListenFuture;
    type DialFuture = TcpDialFuture;

    fn listen(self, address: Multiaddr) -> Result<Self::ListenFuture> {
        match DNSResolver::new(address.clone()) {
            Some(dns) => {
                let task = bind(dns.map_err(|(multiaddr, io_error)| {
                    TransportErrorKind::DNSResolverError(multiaddr, io_error)
                }));
                Ok(TcpListenFuture::new(task))
            }
            None => {
                let task = bind(ok(address));
                Ok(TcpListenFuture::new(task))
            }
        }
    }

    fn dial(self, address: Multiaddr) -> Result<Self::DialFuture> {
        match DNSResolver::new(address.clone()) {
            Some(dns) => {
                // Why do this?
                // Because here need to save the original address as an index to open the specified protocol.
                let task = connect(
                    dns.map_err(|(multiaddr, io_error)| {
                        TransportErrorKind::DNSResolverError(multiaddr, io_error)
                    }),
                    self.timeout,
                    Some(address),
                );
                Ok(TcpDialFuture::new(task))
            }
            None => {
                let dial = connect(ok(address), self.timeout, None);
                Ok(TcpDialFuture::new(dial))
            }
        }
    }
}

type TcpListenFutureInner = Pin<Box<dyn Future<Output = Result<(Multiaddr, TcpListener)>> + Send>>;

/// Tcp listen future
pub struct TcpListenFuture {
    executed: TcpListenFutureInner,
}

impl TcpListenFuture {
    fn new<T>(executed: T) -> Self
    where
        T: Future<Output = Result<(Multiaddr, TcpListener)>> + 'static + Send,
    {
        TcpListenFuture {
            executed: Box::pin(executed),
        }
    }
}

impl Future for TcpListenFuture {
    type Output = Result<(Multiaddr, TcpListener)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.executed.as_mut().poll(cx)
    }
}

type TcpDialFutureInner = Pin<Box<dyn Future<Output = Result<(Multiaddr, TcpStream)>> + Send>>;

/// Tcp dial future
pub struct TcpDialFuture {
    executed: TcpDialFutureInner,
}

impl TcpDialFuture {
    fn new<T>(executed: T) -> Self
    where
        T: Future<Output = Result<(Multiaddr, TcpStream)>> + 'static + Send,
    {
        TcpDialFuture {
            executed: Box::pin(executed),
        }
    }
}

impl Future for TcpDialFuture {
    type Output = Result<(Multiaddr, TcpStream)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.executed.as_mut().poll(cx)
    }
}
