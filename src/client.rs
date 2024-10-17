use std::env;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{atomic, Arc};
use std::sync::atomic::AtomicBool;
use std::thread::sleep;
use std::time::Duration;
use anyhow::Error;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use log::{debug, error};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::crypto::aws_lc_rs::default_provider;
use tokio::net::UdpSocket;
use tokio::{spawn};
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tonic::client::GrpcService;
use tonic::transport::Uri;
use pb::udp_service_client::UdpServiceClient;
use pb::UdpReq;
use pb::UdpRes;
use crate::util;


pub mod pb {
    tonic::include_proto!("dad.xiaomi.uog");
}

pub async fn start(l_addr: String, d_addr: String, auth: String) -> util::Result<()> {
    let sock = UdpSocket::bind(l_addr).await?;
    let sock = Arc::new(sock);

    let (tx, rx) = mpsc::channel::<UdpReq>(1024);
    let rx = tokio_stream::wrappers::ReceiverStream::new(rx);

    let uri = Uri::from_str(d_addr.as_str())?;
    let out_stream = if uri.scheme_str() != Some("https") {
        let mut client = UdpServiceClient::connect(d_addr).await?;
        Arc::new(Mutex::new(client.start_stream(rx).await?.into_inner()))
    } else {
        let _ = default_provider().install_default();
        let tls = rustls_platform_verifier::tls_config();

        let mut http = HttpConnector::new();
        http.enforce_http(false);

        // We have to do some wrapping here to map the request type from
        // `https://example.com` -> `https://[::1]:50051` because `rustls`
        // doesn't accept ip's as `ServerName`.
        let connector = tower::ServiceBuilder::new()
            .layer_fn(move |s| {
                let tls = tls.clone();

                hyper_rustls::HttpsConnectorBuilder::new()
                    .with_tls_config(tls)
                    .https_or_http()
                    .enable_http2()
                    .wrap_connector(s)
            })
            .service(http);

        let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(connector);
        let mut client = UdpServiceClient::with_origin(client, uri);
        Arc::new(Mutex::new(client.start_stream(rx).await?.into_inner()))
    };

    // let mut client = UdpServiceClient::with_origin(client, uri);
    // let out_stream = Arc::new(Mutex::new(client.start_stream(rx).await?.into_inner()));

    let mut last_addr: Option<SocketAddr> = None;
    let should_stop = Arc::new(AtomicBool::new(false));
    let mut buf = [0; 65536];
    while !should_stop.clone().load(atomic::Ordering::Relaxed) {
        match timeout(Duration::from_secs(5), sock.recv_from(&mut buf)).await {
            Ok(Ok((size, addr))) => {
                debug!("Client UDP recv from {:?}, size: {}", &addr, size);

                if let Some(last) = last_addr {
                    if last != addr {
                        return Err(Error::new(std::io::Error::new(ErrorKind::Other, "Address changed unexpectedly")));
                    }
                } else {
                    last_addr = Some(addr);
                    spawn_reader(out_stream.clone(), sock.clone(), addr, should_stop.clone());
                }

                if let Err(e) = tx.send(UdpReq {
                    auth: auth.clone(),
                    payload: buf[..size].to_vec(),
                }).await {
                    error!("Client grpc write error: {:?}", e);
                    return Err(e.into());
                }
            }
            Ok(Err(e)) => {
                error!("Client UDP recv error: {}", e);
                return Err(e.into());
            }
            Err(_) => continue,
        }
        // let (size, addr) = sock.recv_from(&mut buf).await?;
        // debug!("Client UDP recv from {:?}, size: {}", &addr, size);
        //
        // if let Some(last) = last_addr {
        //     if last != addr {
        //         return Err(Error::new(std::io::Error::new(ErrorKind::Other, "Address changed unexpectedly")));
        //     }
        // } else {
        //     last_addr = Some(addr);
        //     spawn_reader(out_stream.clone(), sock.clone(), addr, should_stop.clone());
        // }
        //
        // if let Err(e) = tx.send(UdpReq {
        //     auth: auth.clone(),
        //     payload: buf[..size].to_vec(),
        // }).await {
        //     error!("Client TCP write error: {:?}", e);
        //     return Err(e.into());
        // }
    }
    Ok(())
}

fn spawn_reader(out_stream: Arc<Mutex<tonic::Streaming<UdpRes>>>, sock: Arc<UdpSocket>, addr: SocketAddr, should_stop: Arc<AtomicBool>) {
    spawn(async move {
        while let Some(result) = out_stream.lock().await.next().await {
            match result {
                Ok(v) => {
                    if let Err(e) = sock.send_to(&v.payload, &addr).await {
                        error!("Client UDP send error to {:?}: {:?}", &addr, e);
                        break;
                    }
                }
                Err(err) => {
                    error!("Client reader next error: {:?}", err);
                    break;
                }
            }
        }
        should_stop.store(true, std::sync::atomic::Ordering::Relaxed);
    });
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

    // spawn(async {
    //     start("127.0.0.1:50050".to_string(), "http://127.0.0.1:50051".to_string(), "test".to_string()).await;
    // });
    //
    // sleep(Duration::from_secs(1));
    // let socket = UdpSocket::bind("0.0.0.0:0").await?;
    // let res = socket.connect("127.0.0.1:50050").await;
    // if res.is_err() {
    //     println!("server udp connect {}", res.err().unwrap());
    //     return Ok(());
    // }
    // socket.send("test".as_bytes()).await?;
    // let mut read_buf = vec![0; 128];
    // let (len, _) = socket.recv_from(&mut read_buf).await?;
    // let read_buf = &read_buf[..len];
    // let read_buf = String::from_utf8(read_buf.to_vec()).unwrap();
    // println!("client udp recv {:?}", read_buf);

    let x = start("127.0.0.1:50051".to_string(), "https://127.0.0.1:443".to_string(), "test".to_string()).await;
    match x {
        Err(e) => {
            error!("{:?}", e);
        }
        _ => {}
    }
    Ok(())
}