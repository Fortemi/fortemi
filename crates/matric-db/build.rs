fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
    // cargo-llvm-cov passes --cfg=coverage; tests use it to bound instrumented cost.
    println!("cargo:rustc-check-cfg=cfg(coverage)");
}
