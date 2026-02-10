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
        loop {
            if root_token.is_cancelled() {
                break;
            }
            let child_token = root_token.child_token();
            let result = client::start(
                src_opt.unwrap().to_string(),
                dst_opt.unwrap().to_string(),
                auth.unwrap().to_string(),
                child_token,
                false,
                buffer_size,
            )
            .await;

            if root_token.is_cancelled() {
                break;
            }

            if let Err(e) = result {
                log::error!("Client exited with error: {}, restarting in 1s...", e);
            } else {
                log::info!("Client exited normally, restarting in 1s...");
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }
    Ok(())
}
