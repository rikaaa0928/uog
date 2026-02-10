mod client;
mod pb;
// mod server;
mod util;
pub mod constants;

#[cfg(target_os = "android")]
use jni::objects::{JClass, JObject};
#[cfg(target_os = "android")]
use jni::JNIEnv;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

uniffi::setup_scaffolding!();

#[derive(uniffi::Object)]
pub struct UogRust {
    cancel_token: CancellationToken,
}

#[uniffi::export]
impl UogRust {
    #[uniffi::constructor(name = "new")]
    pub fn new() -> Self {
        UogRust {
            cancel_token: CancellationToken::new(),
        }
    }

    pub fn client(&self, l_addr: &str, d_addr: &str, auth: &str) -> String {
        let rt = Runtime::new().unwrap();
        // 创建子 token，允许多次调用 client()
        loop {
            if self.cancel_token.is_cancelled() {
                break;
            }
            let child_token = self.cancel_token.child_token();
            let r = rt.block_on(client::start(
                l_addr.to_owned(),
                d_addr.to_owned(),
                auth.to_owned(),
                child_token.clone(),
                true,
                128,
            ));
            
            if self.cancel_token.is_cancelled() {
                break;
            }

            if r.is_err() {
                 rt.block_on(async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                });
            } else {
                 rt.block_on(async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                });
            }
        }
        return "".to_string();
    }

    pub fn stop(&self) {
        self.cancel_token.cancel();
    }

    /// Reset the cancellation token to allow reuse
    pub fn reset(&self) -> Self {
        UogRust {
            cancel_token: CancellationToken::new(),
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

// Android 平台现在使用 webpki-roots 提供的 Mozilla 根证书
// 不再需要通过 JNI 初始化系统证书验证器
#[allow(non_snake_case)]
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_moe_rikaaa0928_uot_Init_init(
    _env: JNIEnv,
    _class: JClass,
    _context: JObject,
) {
    // 保留此函数以保持 JNI 接口兼容性
    // 但不再需要初始化系统证书验证器
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread::{sleep, spawn};
    use std::time::Duration;

    #[test]
    fn client() {
        let c = Arc::new(UogRust::new());
        let client = c.clone();
        spawn(move || {
            sleep(Duration::from_secs(5));
            client.stop();
        });
        let x = c.client("127.0.0.1:50051", "https://127.0.0.1:443", "test");
        println!("{:#?}", x);
    }
}
