// Copyright 2018 Parity Technologies (UK) Ltd.
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

use async_trait::async_trait;
use libp2p_core::upgrade::{UpgradeInfo, ProtocolName};
use crate::protocol_handler::{ProtocolHandler, BoxHandler};
use crate::SwarmError;

/// Implementation of `ProtocolHandler` that doesn't handle anything.
#[derive(Clone, Default)]
pub struct DummyProtocolHandler {
}

impl DummyProtocolHandler {
    pub fn new() -> Self {
        DummyProtocolHandler {
        }
    }
}

impl UpgradeInfo for DummyProtocolHandler {
    type Info = &'static [u8];
    fn protocol_info(&self) -> Vec<Self::Info> {
        vec![b"/dummy/1.0.0", b"/dummy/2.0.0"]
    }
}

#[async_trait]
impl<C: Send + std::fmt::Debug + 'static> ProtocolHandler<C> for DummyProtocolHandler {

    async fn handle(&mut self, stream: C, info: <Self as UpgradeInfo>::Info) -> Result<(), SwarmError> {
        log::trace!("Dummy Protocol handling inbound {:?} {:?}", stream, info.protocol_name_str());
        Ok(())
    }
    fn box_clone(&self) -> BoxHandler<C> {
        Box::new(self.clone())
    }
}