mod client;
// mod server;
mod util;

use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;
use jni::JNIEnv;
use jni::objects::{JClass, JObject};

use tokio::sync::oneshot;
use tokio::sync::oneshot::{Receiver, Sender};

uniffi::setup_scaffolding!();

#[derive(uniffi::Object)]
pub struct UogRust {
    interrupt_sender: Arc<Mutex<Option<Sender<()>>>>,
    interrupt_receiver: Arc<Mutex<Option<Receiver<()>>>>,
}

#[uniffi::export]
impl UogRust {
    #[uniffi::constructor(name = "new")]
    pub fn new() -> Self {
        let (interrupt_sender, interrupt_receiver) = oneshot::channel();
        UogRust {
            interrupt_sender: Arc::new(Mutex::new(Some(interrupt_sender))),
            interrupt_receiver: Arc::new(Mutex::new(Some(interrupt_receiver))),
        }
    }

    pub fn client(&self, l_addr: &str, d_addr: &str, auth: &str) -> String {
        let rt = Runtime::new().unwrap();
        if let Ok(mut receiver) = self.interrupt_receiver.lock() {
            if let Some(mut receiver) = receiver.take() {
                let r = rt.block_on(client::start(l_addr.to_owned(), d_addr.to_owned(), auth.to_owned(), &mut receiver));
                if r.is_err() {
                    let x = &r.err().unwrap();
                    return l_addr.to_owned() + " : " + d_addr + " : " + auth + " : " + x.clone().to_string().as_str() + " : " + x.backtrace().to_string().as_str();
                } else {
                    return "".to_string();
                }
            } else {
                return "receiver none".to_string();
            }
        } else {
            return "receiver locked".to_string();
        }
    }

    pub fn stop(&self) {
        if let Ok(mut sender) = self.interrupt_sender.lock() {
            if let Some(sender) = sender.take() {
                let _ = sender.send(());
            }
        }
    }
}

// #[uniffi::export]
// #[no_mangle]
// pub fn start_client(l_addr: &str, d_addr: &str, auth: &str) -> String {
//     let rt = Runtime::new().unwrap();
//     let r = rt.block_on(client::start(l_addr.to_owned(), d_addr.to_owned(), auth.to_owned()));
//     if r.is_err() {
//         let x = &r.err().unwrap();
//         return l_addr.to_owned() + " : " + d_addr + " : " + auth + " : " + x.clone().to_string().as_str() + " : " + x.backtrace().to_string().as_str();
//     } else {
//         return "".to_string();
//     }
// }

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
    use std::thread::{sleep, spawn};
    use std::time::Duration;
    use super::*;

    #[test]
    fn client() {
        let c = Arc::new(UogRust::new());
        let client = c.clone();
        spawn(move || {
            sleep(Duration::from_secs(5));
            client.stop();
        });
        let x = c.run("127.0.0.1:50051", "https://127.0.0.1:443", "test");
        println!("{:#?}", x);

    }
}