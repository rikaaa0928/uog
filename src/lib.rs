mod client;
// mod server;
mod util;

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::runtime::Runtime;
use jni::JNIEnv;
use jni::objects::{JClass, JObject};


#[no_mangle]
pub fn Java_RustLibrary_startClient(l_addr: &str, d_addr: &str, auth: &str, stop_signal: *const AtomicBool) -> String {
    let stop_signal = unsafe { Arc::from_raw(stop_signal) };
    let rt = Runtime::new().unwrap();
    let r = rt.block_on(client::start(l_addr.to_owned(), d_addr.to_owned(), auth.to_owned(), stop_signal));
    if r.is_err() {
        let x = &r.err().unwrap();
        return l_addr.to_owned() + " : " + d_addr + " : " + auth + " : " + x.clone().to_string().as_str() + " : " + x.backtrace().to_string().as_str();
    } else {
        return "".to_string();
    }
}

#[no_mangle]
pub extern "C" fn Java_RustLibrary_createStopSignal() -> *const AtomicBool {
    Arc::into_raw(Arc::new(AtomicBool::new(false)))
}

#[no_mangle]
pub extern "C" fn Java_RustLibrary_setStopSignal(stop_signal: *const AtomicBool) {
    let stop_signal = unsafe { &*(stop_signal as *const AtomicBool) };
    stop_signal.store(true, Ordering::Relaxed);
}

// #[no_mangle]
// pub extern "C" fn start_server(l_addr: String, d_addr: String, auth: String) {
//     let rt = Runtime::new().unwrap();
//     let _ = rt.block_on(server::UogServer::bind(l_addr, d_addr, auth));
// }


#[allow(non_snake_case)]
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_moe_rikaaa0928_uot_Init_init(env: JNIEnv, _class: JClass, context: JObject) {
    // Then, initialize the certificate verifier for future use.
    let _ = rustls_platform_verifier::android::init_hosted(&env, context);
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client() {
        let x = start_client("127.0.0.1:50051", "https://127.0.0.1:443", "test");
        println!("{:#?}", x);
    }
}