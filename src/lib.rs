mod client;
// mod server;
mod util;

use std::future::Future;
use tokio::runtime::Runtime;

uniffi::include_scaffolding!("uog");

#[no_mangle]
pub fn start_client(l_addr: &str, d_addr: &str, auth: &str) -> String {
    let rt = Runtime::new().unwrap();
    let r = rt.block_on(client::start(l_addr.to_owned(), d_addr.to_owned(), auth.to_owned()));
    if r.is_err() {
        return r.err().unwrap().backtrace().to_string();
    } else {
        return "".to_string();
    }
}

// #[no_mangle]
// pub extern "C" fn start_server(l_addr: String, d_addr: String, auth: String) {
//     let rt = Runtime::new().unwrap();
//     let _ = rt.block_on(server::UogServer::bind(l_addr, d_addr, auth));
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client() {
        let x = start_client("127.0.0.1:50051", "https://uog.xiaomi.dad:444", "test");
        println!("{:#?}", x);
    }
}