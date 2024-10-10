use std::cell::RefCell;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::{env, process};
use std::rc::Rc;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use anyhow::Error;
use log::{debug, error};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::spawn;
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tonic::codegen::tokio_stream::Stream;
use pb::udp_service_client::UdpServiceClient;
use pb::UdpReq;

pub mod pb {
    tonic::include_proto!("dad.xiaomi.uog");
}

pub async fn start(l_addr: String, d_addr: String, auth: String) -> crate::Result<()> {
    let sock = UdpSocket::bind(l_addr).await?;
    let ur = Arc::new(sock);
    let uw = ur.clone();
    let (mut tx, mut rx) = mpsc::channel::<UdpReq>(1024);
    let rx = tokio_stream::wrappers::ReceiverStream::new(rx);
    let mut stream = pb::udp_service_client::UdpServiceClient::connect(d_addr.clone()).await?;
    let out_stream = Arc::new(Mutex::new(stream.start_stream(rx).await?.into_inner()));
    let mut last_addr: Option<SocketAddr> = None;

    let res = tx.send(UdpReq { auth: true, payload: auth.into_bytes() }).await;
    if res.is_err() {
        error!("client tcp auth write error {:?}",res.unwrap_err());
    }
    loop {
        let mut buf = [0; 65536];
        let (size, addr) = ur.recv_from(&mut buf).await?;
        let w_buf = &buf[..size];
        debug!("client udp recv {:?} {}",&addr,size);
        let mut reader = out_stream.clone();
        let uw = uw.clone();
        let last_addr_none = last_addr.is_none();
        if last_addr_none {
            last_addr.replace(addr);
            spawn(async move {
                let mut reader = reader.clone();
                while let Some(result) = reader.lock().await.next().await {
                    match result {
                        Ok(v) => {
                            let uw = uw.clone();
                            let res = uw.send_to(v.payload.as_slice(), &addr).await;
                            if res.is_err() {
                                error!("client udp {:?} send error {:?}", &addr,res.unwrap_err());
                                break;
                            }
                        }
                        Err(err) => {
                            error!("client reader next error {:?}",err)
                        }
                    }
                }
                process::exit(1);
            });
        } else if last_addr.clone().unwrap() != addr {
            error!("addr changed {:?} {:?}",&last_addr, &addr);
            return Err(Error::new(std::io::Error::new(ErrorKind::Other, "closed 2")));
        }
        {
            let res = tx.send(UdpReq { auth: false, payload: Vec::from(w_buf) }).await;
            if res.is_err() {
                error!("client tcp write error {:?}",res.unwrap_err());
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn client_test() -> Result<(), Box<dyn std::error::Error>> {
    // let mut client = UdpServiceClient::connect("http://[::1]:50051").await?;
    // let (tx, rx) = mpsc::channel::<UdpReq>(1024);
    // // let (tx, rx) = mpsc::unbounded_channel::<UdpReq>();
    // spawn(async move {
    //     let mut i = 0;
    //     loop {
    //         tx.send(UdpReq { auth: false, payload: format!("{}", i).into_bytes() }).await.expect("send failed");
    //         i += 1;
    //         if (i >= 10) {
    //             break;
    //         }
    //         sleep(Duration::from_secs(1));
    //     }
    //     drop(tx);
    // });
    // // let rx_w = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
    // let rx_w = tokio_stream::wrappers::ReceiverStream::new(rx);
    // let mut response = client.start_stream(rx_w).await?.into_inner();
    // while let Some(result) = response.next().await {
    //     match result {
    //         Ok(v) => {
    //             println!("Received: {:?}", v);
    //         }
    //         Err(err) => {
    //             eprintln!("{:?}", err);
    //         }
    //     }
    // }
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "debug")
    }
    env_logger::init();
    spawn(async {
        start("127.0.0.1:50050".to_string(), "http://127.0.0.1:50051".to_string(), "test".to_string()).await;
    });

    sleep(Duration::from_secs(1));
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    let res = socket.connect("127.0.0.1:50050").await;
    if res.is_err() {
        println!("server udp connect {}", res.err().unwrap());
        return Ok(());
    }
    socket.send("test".as_bytes()).await?;
    let mut read_buf = vec![0; 128];
    let (len, _) = socket.recv_from(&mut read_buf).await?;
    let read_buf = &read_buf[..len];
    let read_buf = String::from_utf8(read_buf.to_vec()).unwrap();
    println!("client udp recv {:?}", read_buf);
    Ok(())
}