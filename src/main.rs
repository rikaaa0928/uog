use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod client;
mod pb;
mod server;
mod util;
mod constants;
use clap::{arg, command, Arg, ArgAction};
use std::env;
use tokio_util::sync::CancellationToken;
use std::collections::VecDeque;
use std::time::{Instant, Duration};

#[tokio::main]
async fn main() -> util::Result<()> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info")
    }
    env_logger::init();
    let matches = command!()
        .arg(
            Arg::new("server")
                .short('s')
                .long("server")
                .action(ArgAction::SetTrue)
                .help("server mod"),
        )
        .arg(arg!([src] "addr src. client mod [udp-ip:udp-port]; server mod [grpc-ip:grpc-port]"))
        .arg(arg!([dst] "addr dst. client mod [grpc-endpoint]; server mod [udp-port]"))
        .arg(arg!([sec] "secret"))
        // .arg(arg!([bind] "port bind. client mod [remote-udp-port]; server mod none"))
        .arg(
            Arg::new("buffer_size")
                .short('b')
                .long("buffer-size")
                .help("Buffer size for the channel (default: 128)")
                .default_value("128"),
        )
        .get_matches();
    let src_opt = matches.get_one::<String>("src");
    let dst_opt = matches.get_one::<String>("dst");
    let auth = matches.get_one::<String>("sec");
    let s_mod = matches.get_one::<bool>("server");
    let buffer_size: usize = matches.get_one::<String>("buffer_size")
        .unwrap()
        .parse()
        .expect("Invalid buffer size");
    if s_mod.is_some() && *s_mod.unwrap() {
        let _ = server::UogServer::bind(
            src_opt.unwrap().to_string(),
            dst_opt.unwrap().to_string(),
            auth.unwrap().to_string(),
            buffer_size,
        )
        .await?;
    } else {
        let root_token = CancellationToken::new();
        let servers: Vec<&str> = dst_opt.unwrap().split(',').collect();
        let secrets: Vec<&str> = auth.unwrap().split(',').collect();

        if secrets.len() != 1 && secrets.len() != servers.len() {
            panic!("The number of secrets must be 1 or equal to the number of servers.");
        }

        let mut restart_timestamps: VecDeque<Instant> = VecDeque::new();
        use std::sync::Arc;
        use tokio::net::UdpSocket;

        let l_addr = src_opt.unwrap().to_string();
        let sock = UdpSocket::bind(&l_addr).await.expect("Bind failed");
        
        log::info!("udp Listening on {}", &l_addr);
        let shared_sock = Arc::new(sock);

        let mut server_index = 0;
        loop {
            if root_token.is_cancelled() {
                break;
            }

            let current_server = servers[server_index % servers.len()];
            let current_secret = if secrets.len() == 1 {
                secrets[0]
            } else {
                secrets[server_index % secrets.len()]
            };
            server_index = server_index.wrapping_add(1);

            let child_token = root_token.child_token();
            log::info!("Connecting to server: {}", current_server);
            let result = client::start(
                shared_sock.clone(),
                current_server.to_string(),
                current_secret.to_string(),
                child_token,
                false,
                buffer_size,
            )
            .await;

            if root_token.is_cancelled() {
                break;
            }

            if let Err(e) = result {
                log::error!("Client exited with error: {}", e);
            } else {
                log::info!("Client exited normally");
            }

            let now = Instant::now();
            restart_timestamps.push_back(now);
            while let Some(&t) = restart_timestamps.front() {
                if now.duration_since(t) > Duration::from_secs(3) {
                    restart_timestamps.pop_front();
                } else {
                    break;
                }
            }

            if restart_timestamps.len() >= 3 {
                log::info!("Restarting in 1s...");
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else {
                log::info!("Restarting immediately...");
            }
        }
    }
    Ok(())
}
