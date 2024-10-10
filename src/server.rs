use std::env;
use std::error::Error;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::thread::sleep;
use log::error;
use serde::de::Unexpected::Option;
use tokio::net::UdpSocket;
use tokio::spawn;
use tokio::sync::mpsc;
use tonic::{transport::Server, Request, Response, Status, Streaming};
use tonic::codegen::tokio_stream::{Stream, StreamExt};
use tonic::codegen::tokio_stream::wrappers::ReceiverStream;
use pb::udp_service_server::UdpService;
use pb::{UdpReq, UdpRes};
pub mod pb {
    tonic::include_proto!("dad.xiaomi.uog");
}
type ResponseStream = Pin<Box<dyn Stream<Item=Result<UdpRes, Status>> + Send>>;
#[derive(Debug, Default)]
pub struct UogServer {
    key: String,
    d_addr: String,
}

#[tonic::async_trait]
impl UdpService for UogServer {
    type startStreamStream = ResponseStream;

    async fn start_stream(&self, request: Request<Streaming<UdpReq>>) -> Result<Response<Self::startStreamStream>, Status> {
        println!("\tstream started");
        let mut in_stream = request.into_inner();
        let (tx, mut rx) = mpsc::channel(1024);
        // to mapped version of `in_stream`.
        let key = self.key.clone();
        // let mut r = None;
        let mut w = None;
        let d_addr = self.d_addr.clone();
        spawn(async move {
            let mut authed = false;
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(v) => {
                        if !authed && !v.auth {
                            break;
                        }
                        if authed && v.auth {
                            break;
                        }
                        if !authed {
                            let remote_key_res = std::str::from_utf8(v.payload.as_slice());
                            if remote_key_res.is_err() {
                                error!("Failed to decode payload: {:?}", remote_key_res.err().unwrap());
                                break;
                            }
                            if key != remote_key_res.unwrap() {
                                error!("Failed to auth: {:?}", remote_key_res.err().unwrap());
                                break;
                            }
                            authed = true;
                            let socket_res = UdpSocket::bind("0.0.0.0:0").await;
                            if socket_res.is_err() {
                                error!("Failed to bind socket: {:?}", socket_res.err().unwrap());
                                break;
                            }
                            let socket = socket_res.unwrap();
                            let res = socket.connect(&d_addr).await;
                            if res.is_err() {
                                error!("server udp connect {}", res.err().unwrap());
                                break;
                            }
                            let r = Arc::new(socket);
                            w = Some(r.clone());
                            let tx = tx.clone();
                            spawn(async move {
                                let mut buf = [0; 65536];
                                loop {
                                    let res = r.recv_from(&mut buf).await;
                                    if res.is_err() {
                                        error!("server udp recv {}", res.err().unwrap());
                                        drop(tx);
                                        return;
                                    }
                                    let (size, _) = res.unwrap();
                                    let w_buf = &buf[..size];
                                    tx
                                        .send(Ok(UdpRes { payload: w_buf.to_vec() }))
                                        .await
                                        .expect("working rx");
                                }
                            });
                            continue;
                        } else {
                            if (w.is_none()) {
                                error!("server udp w is none");
                                break;
                            }
                            if (v.auth) {
                                error!("server udp auth after authed");
                                break;
                            }
                            let res = w.clone().unwrap().send(v.payload.as_slice()).await;
                            if res.is_err() {
                                error!("server udp send error {:?}", res.err().unwrap());
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("in stream next: {:?}", err);
                    }
                }
            }
            drop(tx);
            println!("\tstream ended");
        });

        // echo just write the same data that was received
        let out_stream = ReceiverStream::new(rx);

        Ok(Response::new(
            Box::pin(out_stream) as Self::startStreamStream
        ))
    }
}

impl UogServer {
    pub fn new(d_addr: String, key: String) -> Self {
        UogServer { key, d_addr }
    }
    pub async fn bind(addr: String, d_addr: String, key: String) -> Result<(), Box<dyn std::error::Error>> {
        let uog = UogServer::new(d_addr, key);
        let addr: SocketAddr = addr.parse()?;
        Server::builder()
            .add_service(pb::udp_service_server::UdpServiceServer::new(uog))
            .serve(addr).await?;
        Ok(())
    }
}

#[tokio::test]
async fn server_test() -> Result<(), Box<dyn std::error::Error>> {
    // let addr = "[::1]:50051".parse()?;
    // let greeter = UogServer::default();
    //
    // Server::builder()
    //     .add_service(pb::udp_service_server::UdpServiceServer::new(greeter))
    //     .serve(addr)
    //     .await?;

    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "debug")
    }
    env_logger::init();
    spawn(async {
        let res = UdpSocket::bind("127.0.0.1:50051").await.unwrap();
        let mut buf = [0; 65536];
        loop {
            let (n, addr) = res.recv_from(&mut buf).await.unwrap();
            println!("client udp recv {:?} {}", n, addr);
            let x = res.send_to("pong".as_bytes(), addr).await.unwrap();
            println!("client udp send {:?}", x);
        }
    });
    sleep(std::time::Duration::from_secs(1));
    UogServer::bind("127.0.0.1:50051".to_string(), "127.0.0.1:50051".to_string(),"test".to_string()).await?;

    Ok(())
}
