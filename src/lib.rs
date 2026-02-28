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
use std::collections::VecDeque;
use std::time::{Instant, Duration};

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
        let servers: Vec<&str> = d_addr.split(',').collect();
        let secrets: Vec<&str> = auth.split(',').collect();

        if secrets.len() != 1 && secrets.len() != servers.len() {
            // panic!("The number of secrets must be 1 or equal to the number of servers.");
            return "The number of secrets must be 1 or equal to the number of servers.".to_string();
        }

        let rt = Runtime::new().unwrap();

        use std::sync::Arc;
        use tokio::net::UdpSocket;

        let sock_future = async {
            UdpSocket::bind(&l_addr).await
        };
        let sock = rt.block_on(sock_future).expect("Bind failed");
        log::info!("udp Listening on {}", &l_addr);
        let shared_sock = Arc::new(sock);

        // 创建子 token，允许多次调用 client()
        let mut restart_timestamps: VecDeque<Instant> = VecDeque::new();
        let mut server_index = 0;
        loop {
            if self.cancel_token.is_cancelled() {
                break;
            }

            let current_server = servers[server_index % servers.len()];
            let current_secret = if secrets.len() == 1 {
                secrets[0]
            } else {
                secrets[server_index % secrets.len()]
            };
            server_index = server_index.wrapping_add(1);

            let child_token = self.cancel_token.child_token();
            log::info!("Connecting to server: {}", current_server);
            let _r = rt.block_on(client::start(
                shared_sock.clone(),
                current_server.to_string(),
                current_secret.to_string(),
                child_token.clone(),
                true,
                128,
            ));
            
            if self.cancel_token.is_cancelled() {
                break;
            }

            // if r.is_err() {
            //      rt.block_on(async {
            //         // tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            //     });
            // } else {
            //      rt.block_on(async {
            //         // tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            //     });
            // }

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
