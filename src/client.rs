use crate::util;
use anyhow::Error;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use log::{debug, error, info};
use rustls::crypto::aws_lc_rs::default_provider;
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use std::env;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::oneshot::{Receiver, Sender};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tokio::{io, spawn};
use tokio_stream::StreamExt;
use tonic::client::GrpcService;
use tonic::transport::{Channel, ClientTlsConfig, Uri};
use tonic::Request;

pub mod pb {
    tonic::include_proto!("dad.xiaomi.uog");
}

use pb::udp_service_client::UdpServiceClient;
use pb::{UdpReq, UdpRes};

/// 客户端配置
#[derive(Debug, Clone)]
struct ClientConfig {
    /// 本地 UDP 地址
    local_addr: String,
    /// 远程 gRPC 服务地址
    remote_addr: String,
    /// 认证信息
    auth: String,
    /// 是否作为库使用
    is_lib: bool,
    /// 连接超时时间（秒）
    timeout_secs: u64,
    /// gRPC 通道缓冲区大小
    channel_buffer_size: usize,
}

impl ClientConfig {
    fn new(local_addr: String, remote_addr: String, auth: String, is_lib: bool) -> Self {
        Self {
            local_addr,
            remote_addr,
            auth,
            is_lib,
            timeout_secs: 3,
            channel_buffer_size: 1024,
        }
    }
}

/// 可中断的 UDP 接收操作
async fn interruptible_recv(
    sock: &UdpSocket,
    buf: &mut [u8],
    interrupt: &mut oneshot::Receiver<()>,
    global_int: &mut oneshot::Receiver<()>,
    stop: Arc<AtomicBool>,
) -> Result<io::Result<(usize, SocketAddr)>, Error> {
    tokio::select! {
        result = sock.recv_from(buf) => Ok(result),
        _ = interrupt => {
            Err(Error::from(io::Error::new(ErrorKind::Interrupted, "操作被本地中断")))
        },
        _ = global_int => {
            stop.store(true, Ordering::Relaxed);
            Err(Error::from(io::Error::new(ErrorKind::Interrupted, "操作被全局中断")))
        },
    }
}

/// 可中断的 gRPC 发送操作
async fn interruptible_send(
    tx: &mpsc::Sender<UdpReq>,
    data: UdpReq,
    interrupt: &mut oneshot::Receiver<()>,
    global_int: &mut oneshot::Receiver<()>,
) -> Result<Result<(), mpsc::error::SendError<UdpReq>>, Error> {
    tokio::select! {
        result = tx.send(data) => Ok(result),
        _ = interrupt => Err(Error::from(io::Error::new(ErrorKind::Interrupted, "操作被本地中断"))),
        _ = global_int => Err(Error::from(io::Error::new(ErrorKind::Interrupted, "操作被全局中断"))),
    }
}

/// 创建 gRPC 客户端流
async fn create_grpc_stream(
    config: &ClientConfig,
    request: Request<tokio_stream::wrappers::ReceiverStream<UdpReq>>,
) -> util::Result<Arc<Mutex<tonic::Streaming<UdpRes>>>> {
    let uri = Uri::from_str(&config.remote_addr)?;
    
    // 根据 URI 和配置选择合适的连接方式
    if !config.is_lib || uri.scheme_str() != Some("https") {
        create_standard_grpc_stream(&uri, request, config.timeout_secs).await
    } else {
        create_platform_verified_grpc_stream(&uri, request, config.timeout_secs).await
    }
}

/// 创建标准 gRPC 流
async fn create_standard_grpc_stream(
    uri: &Uri,
    request: Request<tokio_stream::wrappers::ReceiverStream<UdpReq>>,
    timeout_secs: u64,
) -> util::Result<Arc<Mutex<tonic::Streaming<UdpRes>>>> {
    let mut builder = Channel::builder(uri.clone());
    
    // 如果是 HTTPS，配置 TLS
    if uri.scheme_str() == Some("https") {
        let _ = default_provider().install_default();
        builder = builder.tls_config(ClientTlsConfig::new().with_enabled_roots())?;
    }
    
    // 连接到 gRPC 服务器，带超时
    let channel_future = async { builder.connect().await };
    let channel = timeout(Duration::from_secs(timeout_secs), channel_future).await??;
    
    info!("gRPC 通道已连接到 {}", uri);
    
    // 创建客户端并启动流
    let mut client = UdpServiceClient::new(channel);
    let connect_future = async { client.start_stream(request).await };
    let stream = timeout(Duration::from_secs(timeout_secs), connect_future).await??;
    
    Ok(Arc::new(Mutex::new(stream.into_inner())))
}

/// 创建使用平台验证器的 gRPC 流
async fn create_platform_verified_grpc_stream(
    uri: &Uri,
    request: Request<tokio_stream::wrappers::ReceiverStream<UdpReq>>,
    timeout_secs: u64,
) -> util::Result<Arc<Mutex<tonic::Streaming<UdpRes>>>> {
    // 初始化默认提供者
    let _ = default_provider().install_default();
    let tls = rustls_platform_verifier::tls_config();

    // 配置 HTTP 连接器
    let mut http = HttpConnector::new();
    http.enforce_http(false);

    // 创建 HTTPS 连接器
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

    // 创建 HTTP 客户端
    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        .build(connector);
    
    // 创建 gRPC 客户端并启动流
    let mut client = UdpServiceClient::with_origin(client, uri.clone());
    let connect_future = async { client.start_stream(request).await };
    let stream = timeout(Duration::from_secs(timeout_secs), connect_future).await??;
    
    Ok(Arc::new(Mutex::new(stream.into_inner())))
}

/// 启动 UDP 到 gRPC 的客户端
pub async fn start(
    l_addr: String,
    d_addr: String,
    auth: String,
    global_int: &mut Receiver<()>,
    lib: bool,
) -> util::Result<()> {
    // 创建客户端配置
    let config = ClientConfig::new(l_addr.clone(), d_addr.clone(), auth, lib);
    
    // 绑定 UDP 套接字
    let sock = UdpSocket::bind(&config.local_addr).await?;
    info!("UDP 监听在 {}", &config.local_addr);
    let sock = Arc::new(sock);

    // 创建 gRPC 请求通道
    let (tx, rx) = mpsc::channel::<UdpReq>(config.channel_buffer_size);
    let rx = tokio_stream::wrappers::ReceiverStream::new(rx);
    let request = Request::new(rx);
    
    // 创建 gRPC 流
    let out_stream = create_grpc_stream(&config, request).await?;
    info!("gRPC 已连接到 {}", &config.remote_addr);
    
    // 初始化状态变量
    let mut last_addr: Option<SocketAddr> = None;
    let should_stop = Arc::new(AtomicBool::new(false));
    let (interrupt_sender, mut interrupt_receiver) = oneshot::channel();
    let mut interrupt_sender = Some(interrupt_sender);
    let mut buf = [0; 65536];
    
    // 主循环
    while !should_stop.load(Ordering::Relaxed) {
        match interruptible_recv(
            &sock,
            &mut buf,
            &mut interrupt_receiver,
            global_int,
            should_stop.clone(),
        )
        .await
        {
            Ok(Ok((size, addr))) => {
                debug!("客户端 UDP 接收自 {:?}, 大小: {}", &addr, size);

                // 处理地址变更
                if let Some(last) = last_addr {
                    if last != addr {
                        error!("客户端 UDP 接收地址错误 {} vs {}", last, addr);
                        return Err(Error::new(io::Error::new(
                            ErrorKind::Other,
                            "地址意外变更",
                        )));
                    }
                } else {
                    // 首次接收，记录地址并启动读取器
                    last_addr = Some(addr);
                    if let Some(sender) = interrupt_sender.take() {
                        info!("客户端 UDP 来自 {:?}", &addr);
                        spawn_reader(
                            out_stream.clone(),
                            sock.clone(),
                            addr,
                            should_stop.clone(),
                            sender,
                        );
                    }
                }

                // 发送数据到 gRPC 流
                let udp_req = UdpReq {
                    auth: config.auth.clone(),
                    payload: buf[..size].to_vec(),
                };
                
                if let Err(e) = interruptible_send(&tx, udp_req, &mut interrupt_receiver, global_int).await {
                    error!("客户端 gRPC 写入错误: {:?}", e);
                    return Err(e);
                }
            }
            Ok(Err(e)) => {
                error!("客户端 UDP 接收错误: {}", e);
                return Err(e.into());
            }
            Err(e) => {
                // 忽略中断错误，继续循环或退出
                if should_stop.load(Ordering::Relaxed) {
                    break;
                }
                debug!("接收被中断: {:?}", e);
                continue;
            }
        }
    }
    
    Ok(())
}

/// 启动 gRPC 到 UDP 的读取器
fn spawn_reader(
    out_stream: Arc<Mutex<tonic::Streaming<UdpRes>>>,
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    should_stop: Arc<AtomicBool>,
    interrupt: Sender<()>,
) {
    spawn(async move {
        loop { // 使用显式循环以便在多个点检查和中断
            // 在等待下一个 gRPC 消息之前检查停止标志
            if should_stop.load(Ordering::Relaxed) {
                debug!("Reader loop stopping (flag checked before next()).");
                break;
            }

            // 从 gRPC 流获取下一条消息
            let next_message = out_stream.lock().await.next().await;

            match next_message {
                Some(Ok(v)) => {
                    // 在发送 UDP 包之前再次检查停止标志
                    if should_stop.load(Ordering::Relaxed) {
                        debug!("Reader loop stopping (flag checked before send_to).");
                        break;
                    }
                    // 发送 UDP 包
                    if let Err(e) = sock.send_to(&v.payload, &addr).await {
                        error!("客户端 UDP 发送错误到 {:?}: {:?}", &addr, e);
                        // 发送错误时中断循环
                        break;
                    }
                }
                Some(Err(err)) => {
                    // gRPC 流错误时中断循环
                    error!("客户端读取器错误: {:?}", err);
                    break;
                }
                None => {
                    // gRPC 流结束时中断循环
                    debug!("gRPC stream ended.");
                    break;
                }
            }
        }
        // 循环结束后，确保设置停止标志并通知主循环
        debug!("Reader loop finished.");
        should_stop.store(true, Ordering::Relaxed);
        let _ = interrupt.send(()); // 通知主循环此 reader 已结束
    });
}

#[tokio::test]
async fn client_test() -> Result<(), Box<dyn std::error::Error>> {
    // 设置日志级别
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();

    // 创建中断通道
    let (interrupter, mut interrupt_receiver) = oneshot::channel();
    
    // 启动客户端
    let result = start(
        "127.0.0.1:50051".to_string(),
        "https://127.0.0.1:443".to_string(),
        "test".to_string(),
        &mut interrupt_receiver,
        false,
    )
    .await;
    
    // 处理结果
    if let Err(e) = result {
        error!("客户端测试错误: {:?}", e);
    }
    
    // 发送中断信号
    let _ = interrupter.send(());
    
    Ok(())
}
