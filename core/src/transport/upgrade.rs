
//! Transport upgrader.
//!
// TODO: add example

use async_trait::async_trait;
use futures_timer::Delay;
use futures::{StreamExt, Stream, SinkExt, TryStreamExt};
use futures::prelude::*;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{time::Duration};
use libp2p_traits::Write2;
use futures::channel::mpsc;
use futures::select;
use pin_project::{pin_project, project};
use log::{trace};
use crate::{Multiaddr, Transport, transport::{TransportError}};
use crate::transport::TransportListener;
use crate::upgrade::Upgrader;
use futures::stream::FuturesUnordered;
use std::num::NonZeroUsize;
use smallvec::alloc::fmt::UpperExp;

//use crate::transport::security::SecurityUpgrader;


/// A `TransportUpgrade` is a `Transport` that wraps another `Transport` and adds
/// upgrade capabilities to all inbound and outbound connection attempts.
///
#[derive(Debug, Copy, Clone)]
pub struct TransportUpgrade<InnerTrans, S> {
    inner: InnerTrans,

    // protector: Option<TProtector>,
    up: S,
    // mux_up: Option<TMuxUpgrader>,
}

impl<InnerTrans, S> TransportUpgrade<InnerTrans, S>
where
    InnerTrans: Transport,
    S: Upgrader<InnerTrans::Output>,
{
    /// Wraps around a `Transport` to add upgrade capabilities.
    pub fn new(inner: InnerTrans, up: S) -> Self {
        TransportUpgrade {
            inner,
            up
        }
    }
}
/*
#[async_trait]
impl<T, InnerTrans, S, F, Fut> Transport for TransportUpgrade<InnerTrans, S>
where
    InnerTrans: Transport,
    InnerTrans::Listener: TransportListener + Send,
    //S: Upgrader<InnerTrans::Output> + Send + Clone,
    //F: FnMut(InnerTrans::Output) -> Fut,
    //Fut: Future<Output = Result<S::Output, TransportError>>,
{
    type Output = T;//S::Output;
    type Listener = ListenerUpgrade<InnerTrans::Listener, F, Fut>;

    fn listen_on(self, addr: Multiaddr) -> Result<Self::Listener, TransportError>
    where
        S: Upgrader<<<InnerTrans as Transport>::Listener as TransportListener>::Output> + Send + Clone,
        F: FnMut(<<InnerTrans as Transport>::Listener as TransportListener>::Output) -> Fut,
        Fut: Future<Output = Result<T, TransportError>>,
    {
        let inner_listener = self.inner.listen_on(addr)?;
        let listener = ListenerUpgrade::new(inner_listener, |s| {
            let up = self.up.clone();
            async move {
                up.upgrade_inbound(s).await
            }
        });

        Ok(listener)
    }

    async fn dial(self, addr: Multiaddr) -> Result<Self::Output, TransportError>
    where
        S: Upgrader<InnerTrans::Output> + Send + Clone,
        F: FnMut(InnerTrans::Output) -> Fut,
        Fut: Future<Output = Result<S::Output, TransportError>>,
    {
        let stream = self.inner.dial(addr).await?;
        let u = self.up.upgrade_outbound(stream).await?;
        Ok(u)
    }
}

#[pin_project]
pub struct ListenerUpgrade<InnerListener, F, Fut>
{
    #[pin]
    inner: InnerListener,
    //up: S,

    f: F,
    futures: FuturesUnordered<Fut>,
    limit: Option<NonZeroUsize>,
    // TODO: add threshold support here
}

impl<T, InnerListener, F, Fut> ListenerUpgrade<InnerListener, F, Fut>
where
    InnerListener: TransportListener + Send,
    F: FnMut(InnerListener::Output) -> Fut,
    Fut: Future<Output = Result<T, TransportError>> + Send,
    T: Send
{
    pub fn new(inner: InnerListener, f: F) -> Self {
        Self {
            inner,
            //up,
            f,
            futures: FuturesUnordered::new(),
            limit: NonZeroUsize::new(10),
        }
    }
}


#[async_trait]
impl<T, InnerListener, F, Fut> TransportListener for ListenerUpgrade<InnerListener, F, Fut>
where
    InnerListener: TransportListener + Send + Unpin,
    F: FnMut(InnerListener::Output) -> Fut + Send,
    Fut: Future<Output = Result<T, TransportError>> + Send,
    T: Send + 'static,
{
    type Output = T;

    async fn accept(&mut self) -> Result<Self::Output, TransportError> {

        // let mut stream = self.inner.accept().await?;
        //
        // trace!("got a new connection, upgrading...");
        //
        // let ss = self.up.clone().upgrade_inbound(stream).await?;
        //
        // futures_timer::Delay::new(Duration::from_secs(3)).await;
        // Ok(ss)

        // let mut tx = self.tx.clone();
        // let up = self.up.clone();

        //let mut cc = ;

        // loop {
        //     select! {
        //         c = self.inner.incoming().try_for_each_concurrent(20,|s| {
        //             let mut tx = tx.clone();
        //             let up = up.clone();
        //             async move {
        //                 trace!("connected first");
        //                 let ss = up.upgrade_inbound(s).await?;
        //                 futures_timer::Delay::new(Duration::from_secs(3)).await;
        //                 tx.send(ss).await;
        //                 trace!("send an upgrade");
        //                 Ok(())
        //         }}) => {
        //         },
        //         up = self.rx.next() => {
        //             let up = up.unwrap();
        //             return Ok(up);
        //         },
        //     };
        // }

        self.next().await.unwrap()





        //Err(TransportError::Internal)

    }

    fn multi_addr(&self) -> Multiaddr {
        self.inner.multi_addr()
    }
}


impl<T, InnerListener, F, Fut> Stream for ListenerUpgrade<InnerListener, F, Fut>
where
    InnerListener: TransportListener + Send,
    F: FnMut(InnerListener::Output) -> Fut,
    Fut: Future<Output = Result<T, TransportError>>,
{
    type Item = Result<T, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {

        let this = self.project();
        loop {
            let mut made_progress_this_iter = false;

            // Check if we've already created a number of futures greater than `limit`
            if this.limit.map(|limit| limit.get() > this.futures.len()).unwrap_or(true) {
                // let poll_res = match stream.as_mut().as_pin_mut() {
                //     Some(stream) => stream.try_poll_next(cx),
                //     None => Poll::Ready(None),
                // };
                let poll_res = this.listener.accept().try_poll_unpin(cx);

                let elem = match poll_res {
                    Poll::Ready(Ok(elem)) => {
                        made_progress_this_iter = true;
                        Some(elem)
                    },
                    // Poll::Ready(None) => {
                    //     stream.set(None);
                    //     None
                    // }
                    Poll::Pending => None,
                    Poll::Ready(Err(e)) => {
                        // Empty the stream and futures so that we know
                        // the future has completed.
                        // stream.set(None);
                        drop(std::mem::replace(this.futures, FuturesUnordered::new()));
                        return Poll::Ready(None);
                    }
                };

                if let Some(elem) = elem {
                    this.futures.push((this.f)(elem));
                }
            }

            match this.futures.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(s))) => {
                    made_progress_this_iter = true;
                    return Poll::Ready(Some(Ok(s)))
                },
                Poll::Ready(None) => {
                    // if stream.is_none() {
                    //     return Poll::Ready(Ok(()))
                    // }
                },
                Poll::Pending => {}
                Poll::Ready(Some(Err(e))) => {
                    // Empty the stream and futures so that we know
                    // the future has completed.
                    // stream.set(None);
                    drop(std::mem::replace(this.futures, FuturesUnordered::new()));
                    return Poll::Ready(Some(Err(e)));
                }
            }

            if !made_progress_this_iter {
                return Poll::Pending;
            }
        }
    }

}

*/


#[async_trait]
impl<InnerTrans, S> Transport for TransportUpgrade<InnerTrans, S>
where
    InnerTrans: Transport + Send,
    InnerTrans::Listener: TransportListener + Send,
    S: Upgrader<InnerTrans::Output> + Send,
{
    type Output = S::Output;
    type Listener = ListenerUpgrade<InnerTrans::Listener, S>;

    fn listen_on(self, addr: Multiaddr) -> Result<Self::Listener, TransportError>
    {
        let inner_listener = self.inner.listen_on(addr)?;
        let listener = ListenerUpgrade::new(inner_listener, self.up);

        Ok(listener)
    }

    async fn dial(self, addr: Multiaddr) -> Result<Self::Output, TransportError>
    {
        let stream = self.inner.dial(addr).await?;
        self.up.upgrade_outbound(stream).await
    }
}
pub struct ListenerUpgrade<InnerListener, S>
{
    inner: InnerListener,
    up: S,
    // TODO: add threshold support here
}

impl<InnerListener, S> ListenerUpgrade<InnerListener, S>
{
    pub fn new(inner: InnerListener, up: S) -> Self {
        Self {
            inner,
            up,
        }
    }
}

#[async_trait]
impl<InnerListener, S> TransportListener for ListenerUpgrade<InnerListener, S>
where
    InnerListener: TransportListener + Send,
    S: Upgrader<InnerListener::Output> + Send + Clone
{
    type Output = S::Output;

    async fn accept(&mut self) -> Result<Self::Output, TransportError> {

        let stream = self.inner.accept().await?;
        let up = self.up.clone();

        trace!("got a new connection, upgrading...");
        //futures_timer::Delay::new(Duration::from_secs(3)).await;
        up.upgrade_inbound(stream).await
    }

    fn multi_addr(&self) -> Multiaddr {
        self.inner.multi_addr()
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::memory::MemoryTransport;
    use crate::upgrade::dummy::DummyUpgrader;


    #[test]
    fn listen() {
        let transport = TransportUpgrade::new(MemoryTransport::default(), DummyUpgrader::new());
        let mut listener = transport.listen_on("/memory/1639174018481".parse().unwrap()).unwrap();

        futures::executor::block_on(async move {
            let s = listener.accept().await.unwrap();
        });
    }

    #[test]
    fn communicating_between_dialer_and_listener() {

        let msg = [1, 2, 3];

        // Setup listener.
        let rand_port = rand::random::<u64>().saturating_add(1);
        let t1_addr: Multiaddr = format!("/memory/{}", rand_port).parse().unwrap();
        let cloned_t1_addr = t1_addr.clone();

        let t1 = TransportUpgrade::new(MemoryTransport::default(), DummyUpgrader::new());

        let listener = async move {
            let mut listener = t1.listen_on(t1_addr.clone()).unwrap();

            // kingwel, TBD  upgrade memory transport

            // let upgrade = listener.filter_map(|ev| futures::future::ready(
            //     ListenerEvent::into_upgrade(ev.unwrap())
            // )).next().await.unwrap();

            // let mut socket = upgrade.0.await.unwrap();

            let mut socket = listener.accept().await.unwrap();

            let mut buf = [0; 3];
            socket.read_exact(&mut buf).await.unwrap();

            assert_eq!(buf, msg);
        };

        // Setup dialer.

        let t2 = TransportUpgrade::new(MemoryTransport::default(), DummyUpgrader::new());

        let dialer = async move {
            let mut socket = t2.dial(cloned_t1_addr).await.unwrap();
            socket.write_all(&msg).await.unwrap();
        };

        // Wait for both to finish.

        futures::executor::block_on(futures::future::join(listener, dialer));
    }
}

