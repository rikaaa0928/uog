mod client;
// mod server;
mod util;

use std::future::Future;
use tokio::runtime::Runtime;

#[no_mangle]
pub extern "C" fn start_client(l_addr: String, d_addr: String, auth: String) {
    let rt = Runtime::new().unwrap();
    let _ = rt.block_on(client::start(l_addr, d_addr, auth));
}

// #[no_mangle]
// pub extern "C" fn start_server(l_addr: String, d_addr: String, auth: String) {
//     let rt = Runtime::new().unwrap();
//     let _ = rt.block_on(server::UogServer::bind(l_addr, d_addr, auth));
// }
