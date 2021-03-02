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

//! Routing provides the capability of finding a peer with the given peer Id.
//!
//! The `Routing` traits is implemented on types that provide the find_peer
//! method..
//!

use async_trait::async_trait;

use crate::transport::TransportError;
use crate::{Multiaddr, PeerId};

/// `routing` trait for finding a peer.
#[async_trait]
pub trait Routing: Send {
    /// Retrieves the addresses of a remote peer.
    ///
    /// Any types supporting this trait can be used to search network for the
    /// addresses, f.g., Kad-DHT.
    async fn find_peer(&mut self, peer_id: &PeerId) -> Result<Vec<Multiaddr>, TransportError>;

    /// Retrieves the providers for the given key.
    async fn find_providers(&mut self, key: Vec<u8>, count: usize) -> Result<Vec<PeerId>, TransportError>;

    /// Starts announcing the given key to the content routing network.
    async fn provide(&mut self, key: Vec<u8>) -> Result<(), TransportError>;

    fn box_clone(&self) -> IRouting;
}

pub type IRouting = Box<dyn Routing>;

impl Clone for IRouting {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}
