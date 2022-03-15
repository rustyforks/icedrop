extern crate cbindgen;

use std::env;
use std::path::Path;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir).join("../../..").join("icedrop.h");

    cbindgen::Builder::new()
        .with_crate(crate_dir)
        .with_language(cbindgen::Language::C)
        .with_style(cbindgen::Style::Both)
        .with_include_guard("ICEDROP_H")
        .with_autogen_warning("//\n// THIS IS A GENERATED FILE, DO NOT EDIT!!\n//")
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(out_path);
}
