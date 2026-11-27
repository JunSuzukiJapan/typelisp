//! `armv6m` for an ARMv6-M core (`thumbv6m-*`, Cortex-M0 and M0+), whose
//! NVIC has 32 interrupts where an ARMv7-M or ARMv8-M one has up to 240:
//! nothing in the target's own `cfg` tells the two apart.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(armv6m)");
    if std::env::var("TARGET").is_ok_and(|t| t.starts_with("thumbv6m")) {
        println!("cargo:rustc-cfg=armv6m");
    }
}
