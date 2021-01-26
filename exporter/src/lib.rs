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

pub(crate) mod exporter;

use prometheus::{register, Encoder, TextEncoder};
use tide::{Body, Request, Response, Server};

use crate::exporter::Exporter;
use libp2prs_runtime::task;
use libp2prs_swarm::Control;

/// Exporter server
pub struct ExporterServer {
    s: Server<()>,
}

impl ExporterServer {
    pub fn new(control: Control) -> Self {
        let mut s = tide::new();

        // Register exporter to global registry, and then we can use default gather method.
        let exporter = Exporter::new(control);
        let _ = register(Box::new(exporter));
        s.at("/metrics").get(get_metric);
        ExporterServer { s }
    }

    pub fn start(self, addr: String) {
        task::spawn(async move {
            let r = self.s.listen(addr).await;
            log::info!("Exporter server started result={:?}", r);
        });
    }
}

/// Return metrics to prometheus
async fn get_metric(_: Request<()>) -> tide::Result {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];

    encoder.encode(&metric_families, &mut buffer).unwrap();

    let response = Response::builder(200)
        .content_type("text/plain; version=0.0.4")
        .body(Body::from(buffer))
        .build();

    Ok(response)
}
