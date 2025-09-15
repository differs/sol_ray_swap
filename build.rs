use std::io::Result;
fn main() -> Result<()> {
    // 告诉 cargo 一旦 proto 有变就重新编译
    println!("cargo:rerun-if-changed=proto/common.proto");
    println!("cargo:rerun-if-changed=proto/dex_trade_event.proto");

    prost_build::compile_protos(
        &["proto/common.proto", "proto/dex_trade_event.proto"],
        &["proto/"],          // import 搜索路径
    )?;
    println!("cargo:warning=OUT_DIR={}", std::env::var("OUT_DIR").unwrap());
    Ok(())
}