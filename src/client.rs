use crate::pb;
use crate::util;
use crate::constants;
use anyhow::Error;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use log::{debug, error, info};
use once_cell::sync::Lazy;
use pb::udp_service_client::UdpServiceClient;
use pb::UdpReq;
use pb::UdpRes;
use rustls::crypto::aws_lc_rs::default_provider;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use tokio::net::UdpSocket;
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;
use tokio::{io, spawn};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tonic::transport::{Channel, ClientTlsConfig, Uri};
use tonic::Request;

// 静态 TLS 配置，避免每次连接时重建 RootCertStore
static TLS_CONFIG: Lazy<rustls::ClientConfig> = Lazy::new(|| {
    let _ = default_provider().install_default();
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth()
});

async fn interruptible_recv(
    sock: &UdpSocket,
    buf: &mut [u8],
    cancel_token: &CancellationToken,
) -> Result<io::Result<(usize, SocketAddr)>, Error> {
    tokio::select! {
        result = sock.recv_from(buf) => Ok(result),
        _ = cancel_token.cancelled() => {
            Err(Error::from(io::Error::new(ErrorKind::Interrupted, "Operation cancelled")))
        },
    }
}

pub async fn start(
    l_addr: String,
    d_addr: String,
    auth: String,
    cancel_token: CancellationToken,
    lib: bool,
) -> util::Result<()> {
    let sock = UdpSocket::bind(&l_addr).await?;
    info!("udp Listening on {}", &l_addr);
    let sock = Arc::new(sock);

    let (tx, rx) = mpsc::channel::<UdpReq>(1024);
    let rx = tokio_stream::wrappers::ReceiverStream::new(rx);
    let rx = Request::new(rx);
    let uri = Uri::from_str(d_addr.as_str())?;
    let out_stream = if !lib || (&uri).scheme_str() != Some("https") {
        // let timeout = Duration::new(3, 0); // 设置超时时间为 3 秒
        let mut builder = Channel::builder(uri.clone());
        if (&uri).scheme_str() == Some("https") {
            // TLS_CONFIG 的 Lazy 初始化已包含 install_default()
            let _ = Lazy::force(&TLS_CONFIG);
            builder = builder.tls_config(ClientTlsConfig::new().with_enabled_roots())?;
        }
        // let channel = builder // 替换为您的 gRPC 服务器地址
        //     .connect_timeout(timeout) // 设置超时
        //     .connect()
        //     .await?;
        let channel_future = async { builder
            .connect()
            .await };
        let channel = timeout(constants::CLIENT_CONNECT_TIMEOUT, channel_future).await??;
        info!("grpc channel connected {}", &d_addr);
        let mut client = UdpServiceClient::new(channel);
        let connect_future = async { client.start_stream(rx).await };
        let stream = timeout(constants::CLIENT_CONNECT_TIMEOUT, connect_future).await??;
        Arc::new(Mutex::new(stream.into_inner()))
    } else {
        // 使用缓存的 TLS 配置
        let mut http = HttpConnector::new();
        http.enforce_http(false);

        // We have to do some wrapping here to map the request type from
        // `https://example.com` -> `https://[::1]:50051` because `rustls`
        // doesn't accept ip's as `ServerName`.
        let connector = tower::ServiceBuilder::new()
            .layer_fn(move |s| {
                let tls = TLS_CONFIG.clone();

                hyper_rustls::HttpsConnectorBuilder::new()
                    .with_tls_config(tls)
                    .https_or_http()
                    .enable_http2()
                    .wrap_connector(s)
            })
            .service(http);

        let client =
            hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(connector);
        let mut client = UdpServiceClient::with_origin(client, uri);
        let connect_future = async { client.start_stream(rx).await };
        let stream = timeout(constants::CLIENT_CONNECT_TIMEOUT, connect_future).await??;
        Arc::new(Mutex::new(stream.into_inner()))
        // Arc::new(Mutex::new(client.start_stream(rx).await?.into_inner()))
    };
    info!("grpc connected {}", &d_addr);
    let mut last_addr: Option<SocketAddr> = None;
    // 创建子 token 用于 reader 任务
    let reader_token = cancel_token.child_token();
    let mut reader_spawned = false;
    let mut buf = [0; 65536];
    while !cancel_token.is_cancelled() {
        match interruptible_recv(&sock, &mut buf, &cancel_token).await {
            Ok(Ok((size, addr))) => {
                debug!("Client UDP recv from {:?}, size: {}", &addr, size);

                if let Some(last) = last_addr {
                    if last != addr {
                        error!("Client UDP recv error addr {} vs {}", last, addr);
                        return Err(Error::new(io::Error::new(
                            ErrorKind::Other,
                            "Address changed unexpectedly",
                        )));
                    }
                } else {
                    last_addr = Some(addr);
                    if !reader_spawned {
                        info!("Client UDP from {:?}", &addr);
                        spawn_reader(
                            out_stream.clone(),
                            sock.clone(),
                            addr,
                            reader_token.clone(),
                        );
                        reader_spawned = true;
                    }
                }

                if let Err(e) = tx
                    .send(UdpReq {
                        auth: auth.clone(),
                        payload: buf[..size].to_vec(),
                    })
                    .await
                {
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
    }
    Ok(())
}

fn spawn_reader(
    out_stream: Arc<Mutex<tonic::Streaming<UdpRes>>>,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    cancel_token: CancellationToken,
) {
    spawn(async move {
        loop {
            // 使用 tokio::select! 在取消和读取之间选择
            let recv_result = {
                let mut stream = out_stream.lock().await;
                tokio::select! {
                    result = stream.next() => result,
                    _ = cancel_token.cancelled() => {
                        debug!("Client reader cancelled");
                        break;
                    }
                }
            };

            match recv_result {
                Some(Ok(v)) => {
                    if let Err(e) = sock.send_to(&v.payload, &addr).await {
                        error!("Client UDP send error to {:?}: {:?}", &addr, e);
                        break;
                    }
                }
                Some(Err(err)) => {
                    error!("Client reader next error: {:?}", err);
                    break;
                }
                None => {
                    debug!("Client reader stream ended");
                    break;
                }
            }
        }
        // 取消父 token，通知主循环退出
        cancel_token.cancel();
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
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug")
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
    let cancel_token = CancellationToken::new();
    let x = start(
        "127.0.0.1:50051".to_string(),
        "https://127.0.0.1:443".to_string(),
        "test".to_string(),
        cancel_token.clone(),
        false,
    )
    .await;
    match x {
        Err(e) => {
            error!("{:?}", e);
        }
        _ => {}
    }
    cancel_token.cancel();
    Ok(())
}
