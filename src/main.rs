use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod client;
mod server;
mod util;
use clap::{arg, command, Arg, ArgAction};
use std::env;
use tokio::sync::oneshot;

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
        .get_matches();
    let src_opt = matches.get_one::<String>("src");
    let dst_opt = matches.get_one::<String>("dst");
    let auth = matches.get_one::<String>("sec");
    let s_mod = matches.get_one::<bool>("server");
    if s_mod.is_some() && *s_mod.unwrap() {
        let _ = server::UogServer::bind(
            src_opt.unwrap().to_string(),
            dst_opt.unwrap().to_string(),
            auth.unwrap().to_string(),
        )
        .await?;
    } else {
        let (interrupter, mut interrupt_receiver) = oneshot::channel();
        let _ = client::start(
            src_opt.unwrap().to_string(),
            dst_opt.unwrap().to_string(),
            auth.unwrap().to_string(),
            &mut interrupt_receiver,
            false,
        )
        .await?;
        interrupter.send(()).unwrap();
    }
    Ok(())
}
