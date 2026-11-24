use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::IsTerminal;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::admin_auth;
use crate::composer;
use crate::control_plane::ControlPlaneDiagnostics;
use crate::php::PhpRuntime;
use crate::project::ProjectKind;
use crate::terminal::Terminal;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn inspect(executable: &OsStr, script: &Path, arguments: &[OsString]) -> Result<u8, String> {
    let mut runtime = loaded_runtime(executable, script, arguments)?;
    let information = runtime.runtime_info()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&information)
            .map_err(|error| format!("cannot serialize runtime information: {error}"))?
    );
    Ok(0)
}

pub fn routes(executable: &OsStr, script: &Path, arguments: &[OsString]) -> Result<u8, String> {
    let mut runtime = loaded_runtime(executable, script, arguments)?;
    let routes = runtime.routes_info()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&routes)
            .map_err(|error| format!("cannot serialize route information: {error}"))?
    );
    Ok(0)
}

pub fn diagnostics(
    executable: &OsStr,
    script: &Path,
    arguments: &[OsString],
    section: Option<&str>,
) -> Result<u8, String> {
    let mut runtime = loaded_runtime(executable, script, arguments)?;
    let diagnostics = runtime.runtime_diagnostics()?;
    let output = section
        .and_then(|section| diagnostics.get(section))
        .unwrap_or(&diagnostics);
    println!(
        "{}",
        serde_json::to_string_pretty(output)
            .map_err(|error| format!("cannot serialize runtime diagnostics: {error}"))?
    );
    Ok(0)
}

pub fn top(
    address: &str,
    iterations: usize,
    interval: std::time::Duration,
    lag_warning: std::time::Duration,
    json: bool,
) -> Result<u8, String> {
    if iterations == 0 || interval.is_zero() || lag_warning.is_zero() {
        return Err("top iterations, interval, and lag warning must be positive".to_owned());
    }
    let endpoint = HttpEndpoint::parse(&format!(
        "{}/{}",
        address.trim_end_matches('/'),
        if json { "diagnostics" } else { "metrics" }
    ))?
    .with_optional_bearer_from_environment()?;
    let ui = Terminal::stdout();
    for iteration in 0..iterations {
        if iteration > 0 {
            std::thread::sleep(interval);
        }
        let body = endpoint.response_body()?;
        if json {
            let diagnostics = parse_control_plane_diagnostics(&body)?;
            let threshold_micros = u64::try_from(lag_warning.as_micros()).unwrap_or(u64::MAX);
            let warned_workers = diagnostics
                .workers
                .iter()
                .filter(|worker| worker.current_lag_micros >= threshold_micros)
                .count();
            let report = TopSampleReport {
                schema_version: 1,
                sample_index: iteration + 1,
                sample_count: iterations,
                sampled_at_unix_ms: unix_millis()?,
                result_code: if warned_workers == 0 {
                    TopResultCode::Healthy
                } else {
                    TopResultCode::Warning
                } as u8,
                lag_warning_millis: u64::try_from(lag_warning.as_millis()).unwrap_or(u64::MAX),
                warned_worker_count: warned_workers,
                diagnostics,
            };
            println!(
                "{}",
                serde_json::to_string(&report)
                    .map_err(|error| format!("cannot encode top sample: {error}"))?
            );
            std::io::stdout()
                .flush()
                .map_err(|error| format!("cannot flush top sample: {error}"))?;
            continue;
        }
        ui.clear_screen();
        println!(
            "{}  {}",
            ui.brand("PAM / LIVE TELEMETRY"),
            ui.muted(format!(
                "sample {:02}/{:02} · {}",
                iteration + 1,
                iterations,
                address
            ))
        );
        println!("{}", ui.rule());
        let lag_warning_seconds = lag_warning.as_secs_f64();
        let warned_workers = body
            .lines()
            .filter_map(current_worker_lag_seconds)
            .filter(|lag| *lag >= lag_warning_seconds)
            .count();
        if warned_workers == 0 {
            println!(
                "{}",
                ui.status(
                    "ok",
                    format!(
                        "no worker at or above {} ms current event-loop lag",
                        lag_warning.as_millis()
                    )
                )
            );
        } else {
            println!(
                "{}",
                ui.status(
                    "warn",
                    format!(
                        "{warned_workers} worker(s) at or above {} ms current lag; inspect worker, generation, PID, and pool",
                        lag_warning.as_millis()
                    )
                )
            );
        }
        for line in body.lines().filter(|line| visible_top_metric(line)) {
            if let Some((metric, value)) = line.split_once(' ') {
                let label = format!("{metric:<48}");
                if current_worker_lag_seconds(line).is_some_and(|lag| lag >= lag_warning_seconds) {
                    println!("  {} {} {}", ui.warning(label), value, ui.warning("[warn]"));
                } else {
                    println!("  {} {}", ui.accent(label), value);
                }
            } else {
                println!("  {}", ui.muted(line));
            }
        }
    }
    Ok(0)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum TopResultCode {
    Healthy = 1,
    Warning = 2,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TopSampleReport {
    schema_version: u8,
    sample_index: usize,
    sample_count: usize,
    sampled_at_unix_ms: u64,
    result_code: u8,
    lag_warning_millis: u64,
    warned_worker_count: usize,
    diagnostics: ControlPlaneDiagnostics,
}

fn parse_control_plane_diagnostics(body: &str) -> Result<ControlPlaneDiagnostics, String> {
    if body.len() > 1024 * 1024 {
        return Err("control-plane diagnostics exceed the 1 MiB limit".to_owned());
    }
    let diagnostics: ControlPlaneDiagnostics = serde_json::from_str(body)
        .map_err(|error| format!("invalid control-plane diagnostics: {error}"))?;
    diagnostics.validate()?;
    Ok(diagnostics)
}

fn unix_millis() -> Result<u64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_millis();
    u64::try_from(millis).map_err(|_| "Unix timestamp exceeds the supported range".to_owned())
}

fn current_worker_lag_seconds(line: &str) -> Option<f64> {
    let (metric, value) = line.split_once(' ')?;
    if !metric.starts_with("pam_worker_event_loop_lag_seconds{") {
        return None;
    }
    let value = value.parse::<f64>().ok()?;
    value
        .is_finite()
        .then_some(value)
        .filter(|value| *value >= 0.0)
}

fn visible_top_metric(line: &str) -> bool {
    [
        "pam_http_",
        "pam_websocket_",
        "pam_event_loop_",
        "pam_process_",
        "pam_php_",
        "pam_cluster_",
        "pam_pool_",
        "pam_worker_",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

pub fn test(executable: &OsStr, target: &Path, arguments: Vec<OsString>) -> Result<u8, String> {
    let project = composer::discover(target)?
        .ok_or_else(|| format!("no composer.json found from {}", target.display()))?;
    let force_phpunit = arguments.iter().any(|argument| argument == "--phpunit");
    let force_pest = arguments.iter().any(|argument| argument == "--pest");
    if force_phpunit && force_pest {
        return Err("choose only one of --pest or --phpunit".to_owned());
    }
    let mut arguments = arguments
        .into_iter()
        .filter(|argument| argument != "--phpunit" && argument != "--pest")
        .collect::<Vec<_>>();
    let pest = project.vendor_directory.join("pestphp/pest/bin/pest");
    let phpunit = project.vendor_directory.join("phpunit/phpunit/phpunit");
    let using_pest = !force_phpunit && (force_pest || pest.is_file());
    let runner = if force_phpunit {
        phpunit
    } else if using_pest {
        pest
    } else {
        phpunit
    };
    if !runner.is_file() {
        return Err(format!(
            "no test runner found in {}; install Pest or PHPUnit with Composer",
            project.vendor_directory.display()
        ));
    }
    if using_pest
        && !arguments
            .iter()
            .any(|argument| argument == "--fail-on-empty-test-suite")
    {
        arguments.push(OsString::from("--fail-on-empty-test-suite"));
    }

    std::env::set_current_dir(&project.root).map_err(|error| {
        format!(
            "cannot enter Composer project {}: {error}",
            project.root.display()
        )
    })?;
    let mut runtime = PhpRuntime::initialize(executable, &runner, &arguments)?;
    let status = runtime.execute_file(&runner)?;
    if using_pest && status == 0 {
        println!("Pest completed successfully inside the Pam Embed SAPI.");
    }
    Ok(status)
}

/// `pam views:check`: compiles every `resources/views/**/*.nim.php` with the
/// project's `nimbus-views` and returns its exit code (1 when a template fails).
pub fn views_check(
    executable: &OsStr,
    target: &Path,
    arguments: Vec<OsString>,
) -> Result<u8, String> {
    let project = composer::discover(target)?
        .ok_or_else(|| format!("no composer.json found from {}", target.display()))?;
    let checker = project
        .vendor_directory
        .join("pushinbr/nimbus/bin/nimbus-views");
    if !checker.is_file() {
        return Err(format!(
            "{} is missing; views:check needs pushinbr/nimbus 0.3 or newer installed with `pam composer install`",
            checker.display()
        ));
    }
    std::env::set_current_dir(&project.root).map_err(|error| {
        format!(
            "cannot enter Composer project {}: {error}",
            project.root.display()
        )
    })?;
    let mut checker_arguments = vec![OsString::from("check")];
    checker_arguments.extend(arguments);
    let mut runtime = PhpRuntime::initialize(executable, &checker, &checker_arguments)?;
    runtime.execute_file(&checker)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitTemplate {
    Raw,
    Api,
    Web,
    Laravel,
    Desktop,
    Mobile,
    MobileUi,
    Product,
}

impl InitTemplate {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "raw" | "pure" => Ok(Self::Raw),
            "http" | "api" => Ok(Self::Api),
            "web" => Ok(Self::Web),
            "laravel" => Ok(Self::Laravel),
            "desktop" => Ok(Self::Desktop),
            "native" | "mobile" | "android" | "mobile-pure" => Ok(Self::Mobile),
            "native-ui" | "mobile-ui" | "android-ui" | "mobile+ui" => Ok(Self::MobileUi),
            "product" | "fullstack" | "flagship" => Ok(Self::Product),
            _ => Err(format!(
                "unknown init template {value:?}; expected raw, api, web, laravel, desktop, native, native-ui, or product"
            )),
        }
    }
}

#[derive(Debug)]
pub struct InitOptions {
    pub directory: PathBuf,
    pub template: Option<InitTemplate>,
    pub socket: bool,
    pub install: bool,
    pub interaction: bool,
    pub application_id: Option<String>,
    pub application_name: Option<String>,
    pub mobile_starter: Option<MobileStarter>,
    pub mobile_platforms: Vec<MobilePlatform>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MobileStarter {
    Blank = 1,
    Tabs = 2,
    Authentication = 3,
    Ecommerce = 4,
    Chat = 5,
    Showcase = 6,
}

impl MobileStarter {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "blank" => Ok(Self::Blank),
            "tabs" => Ok(Self::Tabs),
            "auth" | "authentication" => Ok(Self::Authentication),
            "ecommerce" | "commerce" => Ok(Self::Ecommerce),
            "chat" => Ok(Self::Chat),
            "showcase" => Ok(Self::Showcase),
            _ => Err("starter requires blank, tabs, auth, ecommerce, chat, or showcase".to_owned()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MobilePlatform {
    Android = 1,
    Ios = 2,
    /// Android TV, Google TV and Fire TV: the same APK with a TV launcher
    /// entry and D-pad focus.
    AndroidTv = 3,
}

impl MobilePlatform {
    /// `android`, `ios`, `android-tv` (or `tv`), `all` (phones) or a comma
    /// list such as `android,ios,android-tv`.
    pub fn parse(value: &str) -> Result<Vec<Self>, String> {
        let mut platforms = Vec::new();
        for part in value
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let parsed: &[Self] = match part {
                "android" => &[Self::Android],
                "ios" => &[Self::Ios],
                "android-tv" | "tv" => &[Self::AndroidTv],
                "all" | "both" => &[Self::Android, Self::Ios],
                _ => {
                    return Err(
                        "platform requires android, ios, android-tv, all, or a comma list"
                            .to_owned(),
                    );
                }
            };
            for platform in parsed {
                if !platforms.contains(platform) {
                    platforms.push(*platform);
                }
            }
        }
        if platforms.is_empty() {
            return Err(
                "platform requires android, ios, android-tv, all, or a comma list".to_owned(),
            );
        }
        Ok(platforms)
    }

    fn target(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
            Self::AndroidTv => "android-tv",
        }
    }
}

pub fn init(executable: &OsStr, mut options: InitOptions) -> Result<u8, String> {
    let (template, socket) = choose_template(
        options.template,
        options.socket,
        options.interaction && std::io::stdin().is_terminal(),
    )?;
    options.template = Some(template);
    options.socket = socket;
    if matches!(
        template,
        InitTemplate::Mobile | InitTemplate::MobileUi | InitTemplate::Product
    ) {
        configure_mobile_options(&mut options)?;
    }

    if matches!(
        template,
        InitTemplate::Desktop
            | InitTemplate::Mobile
            | InitTemplate::MobileUi
            | InitTemplate::Product
    ) && socket
    {
        return Err(
            "the desktop and mobile presets do not use --socket; they expose their own native event systems".to_owned(),
        );
    }
    if template == InitTemplate::Web && socket {
        return Err(
            "the web preset does not use --socket; add pushinbr/pam-socket to it with Composer"
                .to_owned(),
        );
    }
    if template == InitTemplate::Laravel {
        return init_laravel(executable, &options);
    }

    let directory = &options.directory;
    if directory.exists() && !only_version_control(directory)? {
        return Err(format!("{} is not empty", directory.display()));
    }
    let created = !directory.exists();
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    let result = init_project(executable, directory, template, socket, &options);
    if let Err(error) = result {
        // A half-written project would make the retry fail with "not empty".
        if created && fs::remove_dir_all(directory).is_ok() {
            return Err(format!(
                "{error}; the partial project {} was removed, so the same command can be run again",
                directory.display()
            ));
        }
        return Err(error);
    }
    print_init_success(directory, template, socket, options.install);
    Ok(0)
}

fn init_project(
    executable: &OsStr,
    directory: &Path,
    template: InitTemplate,
    socket: bool,
    options: &InitOptions,
) -> Result<(), String> {
    if template == InitTemplate::Raw {
        init_raw(directory, socket)?;
    } else if template == InitTemplate::Desktop {
        init_desktop(directory, options.application_name.as_deref())?;
    } else if template == InitTemplate::Product {
        init_product(directory, options)?;
    } else if matches!(template, InitTemplate::Mobile | InitTemplate::MobileUi) {
        init_mobile(directory, template == InitTemplate::MobileUi, options)?;
    } else if template == InitTemplate::Web {
        init_web(directory)?;
    } else {
        init_api(directory, socket)?;
    }
    write_pam_manifest(directory, template, options)?;

    if options.install {
        if template == InitTemplate::Product {
            for application in ["apps/server", "apps/native", "apps/desktop"] {
                run_composer_in(
                    executable,
                    &directory.join(application),
                    &["install", "--no-interaction"],
                )?;
            }
        } else if directory.join("composer.json").is_file() {
            run_composer_in(executable, directory, &["install", "--no-interaction"])?;
        }
        if template == InitTemplate::MobileUi {
            copy_native_ui_fonts(directory)?;
        }
        if template == InitTemplate::Desktop {
            run_desktop_init(executable, directory, options)?;
        }
    }
    Ok(())
}

/// PAM Native UI renders with Instrument Sans: its `@font-face` rules read
/// `assets/fonts/InstrumentSans-*.ttf` from the application, so a new project
/// gets the files the installed package ships. Existing files are kept.
fn copy_native_ui_fonts(directory: &Path) -> Result<(), String> {
    let source = directory.join("vendor/pushinbr/pam-native-ui/resources/fonts");
    let Ok(entries) = fs::read_dir(&source) else {
        return Ok(());
    };
    let target = directory.join("assets/fonts");
    fs::create_dir_all(&target)
        .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_font = path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ttf"));
        let Some(name) = path.file_name() else {
            continue;
        };
        let destination = target.join(name);
        if is_font && !destination.exists() {
            fs::copy(&path, &destination).map_err(|error| {
                format!(
                    "cannot copy {} to {}: {error}",
                    path.display(),
                    destination.display()
                )
            })?;
        }
    }
    Ok(())
}

/// PAM Desktop 2 owns its application skeleton: once the package is
/// installed, `pam desktop init` (the package's own command) writes it, so a
/// new project always matches the installed release.
/// `--name` and `--application-id` become the window title and the
/// application id of `pam-desktop.json`.
fn run_desktop_init(
    executable: &OsStr,
    directory: &Path,
    options: &InitOptions,
) -> Result<(), String> {
    let mut arguments = vec!["desktop", "init"];
    if let Some(name) = options.application_name.as_deref() {
        arguments.extend(["--name", name]);
    }
    if let Some(id) = options.application_id.as_deref() {
        arguments.extend(["--id", id]);
    }
    let status = std::process::Command::new(executable)
        .args(&arguments)
        .current_dir(directory)
        .status()
        .map_err(|error| format!("cannot run `pam desktop init`: {error}"))?;
    if !status.success() {
        return Err(format!(
            "`pam desktop init` failed with {status}; on Linux the PAM Desktop host needs GTK 4 (Debian and Ubuntu: `sudo apt install libgtk-4-1`)"
        ));
    }
    Ok(())
}

fn configure_mobile_options(options: &mut InitOptions) -> Result<(), String> {
    let interactive = options.interaction && std::io::stdin().is_terminal();
    let default_name = options
        .directory
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("PAM App")
        .replace(['-', '_'], " ");
    if options.application_name.is_none() {
        options.application_name = Some(if interactive {
            prompt_value("Application name", &default_name)?
        } else {
            default_name.clone()
        });
    }
    if options.application_id.is_none() {
        let slug = default_name
            .to_ascii_lowercase()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('_')
            .to_owned();
        let slug = if slug.starts_with(|character: char| character.is_ascii_lowercase()) {
            slug
        } else {
            format!("project_{slug}")
        };
        let default_id = format!("app.pam.{slug}");
        options.application_id = Some(if interactive {
            prompt_value("Application ID", &default_id)?
        } else {
            default_id
        });
    }
    let application_id = options.application_id.as_deref().unwrap_or_default();
    if !valid_application_id(application_id) {
        return Err(
            "application ID must contain at least two dot-separated lowercase identifier segments"
                .to_owned(),
        );
    }
    if options.mobile_starter.is_none() {
        options.mobile_starter = Some(if interactive {
            let choice = prompt_value(
                "Starter: 1 Blank, 2 Tabs, 3 Auth, 4 E-commerce, 5 Chat, 6 Showcase",
                "1",
            )?;
            match choice.as_str() {
                "1" => MobileStarter::Blank,
                "2" => MobileStarter::Tabs,
                "3" => MobileStarter::Authentication,
                "4" => MobileStarter::Ecommerce,
                "5" => MobileStarter::Chat,
                "6" => MobileStarter::Showcase,
                _ => return Err(format!("invalid mobile starter choice {choice:?}")),
            }
        } else {
            MobileStarter::Blank
        });
    }
    if options.mobile_platforms.is_empty() {
        options.mobile_platforms = if interactive {
            MobilePlatform::parse(&prompt_value(
                "Platforms: android, ios, android-tv (comma separated), or all",
                "all",
            )?)?
        } else {
            vec![MobilePlatform::Android, MobilePlatform::Ios]
        };
    }
    Ok(())
}

fn prompt_value(label: &str, default: &str) -> Result<String, String> {
    print!("{label} [{default}] › ");
    std::io::stdout()
        .flush()
        .map_err(|error| format!("cannot display init prompt: {error}"))?;
    let mut value = String::new();
    std::io::stdin()
        .read_line(&mut value)
        .map_err(|error| format!("cannot read init value: {error}"))?;
    let value = value.trim();
    Ok(if value.is_empty() {
        default.to_owned()
    } else {
        value.to_owned()
    })
}

fn valid_application_id(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() >= 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.as_bytes()[0].is_ascii_lowercase()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
}

fn write_pam_manifest(
    directory: &Path,
    template: InitTemplate,
    options: &InitOptions,
) -> Result<(), String> {
    let kind = match template {
        InitTemplate::Api | InitTemplate::Web => ProjectKind::Api,
        InitTemplate::Mobile | InitTemplate::MobileUi => ProjectKind::Native,
        InitTemplate::Laravel => ProjectKind::Laravel,
        InitTemplate::Desktop => ProjectKind::Desktop,
        InitTemplate::Raw => ProjectKind::Raw,
        InitTemplate::Product => ProjectKind::Product,
    };
    let name = directory
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .unwrap_or("pam-project");
    let mut manifest = serde_json::json!({
        "$schema": "https://push-in.github.io/pam-docs/schemas/pam.schema.json",
        "schema": 1,
        "type": kind as u8,
        "name": name,
        "version": "0.1.0",
    });
    if matches!(template, InitTemplate::Mobile | InitTemplate::MobileUi) {
        manifest["native"] = serde_json::json!({
            "applicationId": options.application_id,
            "starter": options.mobile_starter.unwrap_or(MobileStarter::Blank) as u8,
            "platforms": options.mobile_platforms.iter().map(|platform| *platform as u8).collect::<Vec<_>>(),
        });
    }
    if template == InitTemplate::Product {
        manifest["workspace"] = serde_json::json!({
            "surfaceCodes": [1, 2, 3],
            "contractPath": "packages/contracts",
            "designTokenPath": "packages/contracts/design-tokens.json"
        });
    }
    write_new(
        &directory.join("pam.json"),
        &(serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot serialize PAM project manifest: {error}"))?
            + "\n"),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildManifest {
    pam_version: &'static str,
    target: String,
    entry: String,
    php_library: String,
    files: Vec<BuildFile>,
}

#[derive(Serialize)]
struct BuildFile {
    path: String,
    bytes: u64,
    sha256: String,
}

pub fn build(project: &Path, output: &Path, entry: &Path) -> Result<u8, String> {
    if output.exists() {
        return Err(format!(
            "refusing to overwrite build output {}; choose a new --output directory",
            output.display()
        ));
    }
    if entry.is_absolute()
        || entry
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("build entry must be a relative path inside the project".to_owned());
    }
    let entry_source = project.join(entry);
    if !entry_source.is_file() {
        return Err(format!(
            "build entry {} is not a file",
            entry_source.display()
        ));
    }
    if project.join("composer.json").is_file() && !project.join("vendor/autoload.php").is_file() {
        return Err(
            "vendor/autoload.php is missing; run `pam composer install` before `pam build`"
                .to_owned(),
        );
    }

    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate the Pam executable: {error}"))?;
    let php_library = linked_php_library(&executable)?;
    let application = output.join("app");
    let binary_directory = output.join("bin");
    let library_directory = output.join("lib");
    fs::create_dir_all(&application)
        .and_then(|()| fs::create_dir_all(&binary_directory))
        .and_then(|()| fs::create_dir_all(&library_directory))
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;

    let project_root = fs::canonicalize(project)
        .map_err(|error| format!("cannot resolve project root {}: {error}", project.display()))?;
    let output_root = fs::canonicalize(output)
        .map_err(|error| format!("cannot resolve build output {}: {error}", output.display()))?;
    copy_project(
        &project_root,
        project,
        &application,
        &output_root,
        &mut HashSet::new(),
    )?;
    let bundled_executable = binary_directory.join("pam");
    fs::copy(&executable, &bundled_executable)
        .map_err(|error| format!("cannot copy {}: {error}", executable.display()))?;
    let php_library_manifest = if let Some(php_library) = php_library {
        let php_library_name = php_library
            .file_name()
            .ok_or_else(|| "linked PHP library has no filename".to_owned())?;
        fs::copy(&php_library, library_directory.join(php_library_name))
            .map_err(|error| format!("cannot copy {}: {error}", php_library.display()))?;
        format!("lib/{}", php_library_name.to_string_lossy())
    } else {
        "embedded".to_owned()
    };

    let entry = entry.to_string_lossy();
    if entry.contains('\'') {
        return Err("build entry cannot contain a single quote".to_owned());
    }
    let launcher = binary_directory.join("pam-run");
    fs::write(
        &launcher,
        format!(
            "#!/bin/sh\nset -eu\nPAM_BUNDLE=$(CDPATH= cd -- \"$(dirname -- \"$0\")/..\" && pwd)\nexport LD_LIBRARY_PATH=\"$PAM_BUNDLE/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexec \"$PAM_BUNDLE/bin/pam\" \"$PAM_BUNDLE/app/{entry}\" \"$@\"\n"
        ),
    )
    .map_err(|error| format!("cannot write {}: {error}", launcher.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&launcher, fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("cannot mark launcher executable: {error}"))?;
    }

    let manifest = BuildManifest {
        pam_version: env!("CARGO_PKG_VERSION"),
        target: std::env::consts::ARCH.to_owned() + "-" + std::env::consts::OS,
        entry: entry.into_owned(),
        php_library: php_library_manifest,
        files: build_files(output)?,
    };
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|error| format!("cannot serialize build manifest: {error}"))?,
    )
    .map_err(|error| format!("cannot write build manifest: {error}"))?;

    let ui = Terminal::stdout();
    println!("{}", ui.success("● PRODUCTION BUNDLE READY"));
    println!("{}", ui.rule());
    println!(
        "  {} {}",
        ui.muted(format!("{:<12}", "Output")),
        output.display()
    );
    println!(
        "  {} {}/bin/pam-run",
        ui.muted(format!("{:<12}", "Launch")),
        output.display()
    );
    Ok(0)
}

fn linked_php_library(executable: &Path) -> Result<Option<PathBuf>, String> {
    let output = Command::new("ldd")
        .arg(executable)
        .output()
        .map_err(|error| format!("cannot inspect linked libraries with ldd: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ldd failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let line = line.trim();
            if !line.starts_with("libphp") {
                return None;
            }
            let path = line.split("=>").nth(1)?.split_whitespace().next()?;
            Some(PathBuf::from(path))
        })
        .filter(|path| path.is_file()))
}

fn copy_project(
    project_root: &Path,
    source: &Path,
    destination: &Path,
    output: &Path,
    active_directories: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    let canonical_source = fs::canonicalize(source)
        .map_err(|error| format!("cannot resolve {}: {error}", source.display()))?;
    if !canonical_source.starts_with(project_root) {
        return Err(format!(
            "build path escapes the project: {}",
            source.display()
        ));
    }
    if !active_directories.insert(canonical_source.clone()) {
        return Err(format!(
            "build symlink cycle detected at {}",
            source.display()
        ));
    }

    let result = (|| {
        for entry in fs::read_dir(source)
            .map_err(|error| format!("cannot read {}: {error}", source.display()))?
        {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path == output || output.starts_with(&path) {
                continue;
            }
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some(".git" | ".pam" | ".env" | "node_modules" | "target")
            ) {
                continue;
            }
            let target = destination.join(&name);
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_dir() {
                fs::create_dir(&target)
                    .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
                copy_project(project_root, &path, &target, output, active_directories)?;
            } else if file_type.is_file() {
                fs::copy(&path, &target)
                    .map_err(|error| format!("cannot copy {}: {error}", path.display()))?;
            } else if file_type.is_symlink() {
                let canonical = fs::canonicalize(&path)
                    .map_err(|error| format!("cannot resolve {}: {error}", path.display()))?;
                if !canonical.starts_with(project_root) || canonical.starts_with(output) {
                    return Err(format!(
                        "build symlink escapes the project: {}",
                        path.display()
                    ));
                }
                if canonical.is_dir() {
                    fs::create_dir(&target)
                        .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
                    copy_project(
                        project_root,
                        &canonical,
                        &target,
                        output,
                        active_directories,
                    )?;
                } else if canonical.is_file() {
                    fs::copy(&canonical, &target)
                        .map_err(|error| format!("cannot copy {}: {error}", path.display()))?;
                } else {
                    return Err(format!("unsupported build symlink: {}", path.display()));
                }
            }
        }
        Ok(())
    })();
    active_directories.remove(&canonical_source);
    result
}

fn build_files(root: &Path) -> Result<Vec<BuildFile>, String> {
    fn visit(root: &Path, directory: &Path, files: &mut Vec<BuildFile>) -> Result<(), String> {
        for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                visit(root, &path, files)?;
            } else if path.file_name() != Some(OsStr::new("manifest.json")) && path.is_file() {
                let contents = fs::read(&path)
                    .map_err(|error| format!("cannot hash {}: {error}", path.display()))?;
                files.push(BuildFile {
                    path: path
                        .strip_prefix(root)
                        .map_err(|error| error.to_string())?
                        .to_string_lossy()
                        .replace('\\', "/"),
                    bytes: contents.len() as u64,
                    sha256: format!("{:x}", Sha256::digest(contents)),
                });
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn choose_template(
    template: Option<InitTemplate>,
    socket: bool,
    interactive: bool,
) -> Result<(InitTemplate, bool), String> {
    if let Some(template) = template {
        return Ok((template, socket));
    }
    if !interactive {
        return Ok((InitTemplate::Api, socket));
    }

    let ui = Terminal::stdout();
    println!("{}", ui.brand("PAM / NEW PROJECT"));
    println!("{}", ui.rule());
    println!(
        "{}",
        ui.muted("Select the runtime profile that matches what you are shipping.\n")
    );
    println!(
        "  {}  {} {}",
        ui.accent("01"),
        ui.heading(format!("{:<25}", "Raw runtime")),
        ui.muted("Minimal PHP + Embed SAPI")
    );
    println!(
        "  {}  {} {}",
        ui.accent("02"),
        ui.heading(format!("{:<25}", "API")),
        ui.muted("HTTP application starter · recommended")
    );
    println!(
        "  {}  {} {}",
        ui.accent("03"),
        ui.heading(format!("{:<25}", "HTTP + Socket")),
        ui.muted("HTTP and realtime events")
    );
    println!(
        "  {}  {} {}",
        ui.accent("04"),
        ui.heading(format!("{:<25}", "Laravel")),
        ui.muted("Laravel optimized for Pam")
    );
    println!(
        "  {}  {} {}",
        ui.accent("05"),
        ui.heading(format!("{:<25}", "Laravel + Socket")),
        ui.muted("Laravel with realtime events")
    );
    println!(
        "  {}  {} {}",
        ui.accent("06"),
        ui.heading(format!("{:<25}", "Desktop")),
        ui.muted("PAM Desktop 2, native widgets + PHP")
    );
    println!(
        "  {}  {} {}",
        ui.accent("07"),
        ui.heading(format!("{:<25}", "Native · Core")),
        ui.muted("Pure PAM Native primitives")
    );
    println!(
        "  {}  {} {}",
        ui.accent("08"),
        ui.heading(format!("{:<25}", "Native · Official UI")),
        ui.muted("PAM Native UI design system · recommended for apps")
    );
    println!(
        "  {}  {} {}",
        ui.accent("09"),
        ui.heading(format!("{:<25}", "Product workspace")),
        ui.muted("Server + Native + Desktop + shared PHP contracts")
    );
    println!(
        "  {}  {} {}",
        ui.accent("10"),
        ui.heading(format!("{:<25}", "Web")),
        ui.muted("Server-rendered pages with login, sessions and CSRF")
    );
    print!("\n{} ", ui.command("Choose a preset [02] ›"));
    std::io::stdout()
        .flush()
        .map_err(|error| format!("cannot display init prompt: {error}"))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("cannot read init choice: {error}"))?;
    match answer.trim() {
        "" | "2" => Ok((InitTemplate::Api, false)),
        "1" => Ok((InitTemplate::Raw, false)),
        "3" => Ok((InitTemplate::Api, true)),
        "4" => Ok((InitTemplate::Laravel, false)),
        "5" => Ok((InitTemplate::Laravel, true)),
        "6" => Ok((InitTemplate::Desktop, false)),
        "7" => Ok((InitTemplate::Mobile, false)),
        "8" => Ok((InitTemplate::MobileUi, false)),
        "9" => Ok((InitTemplate::Product, false)),
        "10" => Ok((InitTemplate::Web, false)),
        value => Err(format!("invalid init preset {value:?}")),
    }
}

fn init_raw(directory: &Path, socket: bool) -> Result<(), String> {
    let socket_setup = if socket {
        r#"
$socket = new \Pam\WS\Server();
$socket->on('connection', static function (\Pam\WS\Socket $client): void {
    $client->emit('welcome', ['message' => 'Connected to Pam Socket']);
});
"#
    } else {
        ""
    };
    write_new(
        &directory.join("index.php"),
        &format!(
            r#"<?php

declare(strict_types=1);

use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Http\Server;
{socket_setup}
Server::create(static fn (Request $request, Response $response): Response => match ($request->path) {{
    '/api/ping' => $response->json(['message' => 'pong']),
    default => $response->json(['error' => 'Not Found'], 404),
}})->listen((int) (getenv('PAM_PORT') ?: 3000));
"#,
        ),
    )?;
    write_new(&directory.join(".gitignore"), "/.pam/\n")?;
    write_new(&directory.join(".env.example"), "PAM_PORT=3000\n")?;
    Ok(())
}

/// The Composer manifest shared by the Nimbus starters (`api` and `web`).
fn nimbus_composer_manifest(socket: bool, extra: &[(&str, &str)]) -> serde_json::Value {
    let local_packages = local_packages_repository();
    let nimbus_version = "^0.7";
    let arcano_version = "^0.2.9";
    let socket_version = "^1.1";
    let mut require = serde_json::Map::from_iter([
        ("php".to_owned(), serde_json::json!("^8.5")),
        (
            "pushinbr/arcano".to_owned(),
            serde_json::json!(arcano_version),
        ),
        (
            "pushinbr/nimbus".to_owned(),
            serde_json::json!(nimbus_version),
        ),
    ]);
    if socket {
        require.insert(
            "pushinbr/pam-socket".to_owned(),
            serde_json::json!(socket_version),
        );
    }
    let mut manifest = serde_json::json!({
        "name": "app/pam-project",
        "description": "A PHP application powered by the PAM runtime.",
        "type": "project",
        "license": "proprietary",
        "require": require,
        "require-dev": {
            "fakerphp/faker": "^1.24",
            "laravel/pint": "^1.30",
            "phpunit/phpunit": "^12.5",
            "psy/psysh": "^0.12"
        },
        "autoload": {"psr-4": {
            "App\\": "app/",
            "Database\\Factories\\": "database/factories/",
            "Database\\Seeders\\": "database/seeders/"
        }},
        "autoload-dev": {"psr-4": {"Tests\\": "tests/"}},
        "config": {"platform": {"php": "8.5.0"}, "platform-check": true, "sort-packages": true},
        "scripts": {
            "dev": "pam dev index.php",
            "start": "pam start index.php",
            "test": "pam test . --phpunit -c phpunit.xml"
        }
    });
    for (package, constraint) in extra {
        manifest["require"][*package] = serde_json::json!(constraint);
    }
    if let Some(repository) = local_packages {
        manifest["repositories"] = serde_json::json!([repository]);
    }
    manifest
}

/// The web starter: sessions, CSRF, `*.nim.php` views, login, registration and password
/// reset over Arcano, pt_BR and en translations, and feature tests on SQLite.
fn init_web(directory: &Path) -> Result<(), String> {
    let mut manifest = nimbus_composer_manifest(
        false,
        &[("symfony/mailer", "^7.3"), ("symfony/translation", "^7.3")],
    );
    manifest["description"] =
        serde_json::json!("A web application powered by Nimbus and the PAM runtime.");
    manifest["scripts"]["migrate"] = serde_json::json!("pam exec bin/migrate");
    manifest["scripts"]["views:check"] = serde_json::json!("pam views:check");
    write_new(
        &directory.join("composer.json"),
        &(serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot serialize Composer manifest: {error}"))?
            + "\n"),
    )?;
    let migration = format!(
        "database/migrations/{}_0_0_default_create_users.php",
        crate::api_generator::migration_date(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| "system clock is before the Unix epoch".to_owned())?
                .as_secs()
        )
    );
    for (path, contents) in crate::web_starter::FILES {
        let path = if *path == "database/migrations/create_users.php" {
            migration.as_str()
        } else {
            path
        };
        let target = directory.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        write_new(&target, contents)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            directory.join("bin/migrate"),
            fs::Permissions::from_mode(0o755),
        )
        .map_err(|error| format!("cannot make bin/migrate executable: {error}"))?;
    }
    write_nimbus_bin(directory)?;
    fs::create_dir_all(directory.join("storage"))
        .map_err(|error| format!("cannot create storage: {error}"))?;
    let example = crate::web_starter::FILES
        .iter()
        .find(|(path, _)| *path == ".env.example")
        .map(|(_, contents)| *contents)
        .unwrap_or_default();
    write_new(
        &directory.join(".env"),
        &example.replace("APP_KEY=\n", &format!("APP_KEY={}\n", app_key()?)),
    )
}

/// `base64:` and 32 random bytes, the `APP_KEY` format Nimbus reads.
fn app_key() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| format!("cannot generate APP_KEY: {error}"))?;
    Ok(format!(
        "base64:{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
    ))
}

/// `bootstrap/app.php` of the api starter: builds the `Nimbus\App` without listening, so
/// `index.php`, the tests and `bin/nimbus tinker` share one application.
const API_BOOTSTRAP: &str = r#"<?php

declare(strict_types=1);

use Nimbus\App;

$root = dirname(__DIR__);

$app = new App();
$app->routes($root . '/routes.php');

return $app;
"#;

/// `bin/nimbus`: runs the Nimbus console (`bin/nimbus tinker`) from the project root.
const NIMBUS_BIN: &str = r#"#!/usr/bin/env php
<?php

declare(strict_types=1);

// Usage: bin/nimbus tinker [--execute=code]. Loads bootstrap/app.php of this project.
chdir(dirname(__DIR__));
$_composer_autoload_path = dirname(__DIR__) . '/vendor/autoload.php';
if (!is_file($_composer_autoload_path)) {
    fwrite(STDERR, "Composer dependencies are missing; run pam composer install.\n");
    exit(2);
}
require dirname(__DIR__) . '/vendor/pushinbr/nimbus/bin/nimbus';
"#;

fn write_nimbus_bin(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory.join("bin"))
        .map_err(|error| format!("cannot create bin: {error}"))?;
    write_new(&directory.join("bin/nimbus"), NIMBUS_BIN)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            directory.join("bin/nimbus"),
            fs::Permissions::from_mode(0o755),
        )
        .map_err(|error| format!("cannot make bin/nimbus executable: {error}"))?;
    }
    Ok(())
}

fn init_api(directory: &Path, socket: bool) -> Result<(), String> {
    let manifest = nimbus_composer_manifest(socket, &[]);
    for path in [
        "app/Controllers",
        "database/factories",
        "database/migrations",
        "database/seeders",
        "tests",
    ] {
        fs::create_dir_all(directory.join(path))
            .map_err(|error| format!("cannot create {path}: {error}"))?;
    }
    write_new(
        &directory.join("composer.json"),
        &(serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot serialize Composer manifest: {error}"))?
            + "\n"),
    )?;

    let socket_setup = if socket {
        r#"
$socket = \Pam\Socket\Server::create();
$socket->on('connection', static function (\Pam\WS\Socket $client): void {
    $client->emit('welcome', ['message' => 'Connected to Pam Socket']);
});
"#
    } else {
        ""
    };
    write_new(
        &directory.join("index.php"),
        &format!(
            r#"<?php

declare(strict_types=1);

/** @var \Nimbus\App $app */
$app = require __DIR__ . '/bootstrap/app.php';
{socket_setup}
$app->listen((int) (getenv('PAM_PORT') ?: 3000));
"#
        ),
    )?;
    fs::create_dir_all(directory.join("bootstrap"))
        .map_err(|error| format!("cannot create bootstrap: {error}"))?;
    write_new(&directory.join("bootstrap/app.php"), API_BOOTSTRAP)?;
    write_nimbus_bin(directory)?;
    write_new(
        &directory.join("routes.php"),
        r#"<?php

declare(strict_types=1);

use App\Controllers\PingController;
use Nimbus\RouteRegistrar;

/** @var RouteRegistrar $router */

$router->get('api/ping', [PingController::class, 'show']);
"#,
    )?;
    write_new(
        &directory.join("app/Controllers/PingController.php"),
        r#"<?php

declare(strict_types=1);

namespace App\Controllers;

use Pam\Http\Request;
use Pam\Http\Response;

final readonly class PingController
{
    public function show(Request $request, Response $response): Response
    {
        return $response->json(['message' => 'pong']);
    }
}
"#,
    )?;
    write_new(&directory.join(".gitignore"), "/vendor/\n/.pam/\n")?;
    write_new(&directory.join(".env.example"), "PAM_PORT=3000\n")?;
    write_new(
        &directory.join("phpunit.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<phpunit bootstrap="vendor/autoload.php" colors="true" cacheDirectory=".pam/phpunit-cache">
    <testsuites>
        <testsuite name="Pam application">
            <directory>tests</directory>
        </testsuite>
    </testsuites>
</phpunit>
"#,
    )?;
    write_new(
        &directory.join("tests/TestCase.php"),
        r#"<?php

declare(strict_types=1);

namespace Tests;

use Nimbus\App;

abstract class TestCase extends \Nimbus\Testing\TestCase
{
    protected function createApplication(): App
    {
        $app = require dirname(__DIR__) . '/bootstrap/app.php';

        return $app;
    }
}
"#,
    )?;
    write_new(
        &directory.join("tests/ApplicationTest.php"),
        r#"<?php

declare(strict_types=1);

namespace Tests;

final class ApplicationTest extends TestCase
{
    public function test_ping_endpoint(): void
    {
        $this->getJson('/api/ping')
            ->assertOk()
            ->assertJsonPath('message', 'pong');
    }
}
"#,
    )?;
    Ok(())
}

fn init_product(directory: &Path, options: &InitOptions) -> Result<(), String> {
    let applications = directory.join("apps");
    let contracts = directory.join("packages/contracts");
    fs::create_dir_all(contracts.join("src"))
        .map_err(|error| format!("cannot create product workspace: {error}"))?;
    fs::create_dir_all(&applications)
        .map_err(|error| format!("cannot create product applications: {error}"))?;

    write_new(
        &contracts.join("composer.json"),
        r#"{
    "name": "app/product-contracts",
    "description": "Shared integer-backed contracts for the PAM product workspace.",
    "version": "1.0.0",
    "type": "library",
    "license": "proprietary",
    "require": {"php": "^8.5"},
    "autoload": {"psr-4": {"Product\\Contracts\\": "src/"}},
    "config": {"platform": {"php": "8.5.0"}, "platform-check": true, "sort-packages": true}
}
"#,
    )?;
    write_new(
        &contracts.join("src/ProductSurface.php"),
        r#"<?php

declare(strict_types=1);

namespace Product\Contracts;

enum ProductSurface: int
{
    case Server = 1;
    case Native = 2;
    case Desktop = 3;
}
"#,
    )?;
    write_new(
        &contracts.join("src/ReadinessState.php"),
        r#"<?php

declare(strict_types=1);

namespace Product\Contracts;

enum ReadinessState: int
{
    case Operational = 1;
    case Degraded = 2;
    case Offline = 3;
}
"#,
    )?;
    write_new(
        &contracts.join("src/ContractVersion.php"),
        r#"<?php

declare(strict_types=1);

namespace Product\Contracts;

enum ContractVersion: int
{
    case V1 = 1;
}
"#,
    )?;
    write_new(
        &contracts.join("src/ProductMutationKind.php"),
        r#"<?php

declare(strict_types=1);

namespace Product\Contracts;

enum ProductMutationKind: int
{
    case CheckIn = 1;
}
"#,
    )?;
    write_new(
        &contracts.join("src/MutationResultState.php"),
        r#"<?php

declare(strict_types=1);

namespace Product\Contracts;

enum MutationResultState: int
{
    case Accepted = 1;
}
"#,
    )?;
    write_new(
        &contracts.join("src/MutationDeliveryState.php"),
        r#"<?php

declare(strict_types=1);

namespace Product\Contracts;

enum MutationDeliveryState: int
{
    case Delivered = 1;
    case Queued = 2;
}
"#,
    )?;
    write_new(
        &contracts.join("src/ProductMutation.php"),
        r#"<?php

declare(strict_types=1);

namespace Product\Contracts;

final readonly class ProductMutation
{
    public function __construct(
        public ContractVersion $version,
        public ProductMutationKind $kind,
        public string $idempotencyKey,
    ) {
        if (preg_match('/^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$/', $this->idempotencyKey) !== 1) {
            throw new \InvalidArgumentException('Idempotency key is invalid.');
        }
    }

    public static function checkIn(string $idempotencyKey): self
    {
        return new self(ContractVersion::V1, ProductMutationKind::CheckIn, $idempotencyKey);
    }

    /** @param array<array-key, mixed> $payload */
    public static function fromArray(array $payload): self
    {
        $versionCode = $payload['versionCode'] ?? null;
        $kindCode = $payload['mutationKindCode'] ?? null;
        $idempotencyKey = $payload['idempotencyKey'] ?? null;
        if (!is_int($versionCode) || !is_int($kindCode) || !is_string($idempotencyKey)) {
            throw new \UnexpectedValueException('Product mutation is incompatible with this application.');
        }

        $version = ContractVersion::tryFrom($versionCode);
        $kind = ProductMutationKind::tryFrom($kindCode);
        if ($version === null || $kind === null) {
            throw new \UnexpectedValueException('Product mutation uses unsupported contract codes.');
        }

        return new self($version, $kind, $idempotencyKey);
    }

    /** @return array{versionCode: int, mutationKindCode: int, idempotencyKey: string} */
    public function toArray(): array
    {
        return [
            'versionCode' => $this->version->value,
            'mutationKindCode' => $this->kind->value,
            'idempotencyKey' => $this->idempotencyKey,
        ];
    }
}
"#,
    )?;
    write_new(
        &contracts.join("src/ProductMutationReceipt.php"),
        r#"<?php

declare(strict_types=1);

namespace Product\Contracts;

final readonly class ProductMutationReceipt
{
    public function __construct(
        public ContractVersion $version,
        public ProductMutationKind $kind,
        public MutationResultState $state,
        public string $idempotencyKey,
    ) {
        new ProductMutation($this->version, $this->kind, $this->idempotencyKey);
    }

    public static function accepted(ProductMutation $mutation): self
    {
        return new self($mutation->version, $mutation->kind, MutationResultState::Accepted, $mutation->idempotencyKey);
    }

    /** @param array<array-key, mixed> $payload */
    public static function fromArray(array $payload): self
    {
        $versionCode = $payload['versionCode'] ?? null;
        $kindCode = $payload['mutationKindCode'] ?? null;
        $stateCode = $payload['mutationStateCode'] ?? null;
        $idempotencyKey = $payload['idempotencyKey'] ?? null;
        if (!is_int($versionCode) || !is_int($kindCode) || !is_int($stateCode) || !is_string($idempotencyKey)) {
            throw new \UnexpectedValueException('Product mutation receipt is incompatible with this application.');
        }

        $version = ContractVersion::tryFrom($versionCode);
        $kind = ProductMutationKind::tryFrom($kindCode);
        $state = MutationResultState::tryFrom($stateCode);
        if ($version === null || $kind === null || $state === null) {
            throw new \UnexpectedValueException('Product mutation receipt uses unsupported contract codes.');
        }

        return new self($version, $kind, $state, $idempotencyKey);
    }

    /** @return array{versionCode: int, mutationKindCode: int, mutationStateCode: int, idempotencyKey: string} */
    public function toArray(): array
    {
        return [
            'versionCode' => $this->version->value,
            'mutationKindCode' => $this->kind->value,
            'mutationStateCode' => $this->state->value,
            'idempotencyKey' => $this->idempotencyKey,
        ];
    }
}
"#,
    )?;
    write_new(
        &contracts.join("src/ProductSnapshot.php"),
        r#"<?php

declare(strict_types=1);

namespace Product\Contracts;

final readonly class ProductSnapshot
{
    public function __construct(
        public ContractVersion $version,
        public ProductSurface $surface,
        public ReadinessState $state,
        public string $headline,
    ) {
        if (trim($this->headline) === '' || mb_strlen($this->headline) > 120) {
            throw new \InvalidArgumentException('Headline must contain 1 to 120 characters.');
        }
    }

    public static function operational(ProductSurface $surface): self
    {
        return new self(ContractVersion::V1, $surface, ReadinessState::Operational, 'All systems ready');
    }

    /** @param array<array-key, mixed> $payload */
    public static function fromArray(array $payload): self
    {
        $versionCode = $payload['versionCode'] ?? null;
        $surfaceCode = $payload['surfaceCode'] ?? null;
        $stateCode = $payload['stateCode'] ?? null;
        $headline = $payload['headline'] ?? null;

        if (!is_int($versionCode) || !is_int($surfaceCode) || !is_int($stateCode) || !is_string($headline)) {
            throw new \UnexpectedValueException('Product snapshot is incompatible with this application.');
        }

        $version = ContractVersion::tryFrom($versionCode);
        $surface = ProductSurface::tryFrom($surfaceCode);
        $state = ReadinessState::tryFrom($stateCode);

        if ($version === null || $surface === null || $state === null) {
            throw new \UnexpectedValueException('Product snapshot uses unsupported contract codes.');
        }

        return new self($version, $surface, $state, $headline);
    }

    /** @return array{versionCode: int, surfaceCode: int, stateCode: int, headline: string} */
    public function toArray(): array
    {
        return [
            'versionCode' => $this->version->value,
            'surfaceCode' => $this->surface->value,
            'stateCode' => $this->state->value,
            'headline' => $this->headline,
        ];
    }
}
"#,
    )?;
    fs::create_dir_all(contracts.join("schema"))
        .map_err(|error| format!("cannot create product contract schema: {error}"))?;
    write_new(
        &contracts.join("design-tokens.json"),
        r##"{
    "schemaVersion": 1,
    "themes": [
        {
            "modeCode": 1,
            "name": "light",
            "colors": {
                "background": "#f8fafc",
                "surface": "#ffffff",
                "surfaceRaised": "#ffffff",
                "foreground": "#0f172a",
                "mutedForeground": "#475569",
                "border": "#cbd5e1",
                "primary": "#166534",
                "onPrimary": "#ffffff",
                "success": "#15803d",
                "warning": "#854d0e",
                "danger": "#b91c1c",
                "focus": "#166534"
            }
        },
        {
            "modeCode": 2,
            "name": "dark",
            "colors": {
                "background": "#0b1120",
                "surface": "#111827",
                "surfaceRaised": "#182235",
                "foreground": "#f8fafc",
                "mutedForeground": "#cbd5e1",
                "border": "#475569",
                "primary": "#4ade80",
                "onPrimary": "#052e16",
                "success": "#4ade80",
                "warning": "#fbbf24",
                "danger": "#fb7185",
                "focus": "#68ded2"
            }
        }
    ],
    "spacing": [4, 8, 12, 16, 24, 32, 48],
    "radii": [8, 12, 16, 24],
    "motionMs": [150, 240, 360],
    "minimumTouchTarget": 48
}
"##,
    )?;
    write_new(
        &contracts.join("schema/product-design-tokens.schema.json"),
        r##"{
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://pam.dev/schemas/product-design-tokens-v1.json",
    "title": "PAM Product Design Tokens v1",
    "type": "object",
    "additionalProperties": false,
    "required": ["schemaVersion", "themes", "spacing", "radii", "motionMs", "minimumTouchTarget"],
    "properties": {
        "schemaVersion": {"type": "integer", "const": 1},
        "themes": {
            "type": "array",
            "minItems": 2,
            "maxItems": 2,
            "prefixItems": [
                {"$ref": "#/$defs/lightTheme"},
                {"$ref": "#/$defs/darkTheme"}
            ],
            "items": false
        },
        "spacing": {"type": "array", "const": [4, 8, 12, 16, 24, 32, 48]},
        "radii": {"type": "array", "const": [8, 12, 16, 24]},
        "motionMs": {"type": "array", "const": [150, 240, 360]},
        "minimumTouchTarget": {"type": "integer", "minimum": 48}
    },
    "$defs": {
        "colors": {
            "type": "object",
            "additionalProperties": false,
            "required": ["background", "surface", "surfaceRaised", "foreground", "mutedForeground", "border", "primary", "onPrimary", "success", "warning", "danger", "focus"],
            "properties": {
                "background": {"$ref": "#/$defs/color"},
                "surface": {"$ref": "#/$defs/color"},
                "surfaceRaised": {"$ref": "#/$defs/color"},
                "foreground": {"$ref": "#/$defs/color"},
                "mutedForeground": {"$ref": "#/$defs/color"},
                "border": {"$ref": "#/$defs/color"},
                "primary": {"$ref": "#/$defs/color"},
                "onPrimary": {"$ref": "#/$defs/color"},
                "success": {"$ref": "#/$defs/color"},
                "warning": {"$ref": "#/$defs/color"},
                "danger": {"$ref": "#/$defs/color"},
                "focus": {"$ref": "#/$defs/color"}
            }
        },
        "color": {"type": "string", "pattern": "^#[0-9a-f]{6}$"},
        "lightTheme": {
            "type": "object", "additionalProperties": false,
            "required": ["modeCode", "name", "colors"],
            "properties": {"modeCode": {"type": "integer", "const": 1}, "name": {"const": "light"}, "colors": {"$ref": "#/$defs/colors"}}
        },
        "darkTheme": {
            "type": "object", "additionalProperties": false,
            "required": ["modeCode", "name", "colors"],
            "properties": {"modeCode": {"type": "integer", "const": 2}, "name": {"const": "dark"}, "colors": {"$ref": "#/$defs/colors"}}
        }
    }
}
"##,
    )?;
    write_new(
        &contracts.join("schema/product-snapshot.schema.json"),
        r#"{
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://pam.dev/schemas/product-snapshot-v1.json",
    "title": "PAM Product Snapshot v1",
    "type": "object",
    "additionalProperties": false,
    "required": ["versionCode", "surfaceCode", "stateCode", "headline"],
    "properties": {
        "versionCode": {"type": "integer", "const": 1},
        "surfaceCode": {"type": "integer", "enum": [1, 2, 3]},
        "stateCode": {"type": "integer", "enum": [1, 2, 3]},
        "headline": {"type": "string", "minLength": 1, "maxLength": 120}
    }
}
"#,
    )?;
    write_new(
        &contracts.join("schema/product-mutation.schema.json"),
        r#"{
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://pam.dev/schemas/product-mutation-v1.json",
    "title": "PAM Product Mutation v1",
    "type": "object",
    "additionalProperties": false,
    "required": ["versionCode", "mutationKindCode", "idempotencyKey"],
    "properties": {
        "versionCode": {"type": "integer", "const": 1},
        "mutationKindCode": {"type": "integer", "const": 1},
        "idempotencyKey": {"type": "string", "pattern": "^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$"}
    }
}
"#,
    )?;
    write_new(
        &contracts.join("schema/product-mutation-receipt.schema.json"),
        r#"{
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://pam.dev/schemas/product-mutation-receipt-v1.json",
    "title": "PAM Product Mutation Receipt v1",
    "type": "object",
    "additionalProperties": false,
    "required": ["versionCode", "mutationKindCode", "mutationStateCode", "idempotencyKey"],
    "properties": {
        "versionCode": {"type": "integer", "const": 1},
        "mutationKindCode": {"type": "integer", "const": 1},
        "mutationStateCode": {"type": "integer", "const": 1},
        "idempotencyKey": {"type": "string", "pattern": "^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$"}
    }
}
"#,
    )?;
    fs::create_dir_all(contracts.join("tests"))
        .map_err(|error| format!("cannot create product contract tests: {error}"))?;
    write_new(
        &contracts.join("tests/contract.php"),
        r#"<?php

declare(strict_types=1);

require dirname(__DIR__).'/src/ProductSurface.php';
require dirname(__DIR__).'/src/ReadinessState.php';
require dirname(__DIR__).'/src/ContractVersion.php';
require dirname(__DIR__).'/src/ProductMutationKind.php';
require dirname(__DIR__).'/src/MutationResultState.php';
require dirname(__DIR__).'/src/MutationDeliveryState.php';
require dirname(__DIR__).'/src/ProductMutation.php';
require dirname(__DIR__).'/src/ProductMutationReceipt.php';
require dirname(__DIR__).'/src/ProductSnapshot.php';

use Product\Contracts\ProductMutation;
use Product\Contracts\ProductMutationReceipt;
use Product\Contracts\MutationDeliveryState;
use Product\Contracts\ProductSnapshot;
use Product\Contracts\ProductSurface;

function expect(bool $condition, string $message): void
{
    if (!$condition) {
        throw new RuntimeException($message);
    }
}

$schema = json_decode((string) file_get_contents(dirname(__DIR__).'/schema/product-snapshot.schema.json'), true, flags: JSON_THROW_ON_ERROR);
expect($schema['properties']['versionCode']['const'] === 1, 'Schema version must be 1.');
expect($schema['properties']['surfaceCode']['enum'] === [1, 2, 3], 'Schema surfaces must remain sequential.');
expect($schema['properties']['stateCode']['enum'] === [1, 2, 3], 'Schema states must remain sequential.');
$mutationSchema = json_decode((string) file_get_contents(dirname(__DIR__).'/schema/product-mutation.schema.json'), true, flags: JSON_THROW_ON_ERROR);
$receiptSchema = json_decode((string) file_get_contents(dirname(__DIR__).'/schema/product-mutation-receipt.schema.json'), true, flags: JSON_THROW_ON_ERROR);
expect($mutationSchema['properties']['mutationKindCode']['const'] === 1, 'Check-in mutation code must be 1.');
expect($receiptSchema['properties']['mutationStateCode']['const'] === 1, 'Accepted mutation state must be 1.');
$tokenSchema = json_decode((string) file_get_contents(dirname(__DIR__).'/schema/product-design-tokens.schema.json'), true, flags: JSON_THROW_ON_ERROR);
$tokens = json_decode((string) file_get_contents(dirname(__DIR__).'/design-tokens.json'), true, flags: JSON_THROW_ON_ERROR);
expect($tokenSchema['additionalProperties'] === false, 'Design token schema must be fail-closed.');
expect(array_keys($tokens) === ['schemaVersion', 'themes', 'spacing', 'radii', 'motionMs', 'minimumTouchTarget'], 'Design token document has unknown or missing fields.');
expect($tokens['schemaVersion'] === 1, 'Design token schema version must be 1.');
expect(array_column($tokens['themes'], 'modeCode') === [1, 2], 'Theme modes must use sequential integer codes.');
expect(array_column($tokens['themes'], 'name') === ['light', 'dark'], 'Theme order changed unexpectedly.');
expect($tokens['spacing'] === [4, 8, 12, 16, 24, 32, 48], 'Spacing must preserve the 4/8 rhythm.');
expect($tokens['radii'] === [8, 12, 16, 24], 'Radius scale changed unexpectedly.');
expect($tokens['motionMs'] === [150, 240, 360], 'Motion must remain bounded.');
expect($tokens['minimumTouchTarget'] >= 48, 'Touch targets must remain accessible.');
foreach ($tokens['themes'] as $theme) {
    expect(array_keys($theme) === ['modeCode', 'name', 'colors'], 'Theme has unknown or missing fields.');
    expect(array_keys($theme['colors']) === ['background', 'surface', 'surfaceRaised', 'foreground', 'mutedForeground', 'border', 'primary', 'onPrimary', 'success', 'warning', 'danger', 'focus'], 'Theme colors have unknown or missing roles.');
    foreach ($theme['colors'] as $color) {
        expect(is_string($color) && preg_match('/^#[0-9a-f]{6}$/D', $color) === 1, 'Theme colors must use canonical lowercase hex.');
    }
}

foreach ([ProductSurface::Server, ProductSurface::Native, ProductSurface::Desktop] as $surface) {
    $snapshot = ProductSnapshot::operational($surface)->toArray();
    expect($snapshot['versionCode'] === 1, 'Snapshot version must be 1.');
    expect($snapshot['surfaceCode'] === $surface->value, 'Snapshot surface must round-trip.');
    expect($snapshot['stateCode'] === 1, 'Operational state must use code 1.');
    expect($snapshot['headline'] === 'All systems ready', 'Operational headline changed unexpectedly.');
}

$decoded = ProductSnapshot::fromArray([
    'versionCode' => 1,
    'surfaceCode' => 1,
    'stateCode' => 2,
    'headline' => 'Scheduled maintenance',
]);
expect($decoded->toArray()['stateCode'] === 2, 'Decoded state must round-trip.');

$mutation = ProductMutation::checkIn('check-in:device-1:1');
$receipt = ProductMutationReceipt::fromArray(ProductMutationReceipt::accepted($mutation)->toArray());
expect($mutation->toArray()['mutationKindCode'] === 1, 'Check-in mutation kind must be 1.');
expect($receipt->toArray() === [
    'versionCode' => 1,
    'mutationKindCode' => 1,
    'mutationStateCode' => 1,
    'idempotencyKey' => 'check-in:device-1:1',
], 'Accepted mutation receipt must round-trip.');
expect(MutationDeliveryState::Delivered->value === 1, 'Delivered state must be 1.');
expect(MutationDeliveryState::Queued->value === 2, 'Queued state must be 2.');

$invalidMutationRejected = false;
try {
    ProductMutation::fromArray(['versionCode' => 1, 'mutationKindCode' => '1', 'idempotencyKey' => '../unsafe']);
} catch (Throwable) {
    $invalidMutationRejected = true;
}
expect($invalidMutationRejected, 'Invalid mutation types and keys must be rejected.');

foreach ([
    ['versionCode' => 2, 'surfaceCode' => 1, 'stateCode' => 1, 'headline' => 'Future'],
    ['versionCode' => 1, 'surfaceCode' => '1', 'stateCode' => 1, 'headline' => 'Wrong type'],
    ['versionCode' => 1, 'surfaceCode' => 1, 'stateCode' => 9, 'headline' => 'Unknown state'],
] as $invalid) {
    $rejected = false;
    try {
        ProductSnapshot::fromArray($invalid);
    } catch (Throwable) {
        $rejected = true;
    }
    expect($rejected, 'Invalid snapshot must be rejected.');
}

// The native app's theme tokens are generated from this contract.
$generator = dirname(__DIR__).'/bin/theme-css.php';
$nativeTheme = dirname(__DIR__, 3).'/apps/native/src/theme.css';
if (is_file($nativeTheme)) {
    ob_start();
    (static function () use ($generator): void {
        $argv = [$generator];
        require $generator;
    })();
    expect(ob_get_clean() === file_get_contents($nativeTheme), 'apps/native/src/theme.css is out of date: run `composer theme` in apps/native.');
}

echo "Cross-surface product contract verified.\n";
"#,
    )?;

    let server = applications.join("server");
    let native = applications.join("native");
    let desktop = applications.join("desktop");
    fs::create_dir_all(&server).map_err(|error| format!("cannot create server app: {error}"))?;
    fs::create_dir_all(&native).map_err(|error| format!("cannot create native app: {error}"))?;
    fs::create_dir_all(&desktop).map_err(|error| format!("cannot create desktop app: {error}"))?;
    init_api(&server, false)?;
    // PAM Native UI 2: the design-token contract becomes the app's compile-
    // time theme tokens (apps/native/src/theme.css, pam-native.json
    // "theme.tokens"), generated by packages/contracts/bin/theme-css.php.
    init_mobile_with_ui(&native, UiLine::Current, options)?;
    init_product_desktop(&desktop)?;
    add_product_contract(&server.join("composer.json"))?;
    add_product_contract(&native.join("composer.json"))?;
    add_product_contract(&desktop.join("composer.json"))?;
    write_pam_manifest(&server, InitTemplate::Api, options)?;
    write_pam_manifest(&native, InitTemplate::MobileUi, options)?;
    write_pam_manifest(&desktop, InitTemplate::Desktop, options)?;

    replace_generated(
        &server.join("index.php"),
        r#"<?php

declare(strict_types=1);

use Nimbus\App;
use Pam\Http\Request;
use Pam\Http\Response;
use Product\Contracts\ProductMutation;
use Product\Contracts\ProductMutationReceipt;
use Product\Contracts\ProductSnapshot;
use Product\Contracts\ProductSurface;

require __DIR__.'/vendor/autoload.php';

$app = new App;
$app->get('/api/status', static fn (Request $request, Response $response): Response => $response
    ->header('cache-control', 'no-store')
    ->json(ProductSnapshot::operational(ProductSurface::Server)->toArray()));
$app->post('/api/check-ins', static function (Request $request, Response $response): Response {
    try {
        $payload = $request->json();
        if (!is_array($payload)) {
            throw new \UnexpectedValueException('Mutation payload must be an object.');
        }
        $mutation = ProductMutation::fromArray($payload);
        $header = $request->getHeader('idempotency-key');
        if (!is_string($header) || !hash_equals($mutation->idempotencyKey, $header)) {
            throw new \UnexpectedValueException('Idempotency key does not match.');
        }

        return $response
            ->header('cache-control', 'no-store')
            ->json(ProductMutationReceipt::accepted($mutation)->toArray(), 202);
    } catch (\Throwable) {
        return $response
            ->header('cache-control', 'no-store')
            ->json(['message' => 'Invalid product mutation.'], 422);
    }
});
$app->listen((int) (getenv('PAM_PORT') ?: 3000));
"#,
    )?;
    replace_generated(
        &server.join("tests/ApplicationTest.php"),
        r#"<?php

declare(strict_types=1);

use Nimbus\App;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Testing\TestClient;
use PHPUnit\Framework\TestCase;
use Product\Contracts\ProductMutation;
use Product\Contracts\ProductMutationReceipt;
use Product\Contracts\ProductSnapshot;
use Product\Contracts\ProductSurface;

final class ApplicationTest extends TestCase
{
    public function test_shared_product_status_contract(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/api/status', static fn (Request $request, Response $response): Response => $response
            ->header('cache-control', 'no-store')
            ->json(ProductSnapshot::operational(ProductSurface::Server)->toArray()));

        (new TestClient($app))
            ->get('/api/status')
            ->assertSuccessful()
            ->assertHeader('cache-control', 'no-store')
            ->assertJson(['versionCode' => 1, 'surfaceCode' => 1, 'stateCode' => 1, 'headline' => 'All systems ready']);
        self::addToAssertionCount(1);
    }

    public function test_check_in_mutation_is_idempotent_and_versioned(): void
    {
        $app = new App(discoverPackages: false);
        $app->post('/api/check-ins', static function (Request $request, Response $response): Response {
            try {
                $payload = $request->json();
                if (!is_array($payload)) {
                    throw new \UnexpectedValueException;
                }
                $mutation = ProductMutation::fromArray($payload);
                $key = $request->getHeader('idempotency-key');
                if (!is_string($key) || !hash_equals($mutation->idempotencyKey, $key)) {
                    throw new \UnexpectedValueException;
                }
                return $response->json(ProductMutationReceipt::accepted($mutation)->toArray(), 202);
            } catch (\Throwable) {
                return $response->json(['message' => 'Invalid product mutation.'], 422);
            }
        });

        $payload = ProductMutation::checkIn('check-in:test-device:1')->toArray();
        $headers = ['idempotency-key' => 'check-in:test-device:1'];
        $expected = [
            'versionCode' => 1,
            'mutationKindCode' => 1,
            'mutationStateCode' => 1,
            'idempotencyKey' => 'check-in:test-device:1',
        ];
        (new TestClient($app))->postJson('/api/check-ins', $payload, $headers)->assertStatus(202)->assertJson($expected);
        (new TestClient($app))->postJson('/api/check-ins', $payload, $headers)->assertStatus(202)->assertJson($expected);
        (new TestClient($app))->postJson('/api/check-ins', $payload, ['idempotency-key' => 'wrong'])->assertStatus(422);
        self::addToAssertionCount(3);
    }
}
"#,
    )?;
    fs::create_dir_all(contracts.join("bin"))
        .map_err(|error| format!("cannot create product contract tools: {error}"))?;
    write_new(
        &contracts.join("bin/theme-css.php"),
        PRODUCT_THEME_GENERATOR,
    )?;
    write_new(&native.join("src/theme.css"), PRODUCT_THEME_CSS)?;
    use_product_theme_tokens(&native)?;
    write_new(
        &native.join("tests/HelloTest.php"),
        PRODUCT_NATIVE_SCREEN_TEST,
    )?;
    replace_generated(
        &native.join("src/Hello.pam"),
        r#"<?php

declare(strict_types=1);

namespace App;

use Pam\Native\Attributes\State;
use Pam\Native\Component;
use Pam\Native\Http\Http;
use Pam\Native\Http\HttpResponse;
use Pam\Native\Storage\Storage;
use Pam\Native\Sync\Mutation;
use Pam\Native\Sync\OfflineMutationQueue;
use Product\Contracts\ProductMutation;
use Product\Contracts\ProductMutationReceipt;
use Product\Contracts\ProductSnapshot;
use Product\Contracts\ProductSurface;
use Product\Contracts\ReadinessState;

final class Hello extends Component
{
    private const MAX_PENDING_MUTATIONS = 32;

    #[State]
    public string $headline = '';

    #[State]
    public int $refreshCount = 0;

    #[State]
    public int $serverStateCode = 3;

    #[State]
    public string $serverHeadline = 'Server not checked yet';

    #[State]
    public string $serverLabel = 'Offline';

    #[State]
    public string $serverTone = 'danger';

    #[State]
    public string $syncMessage = 'Start the Server, then verify the shared contract.';

    #[State]
    public bool $syncing = false;

    #[State]
    public bool $outboxLoaded = false;

    #[State]
    public int $pendingMutations = 0;

    #[State]
    public string $mutationMessage = 'Loading the private native outbox…';

    private OfflineMutationQueue $outbox;

    public function boot(): void
    {
        $this->outbox = new OfflineMutationQueue;
        // Template expressions read state, not method calls.
        $this->headline = $this->snapshot()->headline;
    }

    public function mount(): void
    {
        try {
            Storage::get('product.outbox.v1', function (?string $snapshot): void {
                try {
                    if ($snapshot !== null) {
                        $this->outbox = OfflineMutationQueue::restore($snapshot);
                    }
                    $this->mutationMessage = 'Offline outbox ready.';
                } catch (\Throwable) {
                    $this->outbox = new OfflineMutationQueue;
                    $this->mutationMessage = 'Invalid outbox was isolated; a clean queue is ready.';
                }
                $this->outboxLoaded = true;
                $this->updatePendingMutations();
                $this->replayReadyMutation();
            });
        } catch (\Throwable) {
            $this->outboxLoaded = true;
            $this->mutationMessage = 'Native storage is unavailable; check-in was not queued.';
        }
    }

    public function resumed(): void
    {
        if ($this->outboxLoaded) {
            $this->replayReadyMutation();
        }
    }

    public function refresh(): void
    {
        if ($this->syncing) {
            return;
        }

        $this->syncing = true;
        $this->refreshCount++;
        $this->syncMessage = 'Checking the Server…';
        $endpoint = getenv('PAM_PRODUCT_SERVER_URL') ?: 'http://127.0.0.1:3000/api/status';

        try {
            Http::request(
                method: 'GET',
                url: $endpoint,
                callback: function (HttpResponse $response): void {
                    try {
                        if (!$response->successful() || strlen($response->body) > 65_536) {
                            throw new \RuntimeException('Server is unavailable.');
                        }

                        $payload = json_decode($response->body, true, flags: JSON_THROW_ON_ERROR);
                        if (!is_array($payload)) {
                            throw new \UnexpectedValueException('Server returned an invalid document.');
                        }

                        $snapshot = ProductSnapshot::fromArray($payload);
                        if ($snapshot->surface !== ProductSurface::Server) {
                            throw new \UnexpectedValueException('Server returned the wrong product surface.');
                        }

                        $this->showServer($snapshot->state, $snapshot->headline);
                        $this->syncMessage = 'Verified contract v'.$snapshot->version->value.' from Server surface 1.';
                    } catch (\Throwable) {
                        $this->showServer(ReadinessState::Offline, 'Server unavailable or incompatible');
                        $this->syncMessage = 'Check PAM_PRODUCT_SERVER_URL and the Server contract version.';
                    } finally {
                        $this->syncing = false;
                    }
                },
                headers: ['Accept' => 'application/json'],
                timeoutMs: 5_000,
            );
        } catch (\Throwable) {
            $this->showServer(ReadinessState::Offline, 'Server request could not start');
            $this->syncMessage = 'Check PAM_PRODUCT_SERVER_URL and the Native network policy.';
            $this->syncing = false;
        }
    }

    public function queueCheckIn(): void
    {
        if (!$this->outboxLoaded) {
            $this->mutationMessage = 'Wait for the offline outbox to finish loading.';
            return;
        }

        try {
            if ($this->pendingMutations >= self::MAX_PENDING_MUTATIONS) {
                throw new \OverflowException('Product outbox is full.');
            }
            $key = 'check-in:'.bin2hex(random_bytes(16));
            $mutation = ProductMutation::checkIn($key);
            $this->outbox->enqueue($key, 'product.check-in', $mutation->toArray());
            $this->updatePendingMutations();
            $this->mutationMessage = 'Check-in persisted; attempting delivery.';
            $this->persistOutbox(function (): void {
                $this->replayReadyMutation();
            });
        } catch (\Throwable) {
            $this->mutationMessage = 'Check-in could not be queued safely.';
        }
    }

    private function replayReadyMutation(): int
    {
        $mutation = $this->outbox->ready(self::nowMs(), 1)[0] ?? null;
        if (!$mutation instanceof Mutation) {
            return 0;
        }

        $this->outbox->sending($mutation->id);
        $endpoint = getenv('PAM_PRODUCT_MUTATION_URL') ?: 'http://127.0.0.1:3000/api/check-ins';
        try {
            Http::json(
                method: 'POST',
                url: $endpoint,
                data: $mutation->payload,
                callback: function (HttpResponse $response) use ($mutation): void {
                    try {
                        if (!$response->successful() || strlen($response->body) > 65_536) {
                            throw new \RuntimeException('Mutation delivery failed.');
                        }
                        $payload = json_decode($response->body, true, flags: JSON_THROW_ON_ERROR);
                        if (!is_array($payload)) {
                            throw new \UnexpectedValueException('Mutation receipt is invalid.');
                        }
                        $receipt = ProductMutationReceipt::fromArray($payload);
                        if (!hash_equals($mutation->key, $receipt->idempotencyKey)) {
                            throw new \UnexpectedValueException('Mutation receipt key does not match.');
                        }
                        $this->outbox->applied($mutation->id);
                        $this->outbox->prune();
                        $this->mutationMessage = 'Check-in accepted by Server.';
                    } catch (\Throwable) {
                        $this->outbox->retry($mutation->id, self::nowMs(), 'delivery');
                        $this->mutationMessage = 'Offline: check-in retained for bounded retry.';
                    }
                    $this->updatePendingMutations();
                    $this->persistOutbox(function (): void {
                        $this->replayReadyMutation();
                    });
                },
                headers: ['Idempotency-Key' => $mutation->key],
                timeoutMs: 5_000,
            );
        } catch (\Throwable) {
            $this->outbox->retry($mutation->id, self::nowMs(), 'transport');
            $this->mutationMessage = 'Offline: check-in retained for bounded retry.';
            $this->updatePendingMutations();
            $this->persistOutbox();
        }

        return $mutation->id;
    }

    private function showServer(ReadinessState $state, string $headline): void
    {
        $this->serverStateCode = $state->value;
        $this->serverHeadline = $headline;
        [$this->serverLabel, $this->serverTone] = match ($state) {
            ReadinessState::Operational => ['Operational', 'success'],
            ReadinessState::Degraded => ['Degraded', 'warning'],
            ReadinessState::Offline => ['Offline', 'danger'],
        };
    }

    private function persistOutbox(?\Closure $stored = null): void
    {
        Storage::set('product.outbox.v1', $this->outbox->export(), $stored);
    }

    private function updatePendingMutations(): void
    {
        $snapshot = json_decode($this->outbox->export(), true, flags: JSON_THROW_ON_ERROR);
        $mutations = is_array($snapshot['mutations'] ?? null) ? $snapshot['mutations'] : [];
        $this->pendingMutations = count(array_filter(
            $mutations,
            static fn (mixed $item): bool => is_array($item)
                && is_int($item['status'] ?? null)
                && !in_array($item['status'], [3, 6], true),
        ));
    }

    private static function nowMs(): int
    {
        return (int) floor(microtime(true) * 1_000);
    }

    public function snapshot(): ProductSnapshot
    {
        return ProductSnapshot::operational(ProductSurface::Native);
    }
}
?>

<template>
    <p-screen
        title="{{ $headline }}"
        subtitle="One typed PHP contract across every PAM runtime."
    >
        <View class="contract">
            <p-badge label="Contract v1 · Native surface 2" tone="info" />
        </View>
        <p-card title="Server state · code {{ $serverStateCode }}" subtitle="{{ $serverHeadline }}">
            <p-badge label="{{ $serverLabel }}" tone="{{ $serverTone }}" />
            <Text class="note" testId="product-sync">{{ $syncMessage }}</Text>
        </p-card>
        <p-btn label="Sync Server · {{ $refreshCount }}" block="true" testId="product-refresh" @press="refresh" />
        <p-card title="Offline outbox · {{ $pendingMutations }} pending">
            <Text class="note" testId="product-outbox">{{ $mutationMessage }}</Text>
        </p-card>
        <p-btn label="Queue resilient check-in" variant="outlined" block="true" testId="product-check-in" @press="queueCheckIn" />
    </p-screen>
</template>

<style scoped>
    .contract { flex-direction: row; }
    .note { font-size: 15px; line-height: 22px; color: var(--p-text-muted); }

    @media (prefers-color-scheme: dark) {
        .note { color: var(--p-text-muted-dark); }
    }
</style>
"#,
    )?;
    write_new(
        &directory.join("README.md"),
        r#"# PAM product workspace

One typed PHP domain across Server, Native, and Desktop.

## First vertical flow

- Server exposes `GET /api/status` with integer `versionCode`, `surfaceCode`, and `stateCode`.
- Native fetches `GET /api/status` through OkHttp/URLSession, caps the response
  at 64 KiB, then validates it through `ProductSnapshot::fromArray()` before
  updating native controls.
- Desktop is a PAM Desktop 2 application (`pushinbr/pam-desktop` ^2.1). Its
  home screen (`src/Screens/Home`) reads the same Server endpoint through
  `App\Services\ProductServer` in PHP, caps the response at 64 KiB, rejects
  redirects and cleartext remote origins, and validates it through
  `ProductSnapshot::fromArray()`. Native is shown as unmonitored until a
  trustworthy telemetry channel is connected.
- Native check-ins use the SDK's bounded `OfflineMutationQueue`, persist before
  transport, deduplicate by idempotency key, retry with capped exponential
  backoff, cap the product outbox at 32 entries to remain below native Storage's
  256 KiB value limit, and prune only after a validated Server receipt.
  This example stores only integer codes and opaque idempotency keys; credentials,
  tokens, personal data, and mutation bodies that contain secrets do not belong
  in this general-purpose app storage.

The meanings live in backed enums under `packages/contracts`; transports never
send string status or surface discriminators. `ProductSnapshot::fromArray()`
rejects unknown versions, codes, and field types before application code uses a
remote payload.

## Run

```bash
cd apps/server && pam dev index.php
cd apps/native && pam dev
cd apps/desktop && pam desktop dev
php packages/contracts/tests/contract.php
```

Native defaults to `http://127.0.0.1:3000/api/status`. Override it when the
Server uses another origin (Android Emulator commonly reaches the development
host through `http://10.0.2.2:3000/api/status`):

```bash
PAM_PRODUCT_SERVER_URL=https://api.example.com/api/status pam dev
PAM_PRODUCT_MUTATION_URL=https://api.example.com/api/check-ins pam dev
```

Production Native builds enforce HTTPS at the host boundary.
Desktop also rejects cleartext remote origins, redirects, invalid contracts,
and responses larger than 64 KiB. Loopback HTTP remains available for local
development.

## Release

Build each application in its own directory, then generate the deterministic
cross-surface index from this workspace root:

```bash
cd apps/server && pam package
cd ../native && pam package
cd ../desktop && pam package
cd ../.. && pam package
```

The root command requires distributables for surface codes `1`, `2`, and `3`,
rejects symlinks and changing files, and writes `dist/product-release.json`
plus its SHA-256 sidecar with create-new semantics.

`pam release --check` from the root runs Doctor, lint, and tests inside every
application before executing `packages/contracts/tests/contract.php`.

Generated caches, native builds, Composer vendors, and desktop targets remain
inside their owning application and are removable with `pam clean`.
"#,
    )?;
    write_new(
        &directory.join(".gitignore"),
        "/.pam/\n/dist/\n/apps/*/.pam/\n/apps/*/vendor/\n/apps/*/target/\n/apps/native/android/.gradle/\n/apps/native/android/**/build/\n",
    )
}

fn add_product_contract(path: &Path) -> Result<(), String> {
    let mut manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?,
    )
    .map_err(|error| format!("invalid generated Composer manifest: {error}"))?;
    manifest["require"]["app/product-contracts"] = serde_json::json!("^1.0");
    if manifest.get("repositories").is_none() {
        manifest["repositories"] = serde_json::json!([]);
    }
    let repositories = manifest["repositories"]
        .as_array_mut()
        .ok_or_else(|| "generated Composer repositories must be an array".to_owned())?;
    repositories.push(serde_json::json!({
        "type": "path",
        "url": "../../packages/contracts",
        "options": {"symlink": false}
    }));
    replace_generated(
        path,
        &(serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot encode generated Composer manifest: {error}"))?
            + "\n"),
    )
}

fn replace_generated(path: &Path, contents: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect generated file {}: {error}", path.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to replace non-regular generated file {}",
            path.display()
        ));
    }
    fs::write(path, contents)
        .map_err(|error| format!("cannot replace generated file {}: {error}", path.display()))
}

/// The `product` workspace's Desktop surface: the same PAM Desktop 2
/// application as `--template desktop`, with a home screen that reads the
/// Server status through the shared contract.
/// Product design tokens as PAM Native UI 2 theme tokens (output of
/// packages/contracts/bin/theme-css.php for the default contract).
/// Renders the product screen on the host (`composer test`), so a template
/// that cannot render fails before it reaches a device.
const PRODUCT_NATIVE_SCREEN_TEST: &str = r#"<?php

declare(strict_types=1);

use App\Hello;
use Pam\Native\App;
use Pam\Native\Plugin\PluginManager;
use Pam\Native\Testing;
use PHPUnit\Framework\TestCase;

final class HelloTest extends TestCase
{
    protected function setUp(): void
    {
        // What index.php and App::run() do: the app's components, then the
        // component libraries its plugins declare (PAM Native UI).
        App::components(dirname(__DIR__).'/src', dirname(__DIR__).'/.pam-native/components');
        PluginManager::boot(dirname(__DIR__));
    }

    protected function tearDown(): void
    {
        Testing\reset();
        PluginManager::reset();
    }

    public function testTheProductScreenRendersItsContract(): void
    {
        $screen = Testing\mount(Hello::class);
        $text = $screen->text();

        self::assertStringContainsString('All systems ready', $text);
        self::assertStringContainsString('Contract v1 · Native surface 2', $text);
        self::assertStringContainsString('Offline', $text);
        self::assertStringContainsString('Queue resilient check-in', $text);
        $screen->unmount();
    }
}
"#;

const PRODUCT_THEME_CSS: &str = r#"/* Generated from packages/contracts/design-tokens.json: run `composer theme` after editing it. */
:root {
    --p-bg: #f8fafc;
    --p-surface: #ffffff;
    --p-surface-muted: #ffffff;
    --p-surface-pressed: #ececee;
    --p-border: #cbd5e1;
    --p-border-strong: #8993a1;
    --p-text: #0f172a;
    --p-text-muted: #475569;
    --p-text-subtle: #7c8795;
    --p-accent: #166534;
    --p-accent-pressed: #13562c;
    --p-on-accent: #ffffff;
    --p-accent-soft: #dde8e4;
    --p-accent-soft-pressed: #cbdcd4;
    --p-accent-text: #155532;
    --p-success: #15803d;
    --p-success-soft: #ddebe5;
    --p-warning: #854d0e;
    --p-warning-soft: #eae5df;
    --p-danger: #b91c1c;
    --p-danger-soft: #f0dfe1;
    --p-info: #166534;
    --p-info-soft: #dde8e4;
    --p-bg-dark: #0b1120;
    --p-surface-dark: #111827;
    --p-surface-muted-dark: #182235;
    --p-surface-pressed-dark: #2a3345;
    --p-border-dark: #475569;
    --p-border-strong-dark: #858f9c;
    --p-text-dark: #f8fafc;
    --p-text-muted-dark: #cbd5e1;
    --p-text-subtle-dark: #919aa7;
    --p-accent-dark: #4ade80;
    --p-accent-pressed-dark: #65e393;
    --p-on-accent-dark: #052e16;
    --p-accent-soft-dark: #132a2c;
    --p-accent-soft-pressed-dark: #183a33;
    --p-accent-text-dark: #4ade80;
    --p-success-dark: #4ade80;
    --p-success-soft-dark: #132a2c;
    --p-warning-dark: #fbbf24;
    --p-warning-soft-dark: #282620;
    --p-danger-dark: #fb7185;
    --p-danger-soft-dark: #281d2c;
    --p-info-dark: #68ded2;
    --p-info-soft-dark: #162a35;
    --p-radius-sm: 8px;
    --p-radius: 12px;
    --p-radius-lg: 16px;
    --p-radius-xl: 24px;
    --p-touch: 48px;
    --p-motion-fast: 150ms;
    --p-motion: 240ms;
}
"#;

const PRODUCT_THEME_GENERATOR: &str = r#"<?php

declare(strict_types=1);

/**
 * Writes the PAM Native UI theme tokens (`:root { --p-*: ... }`) for the
 * product design-token contract. `pam-native.json` "theme.tokens" points the
 * UI library at the result, resolved at compile time:
 *
 *     php ../../packages/contracts/bin/theme-css.php > src/theme.css
 */

$path = $argv[1] ?? dirname(__DIR__).'/design-tokens.json';
$document = json_decode((string) file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
if (!is_array($document) || ($document['schemaVersion'] ?? null) !== 1 || count($document['themes'] ?? []) !== 2) {
    fwrite(STDERR, "Incompatible design token contract.\n");
    exit(1);
}

/** Mixes two #rrggbb colours: $weight of $a. */
$mix = static function (string $a, string $b, float $weight): string {
    $channel = static fn (string $hex, int $index): int => (int) hexdec(substr($hex, 1 + 2 * $index, 2));
    $out = '#';
    for ($index = 0; $index < 3; $index++) {
        $out .= sprintf('%02x', (int) round($channel($a, $index) * $weight + $channel($b, $index) * (1 - $weight)));
    }
    return $out;
};

$palette = static function (array $colors, bool $dark) use ($mix): array {
    $edge = $dark ? '#ffffff' : '#000000';
    return [
        'bg' => $colors['background'],
        'surface' => $colors['surface'],
        'surface-muted' => $colors['surfaceRaised'],
        'surface-pressed' => $mix($colors['foreground'], $colors['surfaceRaised'], 0.08),
        'border' => $colors['border'],
        'border-strong' => $mix($colors['foreground'], $colors['border'], 0.35),
        'text' => $colors['foreground'],
        'text-muted' => $colors['mutedForeground'],
        'text-subtle' => $mix($colors['mutedForeground'], $colors['background'], 0.7),
        'accent' => $colors['primary'],
        'accent-pressed' => $mix($edge, $colors['primary'], 0.15),
        'on-accent' => $colors['onPrimary'],
        'accent-soft' => $mix($colors['primary'], $colors['background'], 0.12),
        'accent-soft-pressed' => $mix($colors['primary'], $colors['background'], 0.2),
        'accent-text' => $dark ? $colors['primary'] : $mix($colors['primary'], $colors['foreground'], 0.8),
        'success' => $colors['success'],
        'success-soft' => $mix($colors['success'], $colors['background'], 0.12),
        'warning' => $colors['warning'],
        'warning-soft' => $mix($colors['warning'], $colors['background'], 0.12),
        'danger' => $colors['danger'],
        'danger-soft' => $mix($colors['danger'], $colors['background'], 0.12),
        'info' => $colors['focus'],
        'info-soft' => $mix($colors['focus'], $colors['background'], 0.12),
    ];
};

[$light, $dark] = $document['themes'];
$lines = ["/* Generated from packages/contracts/design-tokens.json: run `composer theme` after editing it. */", ':root {'];
foreach ($palette($light['colors'], false) as $name => $value) {
    $lines[] = "    --p-{$name}: {$value};";
}
foreach ($palette($dark['colors'], true) as $name => $value) {
    $lines[] = "    --p-{$name}-dark: {$value};";
}
[$small, $medium, $large, $extra] = $document['radii'];
[$fast, $normal] = $document['motionMs'];
$lines[] = "    --p-radius-sm: {$small}px;";
$lines[] = "    --p-radius: {$medium}px;";
$lines[] = "    --p-radius-lg: {$large}px;";
$lines[] = "    --p-radius-xl: {$extra}px;";
$lines[] = "    --p-touch: {$document['minimumTouchTarget']}px;";
$lines[] = "    --p-motion-fast: {$fast}ms;";
$lines[] = "    --p-motion: {$normal}ms;";
$lines[] = '}';
echo implode("\n", $lines), "\n";
"#;

/// Points the product's native app at its generated theme tokens and gives it
/// `composer theme` to regenerate them from the contract.
fn use_product_theme_tokens(native: &Path) -> Result<(), String> {
    for (file, edit) in [("pam-native.json", "theme"), ("composer.json", "scripts")] {
        let path = native.join(file);
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let mut document: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("invalid {}: {error}", path.display()))?;
        if edit == "theme" {
            document["theme"] = serde_json::json!({"tokens": "src/theme.css"});
        } else {
            document["scripts"]["theme"] =
                serde_json::json!("php ../../packages/contracts/bin/theme-css.php > src/theme.css");
        }
        fs::write(
            &path,
            serde_json::to_string_pretty(&document)
                .map_err(|error| format!("cannot serialize {}: {error}", path.display()))?
                + "\n",
        )
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(())
}

fn init_product_desktop(directory: &Path) -> Result<(), String> {
    for folder in [
        "src/Navigation",
        "src/Services",
        "src/Screens/Home",
        "tests",
    ] {
        fs::create_dir_all(directory.join(folder))
            .map_err(|error| format!("cannot create desktop {folder}: {error}"))?;
    }
    init_desktop(directory, None)?;
    write_new(
        &directory.join("index.php"),
        r#"<?php

declare(strict_types=1);

use App\Navigation\AppRouter;
use Pam\Desktop\App;

require __DIR__.'/vendor/autoload.php';

App::components(__DIR__.'/src', __DIR__.'/.pam-desktop/components');
App::run(AppRouter::make());
"#,
    )?;
    write_new(
        &directory.join("src/Navigation/AppRoute.php"),
        r#"<?php

declare(strict_types=1);

namespace App\Navigation;

enum AppRoute: int
{
    case Home = 1;
}
"#,
    )?;
    write_new(
        &directory.join("src/Navigation/AppRouter.php"),
        r#"<?php

declare(strict_types=1);

namespace App\Navigation;

use App\Screens\Home\HomeScreen;
use Pam\Desktop\Navigation\Navigator;
use Pam\Desktop\Routing\Route;

final class AppRouter
{
    public static function make(): Navigator
    {
        return Route::stack(
            name: 'main',
            initial: AppRoute::Home,
            routes: static function (): void {
                Route::screen(AppRoute::Home, HomeScreen::class);
            },
            restoreState: false,
        );
    }
}
"#,
    )?;
    write_new(
        &directory.join("src/Services/ProductServer.php"),
        r#"<?php

declare(strict_types=1);

namespace App\Services;

use Product\Contracts\ProductSnapshot;
use Product\Contracts\ProductSurface;

/** Reads the Server's `GET /api/status` and validates it through the shared contract. */
final class ProductServer
{
    public const string DEFAULT_ENDPOINT = 'http://127.0.0.1:3000/api/status';

    public static function endpoint(string|false $configured): string
    {
        $endpoint = is_string($configured) && $configured !== '' ? $configured : self::DEFAULT_ENDPOINT;
        $parts = parse_url($endpoint);
        $scheme = is_array($parts) && is_string($parts['scheme'] ?? null) ? strtolower($parts['scheme']) : null;
        $host = is_array($parts) ? ($parts['host'] ?? null) : null;
        $loopback = is_string($host) && in_array(strtolower(trim($host, '[]')), ['127.0.0.1', 'localhost', '::1'], true);
        if (!is_string($host) || ($scheme !== 'https' && !($scheme === 'http' && $loopback))) {
            throw new \RuntimeException('PAM_PRODUCT_SERVER_URL must use HTTPS or loopback HTTP.');
        }

        return $endpoint;
    }

    public function status(): ProductSnapshot
    {
        $endpoint = self::endpoint(getenv('PAM_PRODUCT_SERVER_URL'));
        $context = stream_context_create([
            'http' => [
                'method' => 'GET',
                'header' => "Accept: application/json\r\n",
                'timeout' => 3,
                'follow_location' => 0,
                'max_redirects' => 0,
            ],
            'ssl' => ['verify_peer' => true, 'verify_peer_name' => true],
        ]);
        $body = @file_get_contents($endpoint, false, $context, 0, 65_537);
        if (!is_string($body) || strlen($body) > 65_536) {
            throw new \RuntimeException('Server is unavailable or returned too much data.');
        }
        $payload = json_decode($body, true, flags: JSON_THROW_ON_ERROR);
        if (!is_array($payload)) {
            throw new \UnexpectedValueException('Server returned an invalid document.');
        }
        $snapshot = ProductSnapshot::fromArray($payload);
        if ($snapshot->surface !== ProductSurface::Server) {
            throw new \UnexpectedValueException('Server returned the wrong product surface.');
        }

        return $snapshot;
    }
}
"#,
    )?;
    write_new(
        &directory.join("src/Screens/Home/HomeScreen.pam.php"),
        r#"<?php

declare(strict_types=1);

namespace App\Screens\Home;

use App\Services\ProductServer;
use Pam\Desktop\Component;
use Product\Contracts\ProductSurface;
use Product\Contracts\ReadinessState;

final class HomeScreen extends Component
{
    public string $headline = 'Checking the Server…';

    public string $serverState = 'Checking';

    public string $desktopState = 'Ready';

    public int $stateCode = 0;

    public function mount(): void
    {
        $this->refresh();
    }

    public function refresh(): void
    {
        try {
            $snapshot = (new ProductServer())->status();
            $this->stateCode = $snapshot->state->value;
            $this->headline = $snapshot->headline;
            $this->serverState = match ($snapshot->state) {
                ReadinessState::Operational => 'Operational',
                ReadinessState::Degraded => 'Degraded',
                ReadinessState::Offline => 'Offline',
            };
        } catch (\Throwable $error) {
            $this->stateCode = ReadinessState::Offline->value;
            $this->headline = $error->getMessage();
            $this->serverState = 'Offline';
        }
    }

    public function surfaceCode(): int
    {
        return ProductSurface::Desktop->value;
    }
}
?>

<template>
    <Column class="home">
        <Column class="content">
            <Text class="eyebrow">PRODUCT CONTROL CENTER</Text>
            <Text class="title">One domain, three surfaces</Text>
            <Text class="lead" testId="product-headline">{{ $headline }}</Text>
            <Column class="surfaces">
                <Row class="surface"><Text class="surface-name">Server</Text><Text class="surface-state" testId="product-server-state">{{ $serverState }}</Text></Row>
                <Row class="surface"><Text class="surface-name">Desktop</Text><Text class="surface-state">{{ $desktopState }}</Text></Row>
                <Row class="surface"><Text class="surface-name">Native</Text><Text class="surface-state">Not monitored from Desktop</Text></Row>
            </Column>
            <Row class="actions">
                <Button label="Refresh" on:press="refresh" class="primary" testId="product-refresh" />
            </Row>
        </Column>
    </Column>
</template>

<style scoped>
    .home { flex: 1; justify-content: center; align-items: center; padding: 40px; background-color: var(--background); }
    .content { width: 640px; max-width: 100%; gap: 14px; }
    .eyebrow { color: var(--accent); font-size: 12px; font-weight: 700; letter-spacing: 1px; }
    .title { color: var(--text); font-size: 34px; font-weight: 700; line-height: 42px; }
    .lead { color: var(--muted); font-size: 16px; line-height: 25px; }
    .surfaces { margin-top: 12px; padding: 20px; gap: 12px; border-radius: 10px; border-width: 1px; border-color: var(--border); background-color: var(--surface); }
    .surface { gap: 12px; align-items: center; }
    .surface-name { width: 110px; color: var(--text); font-size: 14px; font-weight: 700; }
    .surface-state { flex: 1; color: var(--muted); font-size: 14px; }
    .actions { gap: 18px; align-items: center; margin-top: 10px; }
    .primary { background-color: var(--accent); color: #FFFFFF; border-radius: 8px; height: 40px; padding-left: 18px; padding-right: 18px; font-size: 14px; font-weight: 600; }
    .primary:hover { background-color: var(--accent-strong); }
</style>
"#,
    )?;
    write_new(
        &directory.join("src/app.css"),
        r#"/* Shared design tokens and styles. Screen-specific rules belong to each .pam.php component. */
:root {
    --background: #F6F6F3;
    --surface: #FFFFFF;
    --border: #E2E3DD;
    --text: #17201C;
    --muted: #5B655F;
    --accent: #2F6B55;
    --accent-strong: #245744;
}

@media (prefers-color-scheme: dark) {
    :root {
        --background: #121514;
        --surface: #1A1E1C;
        --border: #2C322F;
        --text: #EEF1EF;
        --muted: #A3ADA7;
        --accent: #5FB592;
        --accent-strong: #4C9E7D;
    }
}

Text { font-family: sans-serif; }
"#,
    )?;
    write_new(
        &directory.join("tests/HomeScreenTest.php"),
        r#"<?php

declare(strict_types=1);

use App\Screens\Home\HomeScreen;
use App\Services\ProductServer;
use Pam\Desktop\App;

use function Pam\Desktop\Testing\mount;

require dirname(__DIR__).'/vendor/autoload.php';

App::components(dirname(__DIR__).'/src', dirname(__DIR__).'/.pam-desktop/components');

function check(bool $passed, string $what): void
{
    if (!$passed) {
        fwrite(STDERR, "failed: {$what}\n");
        exit(1);
    }
}

check(ProductServer::endpoint(false) === ProductServer::DEFAULT_ENDPOINT, 'the Server defaults to loopback');
try {
    ProductServer::endpoint('http://example.com/api/status');
    check(false, 'cleartext remote origins are rejected');
} catch (RuntimeException) {
}

putenv('PAM_PRODUCT_SERVER_URL=http://127.0.0.1:9/api/status');
$home = mount(HomeScreen::class);
check(str_contains($home->text(), 'Offline'), 'an unreachable Server is shown as offline');
$home->unmount();

echo "ok\n";
"#,
    )
}

/// An empty directory, or one that only holds version control (`.git`, a
/// directory in a new repository or a file in a `git worktree`).
fn only_version_control(directory: &Path) -> Result<bool, String> {
    for entry in directory.read_dir().map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_name() != ".git" {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The Composer package name of a new desktop application (`app/<slug>`):
/// from `--name` when given ("Pushin One" gives `pushin-one`), else from the
/// directory (`.` is the current directory's own name).
fn desktop_package_slug(directory: &Path, application_name: Option<&str>) -> String {
    let slugify = |name: &str| {
        let mut slug = String::new();
        for character in name.chars() {
            if character.is_ascii_alphanumeric() {
                slug.push(character.to_ascii_lowercase());
            } else if !slug.ends_with('-') {
                slug.push('-');
            }
        }
        slug.trim_matches('-').to_owned()
    };
    let from_directory = || {
        let resolved = directory
            .canonicalize()
            .unwrap_or_else(|_| directory.to_path_buf());
        resolved
            .file_name()
            .and_then(OsStr::to_str)
            .map(slugify)
            .filter(|slug| !slug.is_empty())
    };
    application_name
        .map(slugify)
        .filter(|slug| !slug.is_empty())
        .or_else(from_directory)
        .unwrap_or_else(|| "desktop-app".to_owned())
}

/// `--template desktop`: a PAM Desktop 2 application. Only the Composer
/// manifest is written here; `pam desktop init` writes the rest after the
/// install (`run_desktop_init`).
fn init_desktop(directory: &Path, application_name: Option<&str>) -> Result<(), String> {
    let slug = desktop_package_slug(directory, application_name);
    let manifest = serde_json::json!({
        "name": format!("app/{slug}"),
        "description": "A PAM Desktop application",
        "type": "project",
        "license": "proprietary",
        "require": {
            "php": "^8.5",
            "pushinbr/pam-desktop": "^2.1"
        },
        "autoload": {
            "psr-4": {
                "App\\": "src/"
            }
        },
        "scripts": {
            "dev": ["Composer\\Config::disableProcessTimeout", "pam-desktop dev"],
            "start": "pam-desktop run",
            "build": "pam-desktop build",
            "test": "php tests/HomeScreenTest.php"
        },
        "config": {
            "platform-check": true,
            "sort-packages": true
        }
    });
    write_new(
        &directory.join("composer.json"),
        &(serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot serialize Composer manifest: {error}"))?
            + "\n"),
    )
}

/// Which PAM Native UI line a native project starts on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UiLine {
    None,
    /// 2.x: `pam init --template native-ui` and the product workspace.
    Current,
}

fn init_mobile(
    directory: &Path,
    with_official_ui: bool,
    options: &InitOptions,
) -> Result<(), String> {
    init_mobile_with_ui(
        directory,
        if with_official_ui {
            UiLine::Current
        } else {
            UiLine::None
        },
        options,
    )
}

fn init_mobile_with_ui(directory: &Path, ui: UiLine, options: &InitOptions) -> Result<(), String> {
    let with_official_ui = ui != UiLine::None;
    let current_ui = ui == UiLine::Current;
    let native_repository = local_native_repository();
    let native_package = native_repository
        .as_ref()
        .map(|repository| repository.package.as_str())
        .unwrap_or("pushinbr/pam-native");
    let mut requirements = serde_json::json!({
        "php": "^8.5"
    });
    // PAM Native UI 2.2 resolves app theme tokens (pam-native.json
    // "theme.tokens"), compiled by PAM Native 1.53.9; earlier fixes it relies
    // on: input flush, pixel grid, dialog width, theme switch, iOS downloads.
    requirements[native_package] = serde_json::json!(if current_ui { "^1.53.9" } else { "^1.0" });
    if with_official_ui {
        requirements["pushinbr/pam-native-ui"] =
            serde_json::json!(if current_ui { "^2.2" } else { "^1.0" });
    }
    let mut manifest = serde_json::json!({
        "name": if with_official_ui {
            "app/pam-native-ui-project"
        } else {
            "app/pam-native-project"
        },
        "description": if with_official_ui {
            "A native Android application powered by PHP and PAM Native UI."
        } else {
            "A native Android application powered by persistent PHP."
        },
        "type": "project",
        "license": "proprietary",
        "require": requirements,
        "require-dev": {
            "phpunit/phpunit": "^12.5"
        },
        "autoload": {
            "psr-4": {
                "App\\": "src/"
            }
        },
        "config": {
            "platform": {
                "php": "8.5.0"
            },
            "platform-check": true,
            "sort-packages": true
        },
        "scripts": {
            "native:doctor": "pam doctor .",
            "native:dev": "pam dev .",
            "native:build": "pam build . --release",
            "native:benchmark": "pam benchmark .",
            "native:profile": "pam profile .",
            "test": "pam test . --phpunit -c phpunit.xml"
        }
    });
    if with_official_ui && native_package != "pushinbr/pam-native" {
        // Source checkouts may still expose the legacy package identity while
        // the public pushinbr namespace migration is being completed.
        manifest["replace"] = serde_json::json!({
            "pushinbr/pam-native": env!("CARGO_PKG_VERSION")
        });
    }
    let mut repositories = Vec::new();
    if let Some(repository) = native_repository {
        repositories.push(repository.definition);
    }
    if with_official_ui {
        if let Some(repository) = local_mobile_ui_repository() {
            repositories.push(repository);
        }
    }
    if !repositories.is_empty() {
        manifest["repositories"] = serde_json::json!(repositories);
    }
    write_new(
        &directory.join("composer.json"),
        &(serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot serialize Composer manifest: {error}"))?
            + "\n"),
    )?;
    let mut native_manifest = serde_json::json!({
        "$schema": "vendor/pushinbr/pam-native/resources/pam-native.schema.json",
        "version": 1,
        "applicationId": options.application_id.as_deref().unwrap_or("app.pam.hello"),
        "name": options.application_name.as_deref().unwrap_or("PAM App"),
        "entry": "index.php",
        "runtime": {"php": "8.5", "channel": "stable"},
        "versionCode": 1,
        "versionName": "0.1.0",
        "android": {"minSdk": 26, "targetSdk": 36, "permissions": [], "deepLinks": []},
        "ios": {"minimumVersion": "15.0"},
        "modules": [],
        "views": []
    });
    if current_ui {
        // The window paints the PAM Native UI theme before the first frame.
        native_manifest["appearance"] = serde_json::json!({
            "defaultMode": "system",
            "light": {"background": "#FDFDFC"},
            "dark": {"background": "#0A0A0A"}
        });
    }
    if options
        .mobile_platforms
        .contains(&MobilePlatform::AndroidTv)
    {
        // Older PAM Native manifests have no targets: write them only for TV.
        native_manifest["targets"] = serde_json::json!(
            options
                .mobile_platforms
                .iter()
                .map(|platform| platform.target())
                .collect::<Vec<_>>()
        );
    }
    write_new(
        &directory.join("pam-native.json"),
        &(serde_json::to_string_pretty(&native_manifest)
            .map_err(|error| format!("cannot serialize PAM Native manifest: {error}"))?
            + "\n"),
    )?;
    let entry = if with_official_ui {
        r#"<?php

declare(strict_types=1);

use App\Hello;
use Pam\Native\App;

require __DIR__.'/vendor/autoload.php';

App::components(__DIR__.'/src', __DIR__.'/.pam-native/components');
App::run(App::make(Hello::class));
"#
    } else {
        r#"<?php

declare(strict_types=1);

use App\Hello;
use Pam\Native\App;

require __DIR__.'/vendor/autoload.php';

App::components(__DIR__.'/src', __DIR__.'/.pam-native/components');
App::theme(\Pam\Native\Theme::pamLab());
App::run(new Hello());
"#
    };
    write_new(&directory.join("index.php"), entry)?;
    fs::create_dir_all(directory.join("src"))
        .map_err(|error| format!("cannot create mobile source directory: {error}"))?;
    let starter = options.mobile_starter.unwrap_or(MobileStarter::Blank);
    let (hello_path, hello) = if with_official_ui {
        (
            directory.join("src/Hello.pam"),
            mobile_ui_starter(
                starter,
                options.application_name.as_deref().unwrap_or("PAM App"),
            ),
        )
    } else {
        (
            directory.join("src/Hello.php"),
            r#"<?php

declare(strict_types=1);

namespace App;

use Pam\Native\Component;
use Pam\Native\Element;
use Pam\Native\Style;
use Pam\Native\UI\Button;
use Pam\Native\UI\Column;
use Pam\Native\UI\SafeAreaView;
use Pam\Native\UI\Screen;
use Pam\Native\UI\Text;

final class Hello extends Component
{
    private int $count = 0;

    public function render(): Element
    {
        return Screen::make(
            SafeAreaView::make(
                Column::make(
                    Text::make('Hello from persistent PHP')
                        ->style(new Style(
                            textColor: 0xFFF8FAFC,
                            fontSize: 28,
                            fontWeight: 700,
                        )),
                    Button::make('Native taps: '.$this->count)
                        ->style(new Style(
                            backgroundColor: 0xFF4ADE80,
                            textColor: 0xFF052E16,
                            minHeight: 48,
                            paddingHorizontal: 16,
                            borderRadius: 12,
                        ))
                        ->onPress($this->increment(...)),
                )->style(new Style(flexGrow: 1, padding: 24, gap: 16)),
            )->style(new Style(flexGrow: 1)),
        )->style(new Style(flexGrow: 1, backgroundColor: 0xFF0F172A));
    }

    public function increment(): void
    {
        $this->count++;
    }
}
"#
            .to_owned(),
        )
    };

    fn mobile_ui_starter(starter: MobileStarter, application_name: &str) -> String {
        // PAM Native UI 2 starters: every input is bound with p-model, so
        // what the user types is in the component's PHP state.
        let (state, methods, content) = match starter {
            MobileStarter::Blank => (
                "#[State]\n    public int $count = 0;\n\n    #[State]\n    public string $name = '';",
                "public function increment(): void\n    {\n        $this->count++;\n    }",
                r#"
    <p-screen title="__APP_NAME__" subtitle="Your native PHP application is ready.">
        <p-card title="Persistent PHP, native UI" subtitle="Type below: the greeting is rendered by PHP.">
            <p-text-field p-model="$name" label="Your name" placeholder="Ada" clearable="true" />
            <Text>{{ $name === '' ? 'Hello!' : 'Hello, '.$name.'!' }}</Text>
            <p-btn label="Native taps: {{ $count }}" block="true" @press="increment" />
        </p-card>
    </p-screen>"#,
            ),
            MobileStarter::Tabs => (
                "#[State]\n    public string $tab = 'home';",
                "public function title(): string\n    {\n        return match ($this->tab) {\n            'search' => 'Search',\n            'profile' => 'Profile',\n            default => 'Home',\n        };\n    }",
                r#"
    <p-screen :title="title()" subtitle="A tabs starter. Each tab is one PHP branch.">
        <p-tabs p-model="$tab" :options="['home' => 'Home', 'search' => 'Search', 'profile' => 'Profile']" />
        <p-empty-state p-if="$tab === 'home'" icon="info" title="Home" text="Build your home screen here." />
        <p-empty-state p-else-if="$tab === 'search'" icon="search" title="Search" text="Results will appear here." />
        <p-section p-else title="Profile">
            <p-list-item title="Account" subtitle="Signed in" chevron="true" />
            <p-list-item title="Notifications" chevron="true" :divider="false" />
        </p-section>
    </p-screen>"#,
            ),
            MobileStarter::Authentication => (
                "#[State]\n    public string $email = '';\n\n    #[State]\n    public string $password = '';\n\n    #[State]\n    public ?string $error = null;\n\n    #[State]\n    public bool $signedIn = false;",
                "public function submit(): void\n    {\n        $this->error = filter_var($this->email, FILTER_VALIDATE_EMAIL) === false\n            ? 'Enter a valid email address.'\n            : (strlen($this->password) < 8 ? 'Use at least 8 characters.' : null);\n        $this->signedIn = $this->error === null;\n    }",
                r#"
    <p-screen title="Welcome back" subtitle="Sign in to continue.">
        <p-alert p-if="$signedIn" tone="success" :text="'Signed in as '.$email" />
        <p-form :error="$error">
            <p-text-field p-model="$email" label="Email" type="email" required="true" />
            <p-text-field p-model="$password" label="Password" type="password" required="true" @submit="submit" />
        </p-form>
        <template #footer>
            <p-btn label="Sign in" block="true" size="lg" @press="submit" />
        </template>
    </p-screen>"#,
            ),
            MobileStarter::Ecommerce => (
                "#[State]\n    public string $query = '';\n\n    #[State]\n    public int $cart = 0;",
                "public function add(): void\n    {\n        $this->cart++;\n    }",
                r#"
    <p-screen title="__APP_NAME__" subtitle="Native commerce starter">
        <p-text-field p-model="$query" placeholder="Search products" type="search" clearable="true" />
        <p-card title="Featured" :subtitle="$query === '' ? 'Catalog, cart and checkout foundations.' : 'Searching for '.$query">
            <p-badge label="New" />
            <p-btn label="Add to cart · {{ $cart }}" @press="add" />
        </p-card>
    </p-screen>"#,
            ),
            MobileStarter::Chat => (
                "#[State]\n    public string $message = '';\n\n    /** @var list<array{id: int, text: string}> */\n    #[State]\n    public array $messages = [];",
                "public function send(): void\n    {\n        if (trim($this->message) === '') {\n            return;\n        }\n        $this->messages[] = ['id' => count($this->messages) + 1, 'text' => trim($this->message)];\n        $this->message = '';\n    }",
                r#"
    <p-screen title="Team chat" subtitle="Messages stay above the keyboard.">
        <p-list-item p-for="$item in $messages" :key="$item['id']" :title="$item['text']" avatar="You" />
        <p-empty-state p-if="$messages === []" icon="info" title="No messages yet" text="Say hello below." />
        <template #footer>
            <p-text-field p-model="$message" placeholder="Message" returnKey="send" @submit="send" />
            <p-btn label="Send" block="true" @press="send" />
        </template>
    </p-screen>"#,
            ),
            MobileStarter::Showcase => (
                "#[State]\n    public string $name = '';\n\n    #[State]\n    public bool $notifications = true;\n\n    #[State]\n    public ?string $plan = 'pro';\n\n    #[State]\n    public float $volume = 40;",
                "public function summary(): string\n    {\n        return ($this->name === '' ? 'Nobody' : $this->name).' · '.$this->plan.' · '.($this->notifications ? 'notifications on' : 'notifications off').' · '.$this->volume.'%';\n    }",
                r#"
    <p-screen title="Component showcase" subtitle="PAM Native UI 2">
        <p-text-field p-model="$name" label="Name" />
        <p-switch p-model="$notifications" label="Notifications" />
        <p-radio-group p-model="$plan" label="Plan" :options="['free' => 'Free', 'pro' => 'Pro']" />
        <p-slider p-model="$volume" label="Volume" :step="5" showValue="true" unit="%" />
        <p-card title="PHP state"><Text>{{ summary() }}</Text></p-card>
    </p-screen>"#,
            ),
        };
        r#"<?php

declare(strict_types=1);

namespace App;

use Pam\Native\Attributes\State;
use Pam\Native\Component;

final class Hello extends Component
{
    __STATE__

    __METHODS__
}
?>

<template>__CONTENT__
</template>
"#
        .replace("__STATE__", state)
        .replace("__METHODS__", methods)
        .replace("__CONTENT__", content)
        .replace("__APP_NAME__", application_name)
    }
    write_new(&hello_path, &hello)?;
    fs::create_dir_all(directory.join("tests"))
        .map_err(|error| format!("cannot create mobile test directory: {error}"))?;
    write_new(
        &directory.join("phpunit.xml"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<phpunit bootstrap="vendor/autoload.php" colors="true" cacheDirectory=".pam/phpunit-cache">
    <testsuites>
        <testsuite name="PAM Native application">
            <directory>tests</directory>
        </testsuite>
    </testsuites>
</phpunit>
"#,
    )?;
    write_new(
        &directory.join("tests/ApplicationTest.php"),
        r#"<?php

declare(strict_types=1);

use PHPUnit\Framework\TestCase;

final class ApplicationTest extends TestCase
{
    public function testNativeApplicationManifestIsPresent(): void
    {
        self::assertFileExists(dirname(__DIR__).'/pam-native.json');
        self::assertFileExists(dirname(__DIR__).'/index.php');
    }
}
"#,
    )?;
    write_new(
        &directory.join(".gitignore"),
        "/vendor/\n/.pam/\n/.pam-native/\n",
    )?;
    fs::create_dir_all(directory.join(".vscode"))
        .map_err(|error| format!("cannot create VS Code settings directory: {error}"))?;
    write_new(
        &directory.join(".vscode/settings.json"),
        r#"{
    "files.associations": {
        "*.pam": "pam",
        "*.pam.php": "pam"
    },
    "[pam]": {
        "editor.defaultFormatter": "pushin.pam-native",
        "editor.formatOnSave": true
    },
    "html.customData": [
        "./vendor/pushinbr/pam-native/resources/pam-native.custom-data.json"
    ]
}
"#,
    )
}

fn init_laravel(executable: &OsStr, options: &InitOptions) -> Result<u8, String> {
    let directory = &options.directory;
    if directory.exists()
        && directory
            .read_dir()
            .map_err(|error| error.to_string())?
            .next()
            .is_some()
    {
        return Err(format!("{} is not empty", directory.display()));
    }

    let mut arguments = vec![
        OsString::from("create-project"),
        OsString::from("laravel/laravel"),
        directory.as_os_str().to_os_string(),
        OsString::from("^13.0"),
        OsString::from("--prefer-dist"),
        OsString::from("--no-interaction"),
        OsString::from("--no-progress"),
    ];
    if !options.install {
        arguments.push(OsString::from("--no-install"));
        arguments.push(OsString::from("--no-scripts"));
    }
    eprintln!("Creating a Laravel application for Pam...");
    let status = composer::run(executable, &arguments)?;
    if status != 0 {
        return Err(format!(
            "Composer could not create the Laravel project (status {status}); partial files were left in {} for inspection",
            directory.display()
        ));
    }
    if !directory.join(".env").is_file() && directory.join(".env.example").is_file() {
        fs::copy(directory.join(".env.example"), directory.join(".env"))
            .map_err(|error| format!("cannot create Laravel .env: {error}"))?;
    }

    let socket_setup = if options.socket {
        r#"
$socket = new \Pam\WS\Server();
$socket->on('connection', static function (\Pam\WS\Socket $client): void {
    $client->emit('welcome', ['message' => 'Connected to Pam Socket']);
});
"#
    } else {
        ""
    };
    write_new(
        &directory.join("pam.php"),
        &format!(
            r#"<?php

declare(strict_types=1);

use Pam\Laravel\Application;
{socket_setup}
$app = Application::boot(__DIR__);
$app->listen(
    port: (int) (getenv('PAM_PORT') ?: 3000),
    host: (string) (getenv('PAM_HOST') ?: '127.0.0.1'),
    options: [
        // Laravel managers and facades are process-global. Pam keeps one request
        // active per worker and scales safely with `pam start --workers N`.
        'maxConcurrentRequests' => 1,
        'responseStreamQueueCapacity' => 16,
        'maxResponseBytes' => 256 * 1024 * 1024,
        'maxResponseChunkBytes' => 1024 * 1024,
        'exposeErrors' => filter_var(
            getenv('APP_DEBUG') ?: 'false',
            FILTER_VALIDATE_BOOLEAN,
        ),
    ],
);
"#,
        ),
    )?;
    fs::write(
        directory.join("routes/api.php"),
        r#"<?php

use Illuminate\Support\Facades\Route;

Route::get('/ping', static fn (): array => ['message' => 'pong']);
"#,
    )
    .map_err(|error| format!("cannot configure Laravel routes: {error}"))?;
    configure_laravel_routing(directory)?;
    configure_laravel_manifest(directory)?;

    write_pam_manifest(directory, InitTemplate::Laravel, options)?;
    print_init_success(
        directory,
        InitTemplate::Laravel,
        options.socket,
        options.install,
    );
    Ok(0)
}

fn configure_laravel_routing(directory: &Path) -> Result<(), String> {
    let path = directory.join("bootstrap/app.php");
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if contents.contains("api: __DIR__.'/../routes/api.php'") {
        return Ok(());
    }
    let needle = "        web: __DIR__.'/../routes/web.php',\n";
    let replacement = concat!(
        "        web: __DIR__.'/../routes/web.php',\n",
        "        api: __DIR__.'/../routes/api.php',\n"
    );
    let configured = contents.replacen(needle, replacement, 1);
    if configured == contents {
        return Err("Laravel bootstrap/app.php has an unsupported routing layout".to_owned());
    }
    fs::write(&path, configured)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(())
}

fn configure_laravel_manifest(directory: &Path) -> Result<(), String> {
    let path = directory.join("composer.json");
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|error| format!("invalid {}: {error}", path.display()))?;
    let scripts = manifest
        .get_mut("scripts")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| "Laravel composer.json has no scripts object".to_owned())?;
    scripts.insert("pam:dev".to_owned(), serde_json::json!("pam dev pam.php"));
    scripts.insert(
        "pam:start".to_owned(),
        serde_json::json!("pam start pam.php"),
    );
    fs::write(
        &path,
        serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot serialize {}: {error}", path.display()))?
            + "\n",
    )
    .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    Ok(())
}

fn local_packages_repository() -> Option<serde_json::Value> {
    if std::env::var_os("PAM_NO_LOCAL_PACKAGES").is_some() {
        return None;
    }
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/*");
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packages/nimbus/composer.json")
        .is_file()
        .then(|| {
            serde_json::json!({
                "type": "path",
                "url": packages.to_string_lossy(),
                "options": {
                    "symlink": false,
                    "versions": {
                        "pushinbr/pam-contracts": "1.1.0",
                        "pushinbr/nimbus": "0.6.0",
                        "pushinbr/pam-http-psr": "1.0.0",
                        "pushinbr/pam-socket": "1.1.0",
                        "pushinbr/pam-http-testing": "1.0.0"
                    }
                }
            })
        })
}

struct LocalComposerRepository {
    package: String,
    definition: serde_json::Value,
}

fn local_native_repository() -> Option<LocalComposerRepository> {
    let configured = std::env::var_os("PAM_NATIVE_PACKAGE_PATH").map(PathBuf::from);
    // Source checkouts may contain an unpublished version whose CLI assets do
    // not exist yet. Local packages must be explicitly selected by developers.
    [configured]
        .into_iter()
        .flatten()
        .find(|path| path.join("composer.json").is_file())
        .and_then(|path| {
            let path = fs::canonicalize(&path).unwrap_or(path);
            let composer = fs::read(path.join("composer.json")).ok()?;
            let manifest = serde_json::from_slice::<serde_json::Value>(&composer).ok()?;
            let package = manifest.get("name")?.as_str()?.to_owned();
            let version = cargo_manifest_version(&path.join("Cargo.toml")).or_else(|| {
                path.parent()?
                    .parent()
                    .and_then(|root| cargo_manifest_version(&root.join("Cargo.toml")))
            })?;
            let definition = serde_json::json!({
                "type": "path",
                "url": path.to_string_lossy(),
                "options": {
                    "symlink": false,
                    "versions": {
                        package.clone(): version
                    }
                }
            });
            Some(LocalComposerRepository {
                package,
                definition,
            })
        })
}

fn cargo_manifest_version(path: &Path) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    contents.lines().find_map(|line| {
        let value = line.trim().strip_prefix("version = ")?;
        let version = value.strip_prefix('"')?.strip_suffix('"')?;
        let parts = version.split('.').collect::<Vec<_>>();
        if parts.len() != 3 || parts.iter().any(|part| part.parse::<u32>().is_err()) {
            return None;
        }
        Some(version.to_owned())
    })
}

fn local_mobile_ui_repository() -> Option<serde_json::Value> {
    let configured = std::env::var_os("PAM_MOBILE_UI_PACKAGE_PATH").map(PathBuf::from);
    // Use the published UI by default. A sibling checkout can include build
    // caches and unpublished files that Composer cannot safely copy.
    [configured]
        .into_iter()
        .flatten()
        .find(|path| path.join("composer.json").is_file())
        .map(|path| {
            let path = fs::canonicalize(&path).unwrap_or(path);
            serde_json::json!({
                "type": "path",
                "url": path.to_string_lossy(),
                "options": {
                    "symlink": false,
                    "versions": {
                        "pushinbr/pam-native-ui": "2.2.0"
                    }
                }
            })
        })
}

fn run_composer_in(executable: &OsStr, directory: &Path, arguments: &[&str]) -> Result<(), String> {
    let previous = std::env::current_dir()
        .map_err(|error| format!("cannot resolve current directory: {error}"))?;
    std::env::set_current_dir(directory)
        .map_err(|error| format!("cannot enter {}: {error}", directory.display()))?;
    let arguments = arguments.iter().map(OsString::from).collect::<Vec<_>>();
    let result = composer::run(executable, &arguments);
    let restore = std::env::set_current_dir(&previous)
        .map_err(|error| format!("cannot restore {}: {error}", previous.display()));
    let status = result?;
    restore?;
    if status != 0 {
        return Err(format!("Composer exited with status {status}"));
    }
    Ok(())
}

fn print_init_success(directory: &Path, template: InitTemplate, socket: bool, installed: bool) {
    let preset = match (template, socket) {
        (InitTemplate::Raw, false) => "raw",
        (InitTemplate::Raw, true) => "raw + Socket",
        (InitTemplate::Api, false) => "API",
        (InitTemplate::Api, true) => "API + Socket",
        (InitTemplate::Web, false) => "Web · login",
        (InitTemplate::Web, true) => unreachable!("web does not support --socket"),
        (InitTemplate::Laravel, false) => "Laravel",
        (InitTemplate::Laravel, true) => "Laravel + Socket",
        (InitTemplate::Desktop, false) => "Desktop",
        (InitTemplate::Desktop, true) => unreachable!("desktop does not support --socket"),
        (InitTemplate::Mobile, false) => "Mobile · Core",
        (InitTemplate::Mobile, true) => unreachable!("mobile does not support --socket"),
        (InitTemplate::MobileUi, false) => "Mobile · Official UI",
        (InitTemplate::MobileUi, true) => unreachable!("mobile UI does not support --socket"),
        (InitTemplate::Product, false) => "Product · Server + Native + Desktop",
        (InitTemplate::Product, true) => unreachable!("product does not support --socket"),
    };
    let ui = Terminal::stdout();
    println!();
    println!("{}", ui.success("● PROJECT ONLINE"));
    println!("{}", ui.rule());
    println!(
        "  {} {}",
        ui.muted(format!("{:<12}", "Preset")),
        ui.heading(preset)
    );
    println!(
        "  {} {}",
        ui.muted(format!("{:<12}", "Directory")),
        directory.display()
    );
    let next = if template == InitTemplate::Product {
        format!("cd {} && read README.md", directory.display())
    } else if template == InitTemplate::Desktop && installed {
        format!("cd {} && pam desktop dev", directory.display())
    } else if template == InitTemplate::Desktop {
        format!(
            "cd {} && pam composer install && pam desktop init && pam desktop dev",
            directory.display()
        )
    } else if matches!(
        template,
        InitTemplate::Desktop | InitTemplate::Mobile | InitTemplate::MobileUi
    ) {
        format!("cd {} && pam dev .", directory.display())
    } else {
        let entry = if template == InitTemplate::Laravel {
            "pam.php"
        } else {
            "index.php"
        };
        format!("cd {} && pam dev {entry}", directory.display())
    };
    println!(
        "  {} {}",
        ui.muted(format!("{:<12}", "Next")),
        ui.command(next)
    );
}

pub fn benchmark(url: &str, requests: usize, concurrency: usize) -> Result<u8, String> {
    if requests == 0 || concurrency == 0 {
        return Err("benchmark requests and concurrency must be positive".to_owned());
    }
    let endpoint = HttpEndpoint::parse(url)?;
    let next = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = std::sync::mpsc::channel();
    let started = Instant::now();
    let workers = concurrency.min(requests);
    let mut threads = Vec::with_capacity(workers);

    for _ in 0..workers {
        let endpoint = endpoint.clone();
        let next = next.clone();
        let sender = sender.clone();
        threads.push(std::thread::spawn(move || {
            loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                if index >= requests {
                    break;
                }
                let began = Instant::now();
                let successful = endpoint
                    .request()
                    .is_ok_and(|status| (200..500).contains(&status));
                let _ = sender.send((began.elapsed().as_secs_f64() * 1_000.0, successful));
            }
        }));
    }
    drop(sender);
    let mut latencies = Vec::with_capacity(requests);
    let mut successful = 0;
    for (latency, ok) in receiver {
        latencies.push(latency);
        successful += usize::from(ok);
    }
    for thread in threads {
        thread
            .join()
            .map_err(|_| "a benchmark worker panicked".to_owned())?;
    }
    latencies.sort_by(f64::total_cmp);
    let elapsed = started.elapsed().as_secs_f64();
    let percentile = |fraction: f64| -> f64 {
        let index = ((latencies.len().saturating_sub(1)) as f64 * fraction).round() as usize;
        latencies.get(index).copied().unwrap_or_default()
    };
    let ui = Terminal::stdout();
    let failed = requests - successful;
    println!("{}  {}", ui.brand("PAM / HTTP BENCHMARK"), ui.muted(url));
    println!("{}", ui.rule());
    println!(
        "  {} {:>12}",
        ui.muted(format!("{:<20}", "Requests")),
        requests
    );
    println!(
        "  {} {:>12}",
        ui.muted(format!("{:<20}", "Successful")),
        ui.success(successful)
    );
    println!(
        "  {} {:>12}",
        ui.muted(format!("{:<20}", "Failed")),
        if failed == 0 {
            ui.success(failed)
        } else {
            ui.danger(failed)
        }
    );
    println!(
        "  {} {:>12.2}",
        ui.muted(format!("{:<20}", "Throughput / sec")),
        requests as f64 / elapsed.max(f64::EPSILON)
    );
    println!("{}", ui.rule());
    println!(
        "  {} {:>9.3} ms",
        ui.muted(format!("{:<20}", "Latency p50")),
        percentile(0.50)
    );
    println!(
        "  {} {:>9.3} ms",
        ui.muted(format!("{:<20}", "Latency p95")),
        percentile(0.95)
    );
    println!(
        "  {} {:>9.3} ms",
        ui.muted(format!("{:<20}", "Latency p99")),
        percentile(0.99)
    );
    Ok(u8::from(successful != requests))
}

fn loaded_runtime(
    executable: &OsStr,
    script: &Path,
    arguments: &[OsString],
) -> Result<PhpRuntime, String> {
    let mut runtime = PhpRuntime::initialize(executable, script, arguments)?;
    let status = runtime.execute_file_quiet(script)?;
    if status != 0 {
        return Err(format!("PHP script exited with status {status}"));
    }
    Ok(runtime)
}

fn write_new(path: &Path, contents: &str) -> Result<(), String> {
    if path.exists() {
        return Err(format!("refusing to overwrite {}", path.display()));
    }
    fs::write(path, contents).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

#[derive(Clone)]
struct HttpEndpoint {
    host: String,
    port: u16,
    target: String,
    bearer: Option<Arc<admin_auth::AdminCredential>>,
}

impl HttpEndpoint {
    fn parse(url: &str) -> Result<Self, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or("benchmark currently requires an http:// URL")?;
        let (authority, target) = rest.split_once('/').unwrap_or((rest, ""));
        let (host, port) = authority
            .rsplit_once(':')
            .map(|(host, port)| {
                port.parse::<u16>()
                    .map(|port| (host.to_owned(), port))
                    .map_err(|_| "invalid benchmark URL port".to_owned())
            })
            .transpose()?
            .unwrap_or_else(|| (authority.to_owned(), 80));
        if host.is_empty() {
            return Err("benchmark URL host is empty".to_owned());
        }
        Ok(Self {
            host,
            port,
            target: format!("/{target}"),
            bearer: None,
        })
    }

    fn with_optional_bearer_from_environment(mut self) -> Result<Self, String> {
        self.bearer = admin_auth::load()?.map(Arc::new);
        Ok(self)
    }

    fn request(&self) -> Result<u16, String> {
        let response = self.response()?;
        let first_line = response
            .split(|byte| *byte == b'\n')
            .next()
            .and_then(|line| std::str::from_utf8(line).ok())
            .ok_or("invalid HTTP response")?;
        first_line
            .split_whitespace()
            .nth(1)
            .ok_or("HTTP status is missing")?
            .parse()
            .map_err(|_| "invalid HTTP status".to_owned())
    }

    fn response_body(&self) -> Result<String, String> {
        let response = self.response()?;
        let first_line = response
            .split(|byte| *byte == b'\n')
            .next()
            .and_then(|line| std::str::from_utf8(line).ok())
            .ok_or("invalid HTTP response")?;
        let mut status = first_line.split_whitespace();
        let _version = status.next();
        let code = status
            .next()
            .ok_or("HTTP status is missing")?
            .parse::<u16>()
            .map_err(|_| "invalid HTTP status".to_owned())?;
        if !(200..300).contains(&code) {
            let reason = status.collect::<Vec<_>>().join(" ");
            let suffix = if reason.is_empty() {
                String::new()
            } else {
                format!(" {reason}")
            };
            return Err(format!("control plane returned HTTP {code}{suffix}"));
        }
        let boundary = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .ok_or("HTTP response body boundary is missing")?;
        String::from_utf8(response[boundary + 4..].to_vec())
            .map_err(|error| format!("HTTP response body is not UTF-8: {error}"))
    }

    fn response(&self) -> Result<Vec<u8>, String> {
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|error| error.to_string())?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .map_err(|error| error.to_string())?;
        let authorization = self.authorization_header();
        write!(
            stream,
            "GET {} HTTP/1.1\r\nHost: {}\r\n{authorization}Connection: close\r\n\r\n",
            self.target, self.host
        )
        .map_err(|error| error.to_string())?;
        let mut response = Vec::new();
        stream
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut response)
            .map_err(|error| error.to_string())?;
        if response.len() > 8 * 1024 * 1024 {
            return Err("HTTP response exceeds the 8 MiB safety limit".to_owned());
        }
        Ok(response)
    }

    fn authorization_header(&self) -> String {
        self.bearer
            .as_ref()
            .map(|credential| format!("Authorization: Bearer {}\r\n", credential.as_str()))
            .unwrap_or_default()
    }
}

pub fn default_script(target: Option<OsString>) -> PathBuf {
    PathBuf::from(target.unwrap_or_else(|| OsString::from("index.php")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn desktop_package_names_come_from_the_name_then_the_directory() {
        let root = std::env::temp_dir().join(format!("pam-init-slug-{}", std::process::id()));
        let directory = root.join("x");
        std::fs::create_dir_all(&directory).unwrap();
        assert_eq!(
            super::desktop_package_slug(&directory, Some("Pushin One")),
            "pushin-one"
        );
        assert_eq!(
            super::desktop_package_slug(&directory, Some("  Ação!! App ")),
            "a-o-app"
        );
        assert_eq!(super::desktop_package_slug(&directory, None), "x");
        assert_eq!(super::desktop_package_slug(&directory, Some("!!!")), "x");
        // Only `.git` (a new repository, or the file of a worktree) counts as empty.
        assert!(super::only_version_control(&directory).unwrap());
        std::fs::create_dir_all(directory.join(".git")).unwrap();
        assert!(super::only_version_control(&directory).unwrap());
        std::fs::remove_dir_all(directory.join(".git")).unwrap();
        std::fs::write(directory.join(".git"), "gitdir: ../.git/worktrees/x\n").unwrap();
        assert!(super::only_version_control(&directory).unwrap());
        std::fs::write(directory.join("README.md"), "").unwrap();
        assert!(!super::only_version_control(&directory).unwrap());
        std::fs::remove_dir_all(&root).unwrap();
    }

    use super::{
        HttpEndpoint, MobilePlatform, current_worker_lag_seconds, parse_control_plane_diagnostics,
        visible_top_metric,
    };

    #[test]
    fn mobile_platforms_accept_android_tv_and_comma_lists() {
        assert_eq!(
            MobilePlatform::parse("android,ios,android-tv").unwrap(),
            vec![
                MobilePlatform::Android,
                MobilePlatform::Ios,
                MobilePlatform::AndroidTv
            ]
        );
        assert_eq!(
            MobilePlatform::parse("tv").unwrap(),
            vec![MobilePlatform::AndroidTv]
        );
        assert_eq!(
            MobilePlatform::parse("all, tv, android").unwrap(),
            vec![
                MobilePlatform::Android,
                MobilePlatform::Ios,
                MobilePlatform::AndroidTv
            ]
        );
        assert!(MobilePlatform::parse("roku").is_err());
        assert!(MobilePlatform::parse(",").is_err());
        assert_eq!(MobilePlatform::AndroidTv.target(), "android-tv");
    }

    #[test]
    fn classifies_only_finite_current_worker_lag_samples() {
        assert_eq!(
            current_worker_lag_seconds(
                "pam_worker_event_loop_lag_seconds{worker=\"2\",pid=\"42\"} 0.012500"
            ),
            Some(0.0125)
        );
        assert_eq!(
            current_worker_lag_seconds(
                "pam_worker_event_loop_lag_max_seconds{worker=\"2\"} 1.500000"
            ),
            None
        );
        assert!(visible_top_metric(
            "pam_worker_event_loop_lag_seconds{worker=\"2\"} 0.012500"
        ));
        assert!(visible_top_metric(
            "pam_pool_event_loop_lag_average_seconds{pool=\"web\"} 0.001000"
        ));
        assert!(!visible_top_metric("process_cpu_seconds 1"));
        assert_eq!(
            current_worker_lag_seconds("pam_worker_event_loop_lag_seconds{worker=\"2\"} NaN"),
            None
        );
        assert_eq!(
            current_worker_lag_seconds("pam_worker_event_loop_lag_seconds{worker=\"2\"} -0.1"),
            None
        );
    }

    #[test]
    fn validates_control_plane_diagnostics_before_streaming() {
        let valid = r#"{"schemaVersion":1,"surfaceCode":1,"resultCode":1,"generation":2,"desiredWorkers":1,"readyWorkers":1,"workers":[{"workerId":3,"generation":2,"pid":42,"pool":"web","lifecycleCode":2,"resultCode":1,"currentLagMicros":1000,"maxLagMicros":2000,"averageLagMicros":1500,"lagSampleCount":2}]}"#;
        let diagnostics = parse_control_plane_diagnostics(valid).unwrap();
        assert_eq!(diagnostics.workers[0].worker_id, 3);

        for invalid in [
            valid.replace("\"schemaVersion\":1", "\"schemaVersion\":2"),
            valid.replace("\"lifecycleCode\":2", "\"lifecycleCode\":9"),
            valid.replace("\"pid\":42", "\"pid\":42,\"unknown\":1"),
        ] {
            assert!(parse_control_plane_diagnostics(&invalid).is_err());
        }
    }

    #[test]
    fn scopes_admin_bearer_to_an_explicit_endpoint_request() {
        let mut endpoint = HttpEndpoint::parse("http://127.0.0.1:3010/diagnostics").unwrap();
        assert!(endpoint.authorization_header().is_empty());
        endpoint.bearer = Some(std::sync::Arc::new(
            crate::admin_auth::validate("0123456789abcdef0123456789abcdef".to_owned()).unwrap(),
        ));
        assert_eq!(
            endpoint.authorization_header(),
            "Authorization: Bearer 0123456789abcdef0123456789abcdef\r\n"
        );
    }
}
