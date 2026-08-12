//! Captures the compiling `rustc` version so the runtime `User-Agent` can report
//! it, mirroring alpaca-py's `APCA-PY/<sdk> Python/<runtime>` format.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rustc-check-cfg=cfg(docsrs)");

    let version = rustc_version::version()
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "unknown".to_owned());
    println!("cargo::rustc-env=ALPACA_RUSTC_VERSION={version}");
}
