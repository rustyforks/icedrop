use env_logger::{self, Env};
use icedrop_core;

fn main() {
  env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

  icedrop_core::discovery_server_main();
}