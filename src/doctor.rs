use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::php::PhpRuntime;
use crate::terminal::Terminal;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CliInfo {
    php_version: String,
    php_version_id: u32,
    zts: bool,
    debug: bool,
    integer_size: u8,
    ini_loaded: Option<String>,
    extensions: Vec<String>,
}

pub fn run(executable: &OsStr, target: &Path) -> Result<u8, String> {
    let mut runtime = PhpRuntime::initialize(executable, target, &[])?;
    let embed = runtime.runtime_info()?;
    let composer = runtime.composer().cloned();
    let mut failed = false;
    let ui = Terminal::stdout();

    println!("{}", ui.brand("PAM / SYSTEM DIAGNOSTICS"));
    println!("{}", ui.rule());
    println!(
        "{}",
        ui.status("ok", format!("PHP Embed version: {}", embed.php_version))
    );
    println!(
        "{}",
        ui.status("info", format!("Embed SAPI: {}", embed.sapi))
    );
    println!(
        "{}",
        ui.status("info", format!("Zend Engine: {}", embed.zend_version))
    );
    println!(
        "{}",
        ui.status(
            "info",
            format!(
                "Embed php.ini: {}",
                embed.ini_loaded.as_deref().unwrap_or("none")
            )
        )
    );
    println!(
        "{}",
        ui.status(
            "info",
            format!("Embed scanned INI files: {}", embed.ini_scanned.len())
        )
    );
    println!(
        "{}",
        ui.status(
            "info",
            format!(
                "Xdebug={} · OPcache={}",
                embed.xdebug_loaded, embed.opcache_loaded
            )
        )
    );

    match cli_info() {
        Ok(cli) => {
            optional_check(
                embed.php_version_id == cli.php_version_id,
                &format!(
                    "Optional CLI comparison: Embed {} / CLI {}",
                    embed.php_version, cli.php_version
                ),
            );
            optional_check(
                embed.zts == cli.zts,
                &format!(
                    "Optional CLI thread safety: Embed ZTS={} / CLI ZTS={}",
                    embed.zts, cli.zts
                ),
            );
            optional_check(
                embed.debug == cli.debug,
                &format!(
                    "Optional CLI debug build: Embed={} / CLI={}",
                    embed.debug, cli.debug
                ),
            );
            optional_check(
                embed.integer_size == cli.integer_size,
                &format!(
                    "Optional CLI integer size: Embed={} bytes / CLI={} bytes",
                    embed.integer_size, cli.integer_size
                ),
            );
            println!(
                "{}",
                ui.status(
                    "info",
                    format!(
                        "CLI php.ini: {}",
                        cli.ini_loaded.as_deref().unwrap_or("none")
                    )
                )
            );

            let embed_extensions = embed.extensions.iter().cloned().collect::<BTreeSet<_>>();
            let cli_extensions = cli.extensions.iter().cloned().collect::<BTreeSet<_>>();
            let missing = cli_extensions
                .difference(&embed_extensions)
                .cloned()
                .collect::<Vec<_>>();
            let extra = embed_extensions
                .difference(&cli_extensions)
                .cloned()
                .collect::<Vec<_>>();
            optional_check(
                missing.is_empty(),
                if missing.is_empty() {
                    "Optional CLI comparison: Embed covers every CLI extension".to_owned()
                } else {
                    format!(
                        "Optional CLI extensions missing in Embed: {}",
                        missing.join(", ")
                    )
                }
                .as_str(),
            );
            if !extra.is_empty() {
                println!(
                    "{}",
                    ui.status(
                        "info",
                        format!("Embed-only extensions: {}", extra.join(", "))
                    )
                );
            }
        }
        Err(error) => {
            println!(
                "{}",
                ui.status(
                    "info",
                    format!(
                        "PHP CLI comparison unavailable: {error}. Pam uses PHP Embed directly."
                    )
                )
            );
        }
    }

    match composer {
        Some(project) => {
            println!(
                "{}",
                ui.status("info", format!("Composer root: {}", project.root.display()))
            );
            println!(
                "{}",
                ui.status(
                    "info",
                    format!(
                        "Composer vendor directory: {}",
                        project.vendor_directory.display()
                    )
                )
            );
            check(
                project.autoload.is_file(),
                &format!("Composer autoload: {}", project.autoload.display()),
                &mut failed,
            );
            let generated_platform_check = project
                .vendor_directory
                .join("composer/platform_check.php")
                .is_file();
            check(
                embed.composer_autoloaded,
                if generated_platform_check {
                    "Composer autoloader and generated platform check loaded inside Embed"
                } else {
                    "Composer autoloader loaded inside Embed"
                },
                &mut failed,
            );
            if !generated_platform_check {
                println!("{}", ui.status("info", "Composer has no generated platform check; run `pam composer check-platform-reqs`."));
            }
        }
        None => println!("{}", ui.status("info", "Composer project: not found")),
    }

    Ok(if failed { 1 } else { 0 })
}

fn cli_info() -> Result<CliInfo, String> {
    let source = r#"
$extensions = get_loaded_extensions();
sort($extensions);
echo json_encode([
    'phpVersion' => PHP_VERSION,
    'phpVersionId' => PHP_VERSION_ID,
    'zts' => (bool) PHP_ZTS,
    'debug' => (bool) PHP_DEBUG,
    'integerSize' => PHP_INT_SIZE,
    'iniLoaded' => php_ini_loaded_file() ?: null,
    'extensions' => $extensions,
], JSON_THROW_ON_ERROR);
"#;
    let output = Command::new("php")
        // PAM's bundled PHP 8.5 configuration targets the embedded runtime.
        // Never leak it into an optional host-CLI comparison, which may use a
        // different PHP module ABI.
        .env_remove("PHPRC")
        .env("PHP_INI_SCAN_DIR", "")
        .arg("-r")
        .arg(source)
        .output()
        .map_err(|error| format!("cannot execute the PHP CLI: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "PHP CLI inspection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid information from PHP CLI: {error}"))
}

fn check(passed: bool, message: &str, failed: &mut bool) {
    let ui = Terminal::stdout();
    if passed {
        println!("{}", ui.status("ok", message));
    } else {
        println!("{}", ui.status("fail", message));
        *failed = true;
    }
}

fn optional_check(passed: bool, message: &str) {
    let ui = Terminal::stdout();
    if passed {
        println!("{}", ui.status("ok", message));
    } else {
        println!("{}", ui.status("warn", message));
    }
}
