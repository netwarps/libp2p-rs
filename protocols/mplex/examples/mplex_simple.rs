use async_std::{
    net::{TcpListener, TcpStream},
    task,
};
use futures::TryStreamExt;
use libp2p_traits::{Read2, Write2};
use log::{error, info};
use mplex::connection::Connection;
use std::time::Duration;

fn main() {
    env_logger::init();
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
        let listener = TcpListener::bind("127.0.0.1:12345").await.unwrap();
        while let Ok((socket, _)) = listener.accept().await {
            task::spawn(async move {
                let mut muxer_conn = Connection::new(socket);
                let mut ctrl = muxer_conn.control();

                task::spawn(async {
                    let mut muxer_conn = muxer_conn;
                    while let Ok(_) = muxer_conn.next_stream().await {}
                    info!("connection is closed");
                });

                while let Ok(mut stream) = ctrl.accept_stream().await {
                    task::spawn(async move {
                        info!("accepted new stream: {:?}", stream);
                        let mut buf = [0; 4096];

                        loop {
                            let n = match stream.read2(&mut buf).await {
                                Ok(num) => num,
                                Err(e) => {
                                    error!("read failed: {:?}", e);
                                    return;
                                }
                            };
                            info!("read {:?}", &buf[..n]);
                            if let Err(e) = stream.write_all2(buf[..n].as_ref()).await {
                                error!("write failed: {:?}", e);
                                return;
                            };
                        }
                        // if let Err(e) = stream.close2().await {
                        //     error!("close failed: {:?}", e);
                        //     return;
                        // };
                    });
                }
            });
        }
    });
}

fn run_client() {
    task::block_on(async {
        let socket = TcpStream::connect("127.0.0.1:12345").await.unwrap();
        let muxer_conn = Connection::new(socket);

        let mut ctrl = muxer_conn.control();

        task::spawn(async {
            let mut muxer_conn = muxer_conn;
            while let Ok(_) = muxer_conn.next_stream().await {}
            info!("connection is closed");
        });

        let mut handles = Vec::new();
        for _ in 0..100 {
            let mut stream = ctrl.clone().open_stream().await.unwrap();
            let handle = task::spawn(async move {
                info!("C: opened new stream {}", stream.id());

                let data = b"hello world";

                stream.write_all2(data.as_ref()).await.unwrap();
                info!("C: {}: wrote {} bytes", stream.id(), data.len());

                let mut frame = vec![0; data.len()];
                stream.read_exact2(&mut frame).await.unwrap();
                info!("C: {}: read {:?}", stream.id(), &frame);
                // assert_eq!(&data[..], &frame[..]);

                stream.close2().await.expect("close stream");

                // wait for stream to send and recv close frame
                // task::sleep(Duration::from_secs(1)).await;
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await;
        }

        ctrl.close().await.expect("close connection");

        info!("shutdown is completed");
    });
}