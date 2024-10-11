use std::env;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use log::{debug, error};
use tokio::net::UdpSocket;
use tokio::spawn;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
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
        debug!("Stream started");
        let in_stream = request.into_inner();
        let (tx, rx) = mpsc::channel(1024);
        let key = Arc::new(self.key.clone());
        let d_addr = self.d_addr.clone();

        let socket = UdpSocket::bind("0.0.0.0:0").await.map_err(|e| {
            error!("Failed to bind socket: {:?}", e);
            Status::internal(format!("Socket bind error: {}", e))
        })?;

        socket.connect(&d_addr).await.map_err(|e| {
            error!("Server UDP connect error: {:?}", e);
            Status::internal(format!("UDP connect error: {}", e))
        })?;

        let socket = Arc::new(socket);
        let should_stop = Arc::new(AtomicBool::new(false));

        self.spawn_read_task(socket.clone(), tx.clone(), should_stop.clone());
        self.spawn_write_task(socket, in_stream, key, should_stop);

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

impl UogServer {
    pub fn new(d_addr: String, key: String) -> Self {
        UogServer { key, d_addr }
    }

    pub async fn bind(addr: String, d_addr: String, key: String) -> crate::Result<()> {
        let uog = UogServer::new(d_addr, key);
        let addr: SocketAddr = addr.parse()?;
        Server::builder()
            .add_service(pb::udp_service_server::UdpServiceServer::new(uog))
            .serve(addr).await?;
        Ok(())
    }

    fn spawn_read_task(&self, socket: Arc<UdpSocket>, tx: mpsc::Sender<Result<UdpRes, Status>>, should_stop: Arc<AtomicBool>) {
        tokio::spawn(async move {
            let mut buf = [0; 65536];
            while !should_stop.load(Ordering::Relaxed) {
                match timeout(Duration::from_secs(30), socket.recv_from(&mut buf)).await {
                    Ok(Ok((size, _))) => {
                        if tx.send(Ok(UdpRes { payload: buf[..size].to_vec() })).await.is_err() {
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        error!("Server UDP recv error: {}", e);
                        break;
                    }
                    Err(_) => continue,
                }
            }
            debug!("Stream write ended");
        });
    }

    fn spawn_write_task(&self, socket: Arc<UdpSocket>, mut in_stream: Streaming<UdpReq>, key: Arc<String>, should_stop: Arc<AtomicBool>) {
        tokio::spawn(async move {
            while let Some(result) = in_stream.next().await {
                match result {
                    Ok(v) if v.auth == *key => {
                        if let Err(e) = socket.send(&v.payload).await {
                            error!("Server UDP send error: {:?}", e);
                        }
                    }
                    Ok(_) => {
                        error!("Server auth failed");
                        break;
                    }
                    Err(e) => {
                        error!("In stream next error: {:?}", e);
                        break;
                    }
                }
            }
            should_stop.store(true, Ordering::Relaxed);
            debug!("Stream read ended");
        });
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
    sleep(std::time::Duration::from_secs(1)).await;
    UogServer::bind("127.0.0.1:50051".to_string(), "127.0.0.1:50051".to_string(), "test".to_string()).await?;

    Ok(())
}
