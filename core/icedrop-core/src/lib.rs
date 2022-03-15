#![feature(fn_traits)]
#![allow(dead_code)]

mod client;
mod endpoint;
mod handlers;
mod proto;
mod server;

pub use client::Client;
