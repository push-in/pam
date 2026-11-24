// Compiles the Zend-free C header reader for the `native_headers` and
// `http_request` targets, instrumented like the Rust code when cargo-fuzz
// builds with a sanitizer.
fn main() {
    println!("cargo:rerun-if-changed=native/header_walk.c");
    println!("cargo:rerun-if-changed=../native/pam_header_reader.h");
    let mut build = cc::Build::new();
    build.file("native/header_walk.c").opt_level(1).debug(true);
    let sanitizers = std::env::var("CARGO_CFG_SANITIZE").unwrap_or_default();
    if sanitizers.contains("address") {
        build.flag_if_supported("-fsanitize=address");
        build.flag_if_supported("-fsanitize=fuzzer-no-link");
    }
    build.compile("pam_fuzz_native");
}
