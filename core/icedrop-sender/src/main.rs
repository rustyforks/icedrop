use std::sync::{Arc, Mutex};
use log::{info};
use env_logger::{self, Env};
use icedrop_core;

fn main() {
  env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

  // icedrop_core::receiver_server_main();
  let controller = Arc::new(Mutex::new(icedrop_core::SenderController::new()));
  let controller_clone = Arc::clone(&controller);
  std::thread::spawn(move || {
    let mut controller_guard = controller_clone.lock().unwrap();
    let rx = controller_guard.take_command_rx();
    drop(controller_guard);
    icedrop_core::sender_main("192.168.1.100:13997".to_owned(), rx);
  });

  let controller_guard = controller.lock().unwrap();
  controller_guard.dispatch_command(icedrop_core::SenderCommand::SendFile("/var/tmp/file_to_send".to_owned()));
  drop(controller_guard);

  loop {
    std::thread::sleep(std::time::Duration::from_secs(5));
  }
}