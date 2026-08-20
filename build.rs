use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

enum PhpLibrary {
    Dynamic { directory: PathBuf, name: String },
    Static { archive: PathBuf },
}

fn main() {
    println!("cargo:rerun-if-changed=native/pam_php.c");
    println!("cargo:rerun-if-changed=native/resolver_shim.c");
    println!("cargo:rerun-if-changed=native/pam.h");
    println!("cargo:rerun-if-changed=runtime/bootstrap.php");
    println!("cargo:rerun-if-changed=runtime/redis.php");
    println!("cargo:rerun-if-changed=runtime/database.php");
    println!("cargo:rerun-if-changed=runtime/laravel.php");
    println!("cargo:rerun-if-changed=runtime/composer_bootstrap.php");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");
    println!("cargo:rerun-if-env-changed=PHP_CONFIG");
    println!("cargo:rerun-if-env-changed=PAM_PHP_INCLUDE_DIRS");
    println!("cargo:rerun-if-env-changed=PAM_PHP_LIB_DIR");
    println!("cargo:rerun-if-env-changed=PAM_UPDATE_SIGNING_IDENTITY_SHA256");
    println!("cargo:rerun-if-env-changed=PAM_UPDATE_NEXT_SIGNING_IDENTITY_SHA256");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo did not set OUT_DIR"));
    let target_os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo did not set target OS");
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let includes = php_includes(&target_os);

    let php_library = find_php_embed_library(&target_os);
    let static_linux = matches!(php_library, PhpLibrary::Static { .. }) && target_os == "linux";
    let msvc = target_env == "msvc";

    compile_shim(&out_dir, &includes, static_linux, msvc);
    archive_shim(&out_dir, static_linux, msvc);

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    if static_linux {
        // The resolver bridge is intentionally not referenced until libphp.a
        // is processed later, so retain both shim objects in full.
        println!("cargo:rustc-link-lib=static:+whole-archive=pam_php");
    } else {
        println!("cargo:rustc-link-lib=static=pam_php");
    }

    match php_library {
        PhpLibrary::Dynamic { directory, name } => link_dynamic_php(&directory, &name, &target_os),
        PhpLibrary::Static { archive } => link_static_php(&archive),
    }
}

fn link_dynamic_php(php_lib_dir: &Path, php_lib_name: &str, target_os: &str) {
    println!("cargo:rustc-link-search=native={}", php_lib_dir.display());
    println!("cargo:rustc-link-lib=dylib={php_lib_name}");

    // Release archives place the exact PHP Embed ABI beside the runtime. Keep
    // both ELF and Mach-O builds relocatable without modifying global loader
    // configuration on the user's machine.
    if target_os == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/../lib");
    } else if target_os == "linux" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib");
    }

    if target_os != "windows" && php_lib_dir.ends_with(".pam-sdk/usr/lib") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", php_lib_dir.display());
    }
}

fn link_static_php(php_archive: &Path) {
    let library_dir = php_archive
        .parent()
        .expect("static PHP archive must have a parent directory");
    let mut archives = fs::read_dir(library_dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", library_dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(OsStr::to_str) == Some("a"))
        .collect::<Vec<_>>();

    archives.sort();

    // PHP module and SAPI registration contains intentionally unreferenced
    // symbols, so the main archive must be retained in full.
    println!("cargo:rustc-link-arg=-Wl,--whole-archive");
    println!("cargo:rustc-link-arg={}", php_archive.display());
    println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");

    // ePHPm SDKs bundle their static dependencies. Keep them after libphp.a
    // so the archive linker can resolve PHP's references in order. Some of
    // those archives reference each other, requiring a linker group.
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    for archive in archives {
        println!("cargo:rustc-link-arg={}", archive.display());
    }
    println!("cargo:rustc-link-arg=-Wl,--end-group");

    if cfg!(target_os = "linux") {
        // Repeat the system runtime libraries after the static archives.
        // rustc's own copies occur earlier and --as-needed may discard them
        // before PHP introduces references to resolver, glibc, and libgcc.
        for library in ["resolv", "dl", "m", "pthread", "rt", "gcc", "gcc_s", "c"] {
            println!("cargo:rustc-link-arg=-l{library}");
        }
    }
}

fn compile_shim(out_dir: &Path, includes: &[String], static_linux: bool, msvc: bool) {
    let compiler = env::var_os("CC").unwrap_or_else(|| if msvc { "cl" } else { "cc" }.into());
    compile_c_object(
        &compiler,
        Path::new("native/pam_php.c"),
        &out_dir.join(if msvc { "pam_php.obj" } else { "pam_php.o" }),
        includes,
        msvc,
    );

    if static_linux {
        compile_c_object(
            &compiler,
            Path::new("native/resolver_shim.c"),
            &out_dir.join("resolver_shim.o"),
            &[],
            false,
        );
    }
}

fn compile_c_object(
    compiler: &OsStr,
    source: &Path,
    object: &Path,
    includes: &[String],
    msvc: bool,
) {
    let mut command = Command::new(compiler);
    if msvc {
        command.args(["/nologo", "/std:c11", "/W4", "/WX", "/c"]);
        for include in includes {
            command.arg(format!("/I{include}"));
        }
        command.arg(source).arg(format!("/Fo{}", object.display()));
    } else {
        command
            .args(["-std=c11", "-fPIC", "-Wall", "-Wextra", "-Werror", "-c"])
            .arg(source)
            .arg("-o")
            .arg(object)
            .args(includes);
    }
    run(&mut command, &format!("compile {}", source.display()));
}

fn archive_shim(out_dir: &Path, static_linux: bool, msvc: bool) {
    let archiver = env::var_os("AR").unwrap_or_else(|| if msvc { "lib" } else { "ar" }.into());
    let mut command = Command::new(&archiver);
    if msvc {
        command
            .args([
                "/nologo",
                &format!("/OUT:{}", out_dir.join("pam_php.lib").display()),
            ])
            .arg(out_dir.join("pam_php.obj"));
    } else {
        command
            .arg("crus")
            .arg(out_dir.join("libpam_php.a"))
            .arg(out_dir.join("pam_php.o"));
    }

    if static_linux {
        command.arg(out_dir.join("resolver_shim.o"));
    }

    run(&mut command, "archive the PHP Embed shim");
}

fn find_php_embed_library(target_os: &str) -> PhpLibrary {
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
        PathBuf::from("/opt/homebrew/lib"),
        PathBuf::from("/opt/homebrew/opt/php/lib"),
        PathBuf::from("/usr/local/opt/php/lib"),
        manifest_dir.join(".pam-sdk/usr/lib"),
    ]);

    for directory in candidates {
        if let Some(library) = embed_library_in(&directory, target_os) {
            let file_name = library
                .file_name()
                .and_then(OsStr::to_str)
                .expect("PHP Embed library name is not valid UTF-8");
            if file_name == "libphp.a" {
                return PhpLibrary::Static { archive: library };
            }

            let name = if target_os == "windows" {
                file_name.strip_suffix(".lib")
            } else {
                file_name.strip_prefix("lib").and_then(|name| {
                    name.strip_suffix(".so")
                        .or_else(|| name.strip_suffix(".dylib"))
                })
            }
            .expect("PHP Embed library has an unsupported name");

            return PhpLibrary::Dynamic {
                directory,
                name: name.to_owned(),
            };
        }
    }

    panic!(
        "PHP Embed library not found. Install libphp-embed or set \
         PAM_PHP_LIB_DIR to a directory containing libphp.a, libphp.so, libphp.dylib, or php*embed.lib."
    );
}

fn embed_library_in(directory: &Path, target_os: &str) -> Option<PathBuf> {
    if target_os == "windows" {
        let mut libraries = fs::read_dir(directory)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|name| name.starts_with("php") && name.ends_with("embed.lib"))
            })
            .collect::<Vec<_>>();
        libraries.sort();
        return libraries.pop();
    }
    let exact = directory.join("libphp.a");
    if exact.is_file() {
        return Some(exact);
    }
    let exact = directory.join("libphp.so");
    if exact.is_file() {
        return Some(exact);
    }
    let exact = directory.join("libphp.dylib");
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
                .is_some_and(|name| {
                    name.starts_with("libphp")
                        && (name.ends_with(".so") || name.ends_with(".dylib"))
                })
        })
        .collect::<Vec<_>>();

    versioned.sort();
    versioned.pop()
}

fn php_includes(target_os: &str) -> Vec<String> {
    if target_os == "windows" {
        let raw = env::var("PAM_PHP_INCLUDE_DIRS")
            .expect("PAM_PHP_INCLUDE_DIRS is required for a Windows PHP Embed build");
        let includes = env::split_paths(OsStr::new(&raw))
            .map(|path| path.to_string_lossy().into_owned())
            .filter(|path| !path.is_empty())
            .collect::<Vec<_>>();
        assert!(
            !includes.is_empty(),
            "PAM_PHP_INCLUDE_DIRS must not be empty"
        );
        includes
    } else {
        let php_config = env::var_os("PHP_CONFIG").unwrap_or_else(|| "php-config".into());
        command_output(&php_config, &["--includes"])
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    }
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
