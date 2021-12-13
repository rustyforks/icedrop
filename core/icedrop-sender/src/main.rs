use std::sync::{Arc, Mutex};
use log::{info};
use env_logger::{self, Env};
use icedrop_core;

fn main() {
  env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

  icedrop_core::receiver_server_main();
}