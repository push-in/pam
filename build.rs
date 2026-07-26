use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn main() {
    println!("cargo:rerun-if-changed=native/pam_php.c");
    println!("cargo:rerun-if-changed=native/pam.h");
    println!("cargo:rerun-if-changed=runtime/bootstrap.php");
    println!("cargo:rerun-if-changed=runtime/laravel.php");
    println!("cargo:rerun-if-changed=runtime/composer_bootstrap.php");
    println!("cargo:rerun-if-changed=runtime/flight_recorder.php");
    println!("cargo:rerun-if-changed=runtime/workflows.php");
    println!("cargo:rerun-if-changed=runtime/contracts.php");
    println!("cargo:rerun-if-changed=runtime/cluster_services.php");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");
    println!("cargo:rerun-if-env-changed=PHP_CONFIG");
    println!("cargo:rerun-if-env-changed=PAM_PHP_LIB_DIR");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo did not set OUT_DIR"));
    let php_config = env::var_os("PHP_CONFIG").unwrap_or_else(|| "php-config".into());
    let includes = command_output(&php_config, &["--includes"]);

    compile_shim(&out_dir, &includes);
    archive_shim(&out_dir);

    let (php_lib_dir, php_lib_name) = find_php_embed_library();

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=pam_php");
    println!("cargo:rustc-link-search=native={}", php_lib_dir.display());
    println!("cargo:rustc-link-lib=dylib={php_lib_name}");

    // Release archives place the exact PHP Embed ABI beside the runtime in
    // ../lib. `$ORIGIN` is resolved by the ELF loader, so the package remains
    // relocatable and does not need to modify the host linker configuration.
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib");

    if php_lib_dir.ends_with(".pam-sdk/usr/lib") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", php_lib_dir.display());
    }
}

fn compile_shim(out_dir: &Path, includes: &str) {
    let compiler = env::var_os("CC").unwrap_or_else(|| "cc".into());
    let object = out_dir.join("pam_php.o");
    let mut command = Command::new(&compiler);

    command
        .arg("-std=c11")
        .arg("-fPIC")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg("-c")
        .arg("native/pam_php.c")
        .arg("-o")
        .arg(&object);

    command.args(includes.split_whitespace());
    run(&mut command, "compile the PHP Embed shim");
}

fn archive_shim(out_dir: &Path) {
    let archiver = env::var_os("AR").unwrap_or_else(|| "ar".into());
    let mut command = Command::new(&archiver);

    command
        .arg("crus")
        .arg(out_dir.join("libpam_php.a"))
        .arg(out_dir.join("pam_php.o"));

    run(&mut command, "archive the PHP Embed shim");
}

fn find_php_embed_library() -> (PathBuf, String) {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("Cargo did not set CARGO_MANIFEST_DIR"),
    );
    let mut candidates = Vec::new();

    if let Some(path) = env::var_os("PAM_PHP_LIB_DIR") {
        candidates.push(PathBuf::from(path));
    }

    candidates.extend([
        PathBuf::from("/usr/lib"),
        PathBuf::from("/usr/local/lib"),
        PathBuf::from("/usr/lib/x86_64-linux-gnu"),
        manifest_dir.join(".pam-sdk/usr/lib"),
    ]);

    for directory in candidates {
        if let Some(library) = embed_library_in(&directory) {
            let file_name = library
                .file_name()
                .and_then(OsStr::to_str)
                .expect("PHP Embed library name is not valid UTF-8");
            let link_name = file_name
                .strip_prefix("lib")
                .and_then(|name| name.strip_suffix(".so"))
                .expect("PHP Embed library must be named libphp*.so");

            return (directory, link_name.to_owned());
        }
    }

    panic!(
        "PHP Embed library not found. Install libphp-embed (for example, \
         `sudo apt-get install libphp8.4-embed`) or set PAM_PHP_LIB_DIR."
    );
}

fn embed_library_in(directory: &Path) -> Option<PathBuf> {
    let exact = directory.join("libphp.so");
    if exact.is_file() {
        return Some(exact);
    }

    let mut versioned = fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with("libphp") && name.ends_with(".so"))
        })
        .collect::<Vec<_>>();

    versioned.sort();
    versioned.pop()
}

fn command_output(program: &OsStr, args: &[&str]) -> String {
    let output = Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {program:?}: {error}"));

    ensure_success(program, &output);
    String::from_utf8(output.stdout)
        .unwrap_or_else(|error| panic!("{program:?} returned non-UTF-8 output: {error}"))
}

fn run(command: &mut Command, action: &str) {
    let debug_command = format!("{command:?}");
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("failed to {action} with {debug_command}: {error}"));

    if !output.status.success() {
        panic!(
            "failed to {action} with {debug_command}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn ensure_success(program: &OsStr, output: &Output) {
    if !output.status.success() {
        panic!(
            "{program:?} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
