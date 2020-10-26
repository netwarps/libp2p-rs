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

use async_std::{
    net::{TcpListener, TcpStream},
    task,
};
use libp2prs_traits::{ReadEx, WriteEx};
use libp2prs_yamux::{connection::Connection, connection::Mode, error::ConnectionError, Config};
use log::info;
use std::collections::VecDeque;
use std::sync::Arc;

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("info")).init();
    if std::env::args().nth(1) == Some("server".to_string()) {
        info!("Starting server ......");
        run_server();
    } else {
        info!("Starting client ......");
        run_client();
    }
}

fn run_server() {
    task::block_on(async {
        let listener = TcpListener::bind("127.0.0.1:8088").await.unwrap();
        while let Ok((socket, _)) = listener.accept().await {
            task::spawn(async move {
                let muxer_conn = Connection::new(socket, Config::default(), Mode::Server);
                let mut ctrl = muxer_conn.control();

                task::spawn(async {
                    let mut muxer_conn = muxer_conn;
                    let _ = muxer_conn.next_stream().await;
                    info!("connection is closed");
                });

                while let Ok(mut stream) = ctrl.accept_stream().await {
                    info!("accepted new stream: {:?}", stream);
                    task::spawn(async move {
                        let mut len = [0; 4];
                        stream.read_exact2(&mut len).await?;
                        let mut buf = vec![0; u32::from_be_bytes(len) as usize];
                        let _ = stream.read_exact2(&mut buf).await;
                        stream.write_all2(&buf).await?;
                        stream.close2().await?;
                        Ok::<(), ConnectionError>(())
                    });
                }
            });
        }
    });
}

fn run_client() {
    task::block_on(async {
        let socket = TcpStream::connect("127.0.0.1:8088").await.unwrap();
        let muxer_conn = Connection::new(socket, Config::default(), Mode::Client);

        let mut ctrl = muxer_conn.control();

        let loop_handle = task::spawn(async {
            let mut muxer_conn = muxer_conn;
            let _ = muxer_conn.next_stream().await;
            info!("connection is closed");
        });

        let mut handles = VecDeque::new();
        let data = Arc::new(vec![0x42; 100 * 1024]);
        for _ in 0..100 {
            let mut stream = ctrl.clone().open_stream().await.unwrap();
            let data = data.clone();
            info!("C: opened new stream {}", stream.id());
            let handle = task::spawn(async move {
                stream.write_all2(&(data.len() as u32).to_be_bytes()[..]).await.unwrap();

                stream.write_all2(data.as_ref()).await.unwrap();
                info!("C: {}: wrote {} bytes", stream.id(), data.len());

                let mut frame = vec![0; data.len()];
                stream.read_exact2(&mut frame).await.unwrap();
                assert_eq!(&data[..], &frame[..]);

                stream.close2().await.expect("close stream");
            });
            handles.push_back(handle);
        }

        for handle in handles {
            handle.await;
        }

        ctrl.close().await.expect("close connection");

        loop_handle.await;

        info!("shutdown is completed");
    });
}
