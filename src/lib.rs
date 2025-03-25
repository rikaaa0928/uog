mod client;
// mod server;
mod util;

use jni::objects::{JClass, JObject};
use jni::JNIEnv;
use std::future::Future;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::sleep;
use std::time::Duration;
use log::{error, info}; // Ensure info is imported
use serde::Serialize; // Import Serialize
use serde_json; // Import serde_json
use tokio::runtime::Runtime;

use tokio::sync::oneshot;
use tokio::sync::oneshot::{Receiver, Sender};

uniffi::setup_scaffolding!();

// Define the status enum - make it serializable
#[derive(Debug, Clone, Serialize, uniffi::Enum)]
pub enum ClientStatus {
    Idle,
    Running,
    Success,
    Error { msg: String },
}

#[derive(uniffi::Object)]
pub struct UogRust {
    interrupt_sender: Arc<Mutex<Option<Sender<()>>>>,
    interrupt_receiver: Arc<Mutex<Option<Receiver<()>>>>,
    running: AtomicBool,
    pause_state: Arc<(Mutex<bool>, Condvar)>,
    // Add fields for status and last error
    current_status: Arc<Mutex<ClientStatus>>,
    last_error: Arc<Mutex<Option<String>>>,
}

#[uniffi::export]
impl UogRust {
    #[uniffi::constructor(name = "new")]
    pub fn new() -> Self {
        let (interrupt_sender, interrupt_receiver) = oneshot::channel();
        UogRust {
            interrupt_sender: Arc::new(Mutex::new(Some(interrupt_sender))),
            interrupt_receiver: Arc::new(Mutex::new(Some(interrupt_receiver))),
            running: AtomicBool::new(false),
            pause_state: Arc::new((Mutex::new(false), Condvar::new())),
            // Initialize status fields
            current_status: Arc::new(Mutex::new(ClientStatus::Idle)),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&self, l_addr: &str, d_addr: &str, auth: &str) -> String {
        self.running
            .store(true, std::sync::atomic::Ordering::Relaxed);
        
        let pause_state = self.pause_state.clone();
        let status_lock = self.current_status.clone();
        let error_lock = self.last_error.clone();

        // Set initial status to Running when start is called
        *status_lock.lock().unwrap() = ClientStatus::Running;

        if let Ok(mut receiver_guard) = self.interrupt_receiver.lock() {
            // Use take() which returns Option<T>, consuming the value if Some
            if let Some(mut receiver) = receiver_guard.take() {
                 // Drop the guard early after taking the receiver
                drop(receiver_guard);

                loop {
                    let rt = Runtime::new().unwrap();
                    let running = self.running.load(std::sync::atomic::Ordering::Relaxed);
                    if !running {
                        // Set status to Idle when stopped externally
                        *status_lock.lock().unwrap() = ClientStatus::Idle;
                        info!("Client loop stopped externally.");
                        break "stopped externally".to_string();
                    }

                    // Set status to Running before each client::start call
                    *status_lock.lock().unwrap() = ClientStatus::Running;
                    info!("Calling client::start...");

                    let r = rt.block_on(client::start(
                        l_addr.to_owned(),
                        d_addr.to_owned(),
                        auth.to_owned(),
                        &mut receiver, // Pass the owned receiver
                        true, // Assuming keep_alive is intended
                    ));

                    // Update status and last_error based on the result
                    let mut status_guard = status_lock.lock().unwrap();
                    let mut error_guard = error_lock.lock().unwrap();
                    if r.is_err() {
                        let err = r.err().unwrap();
                        let err_msg = format!("{:?}", err);
                        println!("client::start error: {}", &err_msg); // Keep console log
                        *status_guard = ClientStatus::Error { msg: err_msg.clone() };
                        *error_guard = Some(err_msg);
                        info!("client::start finished with error.");
                        // Drop guards before potentially long operations (sleep, wait)
                        drop(status_guard);
                        drop(error_guard);
                        sleep(Duration::from_millis(3000));
                    } else {
                        *status_guard = ClientStatus::Success;
                        *error_guard = None; // Clear last error on success
                        info!("client::start finished successfully.");
                        // Drop guards before potentially long operations (wait)
                        drop(status_guard);
                        drop(error_guard);
                    }

                    // Check pause state after client::start finishes
                    let (lock, cvar) = &*pause_state;
                    let mut paused = lock.lock().unwrap();
                    while *paused {
                        info!("Client loop paused, waiting...");
                        paused = cvar.wait(paused).unwrap();
                        info!("Client loop resuming...");
                    }
                    // Loop continues automatically
                }
            } else {
                // Failed to take the receiver from the Option
                let err_msg = "Failed to take interrupt receiver (already taken or None)".to_string();
                error!("{}", err_msg);
                 *status_lock.lock().unwrap() = ClientStatus::Error { msg: err_msg.clone() };
                return err_msg;
            }
        } else {
            // Failed to lock the mutex containing the Option<Receiver>
            let err_msg = "Failed to lock interrupt receiver mutex".to_string();
            error!("{}", err_msg);
            *status_lock.lock().unwrap() = ClientStatus::Error { msg: err_msg.clone() };
            return err_msg;
        }
    }


    pub fn stop(&self) {
        if let Ok(mut sender) = self.interrupt_sender.lock() {
            if let Some(sender) = sender.take() {
                self.running
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                let _ = sender.send(());
            }
        }
    }

    /// Pauses the loop in the `start` method after the current `client::start` finishes.
    pub fn pause(&self) {
        let (lock, _cvar) = &*self.pause_state;
        let mut paused = lock.lock().unwrap();
        *paused = true;
        log::info!("UogRust paused");
    }

    /// Resumes the loop in the `start` method if it was paused.
    pub fn resume(&self) {
        let (lock, cvar) = &*self.pause_state;
        let mut paused = lock.lock().unwrap();
        *paused = false;
        // Notify the waiting thread in the start loop
        cvar.notify_one();
        info!("UogRust resumed");
    }

    /// Gets the current status and the last error message as a JSON string.
    pub fn get_status(&self) -> String {
        let status = self.current_status.lock().unwrap().clone();
        let last_error = self.last_error.lock().unwrap().clone();

        // Create a JSON value
        let status_json = serde_json::json!({
            "status": status,
            "last_error": last_error,
        });

        // Serialize to string, handle potential errors
        serde_json::to_string(&status_json).unwrap_or_else(|e| {
            error!("Failed to serialize status to JSON: {}", e);
            // Return a fallback JSON error string
            "{\"status\":\"Error\",\"last_error\":\"Failed to serialize status\"}".to_string()
        })
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
pub extern "system" fn Java_moe_rikaaa0928_uot_Init_init(
    env: JNIEnv,
    _class: JClass,
    context: JObject,
) {
    // Then, initialize the certificate verifier for future use.
    let _ = rustls_platform_verifier::android::init_hosted(&env, context);
}

#[cfg(test)]
mod tests {
    use std::env;
    use super::*;
    use std::thread::{sleep, spawn};
    use std::time::Duration;
    use env_logger;

    #[test]
    fn client_test() {
        env::set_var("RUST_LOG", "info");
        env_logger::init();
        let c = Arc::new(UogRust::new());
        let client_control = c.clone();
        let client_run = c.clone();
        let client_status_check = c.clone();

        // Control Thread
        let control_handle = spawn(move || {
            info!("Control thread started");

            // Check initial status (might be Running or Error quickly)
            sleep(Duration::from_millis(200)); // Give start loop time to begin
            let initial_status_json = client_status_check.get_status();
            info!("Initial Status JSON: {}", initial_status_json);
            // Basic check if it's valid JSON
            assert!(serde_json::from_str::<serde_json::Value>(&initial_status_json).is_ok());


            sleep(Duration::from_secs(5));
            info!("Pausing client...");
            client_control.pause();
            sleep(Duration::from_millis(500));
            let paused_status_json = client_status_check.get_status();
            info!("Status JSON after pause signal: {}", paused_status_json);
             assert!(serde_json::from_str::<serde_json::Value>(&paused_status_json).is_ok());

            sleep(Duration::from_secs(10));
            info!("Resuming client...");
            client_control.resume();
            sleep(Duration::from_millis(500));
             let resumed_status_json = client_status_check.get_status();
            info!("Status JSON after resume signal: {}", resumed_status_json);
            assert!(serde_json::from_str::<serde_json::Value>(&resumed_status_json).is_ok());


            sleep(Duration::from_secs(5));
            info!("Stopping client...");
            client_control.stop();
            info!("Stop signal sent");

             sleep(Duration::from_millis(500));
             let final_status_json = client_status_check.get_status();
             info!("Status JSON after stop signal: {}", final_status_json);
             let final_status_val: serde_json::Value = serde_json::from_str(&final_status_json).unwrap();
             // Assert final state is Idle
             assert_eq!(final_status_val["status"], "Idle");
        });

        // Run the client start method in the main thread
        info!("Starting client run loop...");
        let run_result =  c.start("127.0.0.1:50051", "https://127.0.0.1:443", "test");
        info!("Client run loop finished with result: {:?}", run_result);

        control_handle.join().unwrap(); // Wait for control thread to finish
        
        // Final check after everything stopped
        let final_status_main_json = c.get_status();
        info!("Final Status JSON (main thread): {}", final_status_main_json);
        let final_status_main_val: serde_json::Value = serde_json::from_str(&final_status_main_json).unwrap();
        assert_eq!(final_status_main_val["status"], "Idle");
    }
}
