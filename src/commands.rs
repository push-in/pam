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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitTemplate {
    Raw,
    Api,
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
            "api" => Ok(Self::Api),
            "laravel" => Ok(Self::Laravel),
            "desktop" => Ok(Self::Desktop),
            "mobile" | "android" | "mobile-pure" => Ok(Self::Mobile),
            "mobile-ui" | "android-ui" | "mobile+ui" => Ok(Self::MobileUi),
            "product" | "fullstack" | "flagship" => Ok(Self::Product),
            _ => Err(format!(
                "unknown init template {value:?}; expected raw, api, laravel, desktop, mobile, mobile-ui, or product"
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
}

impl MobilePlatform {
    pub fn parse(value: &str) -> Result<Vec<Self>, String> {
        match value {
            "android" => Ok(vec![Self::Android]),
            "ios" => Ok(vec![Self::Ios]),
            "all" | "both" => Ok(vec![Self::Android, Self::Ios]),
            _ => Err("platform requires android, ios, or all".to_owned()),
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
    if template == InitTemplate::Laravel {
        return init_laravel(executable, &options);
    }

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
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    if template == InitTemplate::Raw {
        init_raw(directory, socket)?;
    } else if template == InitTemplate::Desktop {
        init_desktop(directory)?;
    } else if template == InitTemplate::Product {
        init_product(directory, &options)?;
    } else if matches!(template, InitTemplate::Mobile | InitTemplate::MobileUi) {
        init_mobile(directory, template == InitTemplate::MobileUi, &options)?;
    } else {
        init_api(directory, socket)?;
    }
    write_pam_manifest(directory, template, &options)?;

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
    }
    print_init_success(directory, template, socket);
    Ok(0)
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
            MobilePlatform::parse(&prompt_value("Platforms: android, ios, or all", "all")?)?
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
        InitTemplate::Api => ProjectKind::Api,
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
        ui.heading(format!("{:<25}", "API + Socket")),
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
        ui.muted("Servo shell + PHP")
    );
    println!(
        "  {}  {} {}",
        ui.accent("07"),
        ui.heading(format!("{:<25}", "Mobile · Core")),
        ui.muted("Pure PAM Native primitives")
    );
    println!(
        "  {}  {} {}",
        ui.accent("08"),
        ui.heading(format!("{:<25}", "Mobile · Official UI")),
        ui.muted("PAM Mobile UI design system · recommended for apps")
    );
    println!(
        "  {}  {} {}",
        ui.accent("09"),
        ui.heading(format!("{:<25}", "Product workspace")),
        ui.muted("Server + Native + Desktop + shared PHP contracts")
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

fn init_api(directory: &Path, socket: bool) -> Result<(), String> {
    let local_packages = local_packages_repository();
    let api_version = "^2.0";
    let ecosystem_version = "^1.0";
    let mut require = serde_json::Map::from_iter([
        ("php".to_owned(), serde_json::json!("^8.4")),
        (
            "pushinbr/pam-api".to_owned(),
            serde_json::json!(api_version),
        ),
    ]);
    if socket {
        require.insert(
            "pushinbr/pam-socket".to_owned(),
            serde_json::json!(ecosystem_version),
        );
    }
    let mut manifest = serde_json::json!({
        "name": "app/pam-project",
        "description": "A PHP application powered by the PAM runtime.",
        "type": "project",
        "license": "proprietary",
        "require": require,
        "require-dev": {
            "laravel/pint": "^1.30",
            "phpunit/phpunit": "^12.5"
        },
        "autoload": {"psr-4": {"App\\": "src/"}},
        "config": {"platform-check": true, "sort-packages": true},
        "scripts": {
            "dev": "pam dev index.php",
            "migrate": "pam bin/migrate",
            "start": "pam start index.php",
            "test": "pam test . --phpunit -c phpunit.xml"
        }
    });
    if let Some(repository) = local_packages {
        manifest["repositories"] = serde_json::json!([repository]);
    }
    write_new(
        &directory.join("composer.json"),
        &(serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot serialize Composer manifest: {error}"))?
            + "\n"),
    )?;

    let mut index = include_str!("../packages/skeleton/index.php").to_owned();
    if socket {
        index = index.replace(
            "$app = new App();",
            r#"$app = new App();

$socket = \Pam\Socket\Server::create();
$socket->on('connection', static function (\Pam\WS\Socket $client): void {
    $client->emit('welcome', ['message' => 'Connected to Pam Socket']);
});"#,
        );
    }
    write_new(&directory.join("index.php"), &index)?;
    for (path, contents) in API_STARTER_FILES {
        let target = directory.join(path);
        let parent = target
            .parent()
            .ok_or_else(|| format!("starter file has no parent: {}", target.display()))?;
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        write_new(&target, contents)?;
    }
    Ok(())
}

const API_STARTER_FILES: &[(&str, &str)] = &[
    (
        ".env.example",
        include_str!("../packages/skeleton/.env.example"),
    ),
    (
        ".gitignore",
        include_str!("../packages/skeleton/.gitignore"),
    ),
    (
        "phpunit.xml",
        include_str!("../packages/skeleton/phpunit.xml"),
    ),
    (
        "src/Http/Controllers/PingController.php",
        include_str!("../packages/skeleton/src/Http/Controllers/PingController.php"),
    ),
    (
        "src/Http/Resources/PingResource.php",
        include_str!("../packages/skeleton/src/Http/Resources/PingResource.php"),
    ),
    (
        "src/Http/Controllers/ProductController.php",
        include_str!("../packages/skeleton/src/Http/Controllers/ProductController.php"),
    ),
    (
        "src/Http/Requests/StoreProductRequest.php",
        include_str!("../packages/skeleton/src/Http/Requests/StoreProductRequest.php"),
    ),
    (
        "src/Http/Resources/ProductResource.php",
        include_str!("../packages/skeleton/src/Http/Resources/ProductResource.php"),
    ),
    (
        "src/Domain/Products/CreateProductData.php",
        include_str!("../packages/skeleton/src/Domain/Products/CreateProductData.php"),
    ),
    (
        "src/Domain/Products/ProductStatus.php",
        include_str!("../packages/skeleton/src/Domain/Products/ProductStatus.php"),
    ),
    (
        "src/Models/Product.php",
        include_str!("../packages/skeleton/src/Models/Product.php"),
    ),
    (
        "src/Providers/AppServiceProvider.php",
        include_str!("../packages/skeleton/src/Providers/AppServiceProvider.php"),
    ),
    (
        "src/Repositories/ProductRepository.php",
        include_str!("../packages/skeleton/src/Repositories/ProductRepository.php"),
    ),
    (
        "src/Repositories/EloquentProductRepository.php",
        include_str!("../packages/skeleton/src/Repositories/EloquentProductRepository.php"),
    ),
    (
        "src/Services/ProductService.php",
        include_str!("../packages/skeleton/src/Services/ProductService.php"),
    ),
    (
        "src/Services/ReadinessService.php",
        include_str!("../packages/skeleton/src/Services/ReadinessService.php"),
    ),
    (
        "src/Services/ReadinessSnapshot.php",
        include_str!("../packages/skeleton/src/Services/ReadinessSnapshot.php"),
    ),
    (
        "src/Services/ReadinessStatus.php",
        include_str!("../packages/skeleton/src/Services/ReadinessStatus.php"),
    ),
    (
        "tests/ApplicationTest.php",
        include_str!("../packages/skeleton/tests/ApplicationTest.php"),
    ),
    (
        "tests/bootstrap.php",
        include_str!("../packages/skeleton/tests/bootstrap.php"),
    ),
    (
        "bin/migrate",
        include_str!("../packages/skeleton/bin/migrate"),
    ),
    (
        "database/migrations/2026_08_21_000000_create_products.php",
        include_str!(
            "../packages/skeleton/database/migrations/2026_08_21_000000_create_products.php"
        ),
    ),
];

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
    "require": {"php": "^8.4"},
    "autoload": {"psr-4": {"Product\\Contracts\\": "src/"}},
    "config": {"platform-check": true, "sort-packages": true}
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
    init_mobile(&native, true, options)?;
    init_desktop(&desktop)?;
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

use Pam\App;
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

use Pam\App;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Api\Testing\TestClient;
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
    write_new(
        &native.join("src/ProductTheme.php"),
        r##"<?php

declare(strict_types=1);

namespace App;

use Pam\MobileUi\Enum\ColorToken;
use Pam\MobileUi\PamUI;
use Pam\MobileUi\Theme\Color;
use Pam\MobileUi\Theme\Theme;
use Pam\MobileUi\Theme\Themes;

final class ProductTheme
{
    private const array REQUIRED_ROLES = [
        'background', 'surface', 'surfaceRaised', 'foreground',
        'mutedForeground', 'border', 'primary', 'onPrimary',
        'success', 'warning', 'danger', 'focus',
    ];

    private function __construct()
    {
    }

    public static function install(): void
    {
        $path = dirname(__DIR__, 3).'/packages/contracts/design-tokens.json';
        $contents = file_get_contents($path, false, null, 0, 32_769);
        if (!is_string($contents) || strlen($contents) > 32_768) {
            throw new \RuntimeException('Product design token contract is missing or too large.');
        }
        $document = json_decode($contents, true, flags: JSON_THROW_ON_ERROR);
        if (!is_array($document)
            || array_keys($document) !== ['schemaVersion', 'themes', 'spacing', 'radii', 'motionMs', 'minimumTouchTarget']
            || $document['schemaVersion'] !== 1
            || !is_array($document['themes'])
            || count($document['themes']) !== 2) {
            throw new \UnexpectedValueException('Product design token contract is incompatible.');
        }
        $light = self::theme($document['themes'][0], 1, 'light', Themes::light());
        $dark = self::theme($document['themes'][1], 2, 'dark', Themes::dark());
        PamUI::theme($light, $dark);
    }

    /** @param array<array-key, mixed> $payload */
    private static function theme(array $payload, int $modeCode, string $name, Theme $base): Theme
    {
        if (array_keys($payload) !== ['modeCode', 'name', 'colors']
            || $payload['modeCode'] !== $modeCode
            || $payload['name'] !== $name
            || !is_array($payload['colors'])
            || array_keys($payload['colors']) !== self::REQUIRED_ROLES) {
            throw new \UnexpectedValueException('Product theme is incompatible.');
        }
        $colors = $payload['colors'];

        return $base->withColors([
            ColorToken::Background->value => self::color($colors['background']),
            ColorToken::SurfaceSunken->value => self::color($colors['background']),
            ColorToken::Card->value => self::color($colors['surface']),
            ColorToken::Surface->value => self::color($colors['surface']),
            ColorToken::Popover->value => self::color($colors['surfaceRaised']),
            ColorToken::SurfaceElevated->value => self::color($colors['surfaceRaised']),
            ColorToken::Foreground->value => self::color($colors['foreground']),
            ColorToken::OnSurface->value => self::color($colors['foreground']),
            ColorToken::PopoverForeground->value => self::color($colors['foreground']),
            ColorToken::MutedForeground->value => self::color($colors['mutedForeground']),
            ColorToken::Border->value => self::color($colors['border']),
            ColorToken::Input->value => self::color($colors['border']),
            ColorToken::Primary->value => self::color($colors['primary']),
            ColorToken::PrimaryForeground->value => self::color($colors['onPrimary']),
            ColorToken::Success->value => self::color($colors['success']),
            ColorToken::Warning->value => self::color($colors['warning']),
            ColorToken::Destructive->value => self::color($colors['danger']),
            ColorToken::Focus->value => self::color($colors['focus']),
            ColorToken::Ring->value => self::color($colors['focus']),
        ]);
    }

    private static function color(mixed $value): Color
    {
        if (!is_string($value) || preg_match('/^#[0-9a-f]{6}$/D', $value) !== 1) {
            throw new \UnexpectedValueException('Product color must use canonical lowercase hex.');
        }

        return Color::rgb(
            (int) hexdec(substr($value, 1, 2)),
            (int) hexdec(substr($value, 3, 2)),
            (int) hexdec(substr($value, 5, 2)),
        );
    }
}
"##,
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

final class Hello extends Component
{
    private const MAX_PENDING_MUTATIONS = 32;

    #[State]
    public int $refreshCount = 0;

    #[State]
    public int $serverStateCode = 3;

    #[State]
    public string $serverHeadline = 'Server not checked yet';

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
        ProductTheme::install();
        $this->outbox = new OfflineMutationQueue;
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

                        $this->serverStateCode = $snapshot->state->value;
                        $this->serverHeadline = $snapshot->headline;
                        $this->syncMessage = 'Verified contract v'.$snapshot->version->value.' from Server surface 1.';
                    } catch (\Throwable) {
                        $this->serverStateCode = 3;
                        $this->serverHeadline = 'Server unavailable or incompatible';
                        $this->syncMessage = 'Check PAM_PRODUCT_SERVER_URL and the Server contract version.';
                    } finally {
                        $this->syncing = false;
                    }
                },
                headers: ['Accept' => 'application/json'],
                timeoutMs: 5_000,
            );
        } catch (\Throwable) {
            $this->serverStateCode = 3;
            $this->serverHeadline = 'Server request could not start';
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
    <PamUIProvider mode="system">
        <SafeAreaView class="flex-1 ui-surface">
            <Center class="flex-1 px-6">
                <Card class="w-full max-w-md gap-5 p-6">
                    <Badge variant="secondary"><BadgeText>Contract v1 · Native surface 2</BadgeText></Badge>
                    <Heading size="2xl">{{ $this->snapshot()->headline }}</Heading>
                    <Text class="text-muted-foreground">One typed PHP contract across every PAM runtime.</Text>
                    <Card class="gap-2 p-4">
                        <Text>Server state · code {{ $serverStateCode }}</Text>
                        <Heading size="md">{{ $serverHeadline }}</Heading>
                        <Text class="text-muted-foreground">{{ $syncMessage }}</Text>
                    </Card>
                    <Button size="lg" on:press="refresh">
                        <ButtonText>Sync Server · {{ $refreshCount }}</ButtonText>
                    </Button>
                    <Card class="gap-2 p-4">
                        <Text>Offline outbox · {{ $pendingMutations }} pending</Text>
                        <Text class="text-muted-foreground">{{ $mutationMessage }}</Text>
                    </Card>
                    <Button size="lg" variant="outline" on:press="queueCheckIn">
                        <ButtonText>Queue resilient check-in</ButtonText>
                    </Button>
                </Card>
            </Center>
        </SafeAreaView>
    </PamUIProvider>
</template>
"#,
    )?;
    replace_generated_once(
        &desktop.join("app.php"),
        "use Pam\\Desktop\\WindowTheme;",
        "use Pam\\Desktop\\WindowTheme;\nuse Product\\Contracts\\MutationDeliveryState;\nuse Product\\Contracts\\ProductMutation;\nuse Product\\Contracts\\ProductMutationReceipt;\nuse Product\\Contracts\\ProductSnapshot;\nuse Product\\Contracts\\ProductSurface;\nuse Product\\Contracts\\ReadinessState;",
        "desktop contract import",
    )?;
    replace_generated_once(
        &desktop.join("app.php"),
        "}\n\nHelloApp::run();",
        r#"    #[Command('product.status')]
    public function productStatus(): array
    {
        return ProductSnapshot::operational(ProductSurface::Desktop)->toArray();
    }

    #[Command('product.theme')]
    public function productTheme(int $modeCode): array
    {
        if (!in_array($modeCode, [1, 2], true)) {
            throw new \InvalidArgumentException('Product theme mode must be 1 or 2.');
        }
        $path = dirname(__DIR__, 2).'/packages/contracts/design-tokens.json';
        $contents = file_get_contents($path, false, null, 0, 32_769);
        if (!is_string($contents) || strlen($contents) > 32_768) {
            throw new \RuntimeException('Product design token contract is missing or too large.');
        }
        $document = json_decode($contents, true, flags: JSON_THROW_ON_ERROR);
        if (!is_array($document)
            || array_keys($document) !== ['schemaVersion', 'themes', 'spacing', 'radii', 'motionMs', 'minimumTouchTarget']
            || $document['schemaVersion'] !== 1
            || !is_array($document['themes'])
            || count($document['themes']) !== 2) {
            throw new \UnexpectedValueException('Product design token contract is incompatible.');
        }
        $theme = $document['themes'][$modeCode - 1] ?? null;
        $roles = ['background', 'surface', 'surfaceRaised', 'foreground', 'mutedForeground', 'border', 'primary', 'onPrimary', 'success', 'warning', 'danger', 'focus'];
        if (!is_array($theme)
            || array_keys($theme) !== ['modeCode', 'name', 'colors']
            || $theme['modeCode'] !== $modeCode
            || $theme['name'] !== ($modeCode === 1 ? 'light' : 'dark')
            || !is_array($theme['colors'])
            || array_keys($theme['colors']) !== $roles) {
            throw new \UnexpectedValueException('Product theme is incompatible.');
        }
        foreach ($theme['colors'] as $color) {
            if (!is_string($color) || preg_match('/^#[0-9a-f]{6}$/D', $color) !== 1) {
                throw new \UnexpectedValueException('Product theme contains an invalid color.');
            }
        }

        return $theme;
    }

    #[Command('product.server-status')]
    public function productServerStatus(): array
    {
        $startedAt = hrtime(true);
        try {
            $snapshot = $this->fetchProductServerStatus();
            $this->recordProductTelemetrySample(
                $snapshot->state->value,
                self::elapsedProductMilliseconds($startedAt),
            );
            return $snapshot->toArray();
        } catch (\Throwable $error) {
            $this->recordProductTelemetrySample(
                ReadinessState::Offline->value,
                self::elapsedProductMilliseconds($startedAt),
            );
            throw $error;
        }
    }

    #[Command('product.telemetry-history')]
    public function productTelemetryHistory(): array
    {
        return [
            'versionCode' => 1,
            'samples' => $this->readProductTelemetryHistory(),
        ];
    }

    private function fetchProductServerStatus(): ProductSnapshot
    {
        $endpoint = self::productEndpoint('PAM_PRODUCT_SERVER_URL', 'http://127.0.0.1:3000/api/status');

        $context = stream_context_create([
            'http' => [
                'method' => 'GET',
                'header' => "Accept: application/json\r\n",
                'timeout' => 5,
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

    #[Command('product.check-in')]
    public function productCheckIn(): array
    {
        $mutation = ProductMutation::checkIn('check-in:'.bin2hex(random_bytes(16)));
        $outbox = $this->readProductOutbox();
        if (count($outbox) >= 32) {
            throw new \OverflowException('Desktop product outbox is full.');
        }
        $outbox[] = $mutation;
        $this->writeProductOutbox($outbox);
        $remaining = $this->replayProductOutbox();
        $queued = array_any(
            $remaining,
            static fn (ProductMutation $item): bool => hash_equals($item->idempotencyKey, $mutation->idempotencyKey),
        );

        return [
            'deliveryStateCode' => $queued ? MutationDeliveryState::Queued->value : MutationDeliveryState::Delivered->value,
            'pendingCount' => count($remaining),
        ];
    }

    #[Command('product.outbox.replay')]
    public function replayProductOutboxCommand(): array
    {
        $remaining = $this->replayProductOutbox();
        return [
            'deliveryStateCode' => $remaining === []
                ? MutationDeliveryState::Delivered->value
                : MutationDeliveryState::Queued->value,
            'pendingCount' => count($remaining),
        ];
    }

    /** @return list<ProductMutation> */
    private function replayProductOutbox(): array
    {
        $remaining = $this->readProductOutbox();
        while (($mutation = $remaining[0] ?? null) instanceof ProductMutation) {
            try {
                $this->sendProductMutation($mutation);
                array_shift($remaining);
            } catch (\Throwable) {
                break;
            }
        }
        $this->writeProductOutbox($remaining);
        return $remaining;
    }

    private function sendProductMutation(ProductMutation $mutation): void
    {
        $endpoint = self::productEndpoint('PAM_PRODUCT_MUTATION_URL', 'http://127.0.0.1:3000/api/check-ins');
        $body = json_encode($mutation->toArray(), JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        $context = stream_context_create([
            'http' => [
                'method' => 'POST',
                'header' => "Accept: application/json\r\nContent-Type: application/json\r\nIdempotency-Key: {$mutation->idempotencyKey}\r\n",
                'content' => $body,
                'timeout' => 5,
                'follow_location' => 0,
                'max_redirects' => 0,
            ],
            'ssl' => ['verify_peer' => true, 'verify_peer_name' => true],
        ]);
        $response = @file_get_contents($endpoint, false, $context, 0, 65_537);
        if (!is_string($response) || strlen($response) > 65_536) {
            throw new \RuntimeException('Mutation endpoint is unavailable or returned too much data.');
        }
        $payload = json_decode($response, true, flags: JSON_THROW_ON_ERROR);
        if (!is_array($payload)) {
            throw new \UnexpectedValueException('Mutation endpoint returned an invalid document.');
        }
        $receipt = ProductMutationReceipt::fromArray($payload);
        if (!hash_equals($mutation->idempotencyKey, $receipt->idempotencyKey)) {
            throw new \UnexpectedValueException('Mutation receipt key does not match.');
        }
    }

    /** @return list<ProductMutation> */
    private function readProductOutbox(): array
    {
        $path = self::productOutboxPath();
        if (!is_file($path)) {
            return [];
        }
        if (is_link($path) || filesize($path) > 65_536) {
            throw new \RuntimeException('Desktop product outbox is unsafe or too large.');
        }
        $contents = file_get_contents($path, false, null, 0, 65_537);
        if (!is_string($contents) || strlen($contents) > 65_536) {
            throw new \RuntimeException('Desktop product outbox could not be read safely.');
        }
        $payload = json_decode($contents, true, flags: JSON_THROW_ON_ERROR);
        if (!is_array($payload) || ($payload['versionCode'] ?? null) !== 1 || !is_array($payload['mutations'] ?? null)
            || count($payload['mutations']) > 32) {
            throw new \UnexpectedValueException('Desktop product outbox is incompatible.');
        }
        return array_map(
            static function (mixed $item): ProductMutation {
                if (!is_array($item)) {
                    throw new \UnexpectedValueException('Desktop product outbox entry is invalid.');
                }
                return ProductMutation::fromArray($item);
            },
            array_values($payload['mutations']),
        );
    }

    /** @param list<ProductMutation> $mutations */
    private function writeProductOutbox(array $mutations): void
    {
        if (count($mutations) > 32) {
            throw new \OverflowException('Desktop product outbox is full.');
        }
        $path = self::productOutboxPath();
        if (is_link($path)) {
            throw new \RuntimeException('Desktop product outbox cannot be a symbolic link.');
        }
        $encoded = json_encode([
            'versionCode' => 1,
            'mutations' => array_map(static fn (ProductMutation $item): array => $item->toArray(), $mutations),
        ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        if (strlen($encoded) > 65_536) {
            throw new \OverflowException('Desktop product outbox exceeds 64 KiB.');
        }

        self::writeProductStorageDocument($path, $encoded, 'outbox');
    }

    /** @return list<array{observedAtUnixMs: int, latencyMs: int, stateCode: int}> */
    private function readProductTelemetryHistory(): array
    {
        $path = self::productTelemetryPath();
        if (!is_file($path)) {
            return [];
        }
        if (is_link($path) || filesize($path) > 16_384) {
            throw new \RuntimeException('Desktop product telemetry history is unsafe or too large.');
        }
        $contents = file_get_contents($path, false, null, 0, 16_385);
        if (!is_string($contents) || strlen($contents) > 16_384) {
            throw new \RuntimeException('Desktop product telemetry history could not be read safely.');
        }
        $payload = json_decode($contents, true, flags: JSON_THROW_ON_ERROR);
        if (!is_array($payload) || array_keys($payload) !== ['versionCode', 'samples']
            || ($payload['versionCode'] ?? null) !== 1 || !is_array($payload['samples'] ?? null)
            || count($payload['samples']) > 24) {
            throw new \UnexpectedValueException('Desktop product telemetry history is incompatible.');
        }
        $previousTimestamp = 0;
        foreach ($payload['samples'] as $sample) {
            if (!is_array($sample) || array_keys($sample) !== ['observedAtUnixMs', 'latencyMs', 'stateCode']
                || !is_int($sample['observedAtUnixMs']) || $sample['observedAtUnixMs'] < $previousTimestamp
                || !is_int($sample['latencyMs']) || $sample['latencyMs'] < 0 || $sample['latencyMs'] > 30_000
                || !is_int($sample['stateCode']) || ReadinessState::tryFrom($sample['stateCode']) === null) {
                throw new \UnexpectedValueException('Desktop product telemetry sample is incompatible.');
            }
            $previousTimestamp = $sample['observedAtUnixMs'];
        }
        return array_values($payload['samples']);
    }

    private function appendProductTelemetrySample(int $stateCode, int $latencyMs): void
    {
        if (ReadinessState::tryFrom($stateCode) === null || $latencyMs < 0 || $latencyMs > 30_000) {
            throw new \UnexpectedValueException('Desktop product telemetry sample is out of bounds.');
        }
        $samples = $this->readProductTelemetryHistory();
        $samples[] = [
            'observedAtUnixMs' => (int) floor(microtime(true) * 1_000),
            'latencyMs' => $latencyMs,
            'stateCode' => $stateCode,
        ];
        $samples = array_slice($samples, -24);
        $encoded = json_encode([
            'versionCode' => 1,
            'samples' => $samples,
        ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        if (strlen($encoded) > 16_384) {
            throw new \OverflowException('Desktop product telemetry history exceeds 16 KiB.');
        }
        self::writeProductStorageDocument(self::productTelemetryPath(), $encoded, 'telemetry history');
    }

    private function recordProductTelemetrySample(int $stateCode, int $latencyMs): void
    {
        try {
            $this->appendProductTelemetrySample($stateCode, $latencyMs);
        } catch (\Throwable $error) {
            error_log('PAM Product telemetry sample was not persisted: '.$error->getMessage());
        }
    }

    private static function elapsedProductMilliseconds(int $startedAt): int
    {
        return min(30_000, max(0, (int) floor((hrtime(true) - $startedAt) / 1_000_000)));
    }

    private static function writeProductStorageDocument(string $path, string $encoded, string $label): void
    {
        if (is_link($path)) {
            throw new \RuntimeException("Desktop product {$label} cannot be a symbolic link.");
        }
        $temporary = $path.'.'.bin2hex(random_bytes(8)).'.tmp';
        $handle = fopen($temporary, 'x+b');
        if ($handle === false) {
            throw new \RuntimeException("Desktop product {$label} temporary file could not be created.");
        }
        try {
            $written = 0;
            while ($written < strlen($encoded)) {
                $bytes = fwrite($handle, substr($encoded, $written));
                if (!is_int($bytes) || $bytes <= 0) {
                    throw new \RuntimeException("Desktop product {$label} could not be persisted.");
                }
                $written += $bytes;
            }
            if (!fflush($handle)) {
                throw new \RuntimeException("Desktop product {$label} could not be persisted.");
            }
            if (function_exists('fsync') && !fsync($handle)) {
                throw new \RuntimeException("Desktop product {$label} could not be synchronized.");
            }
            @chmod($temporary, 0600);
            fclose($handle);
            $handle = null;
            if (!rename($temporary, $path)) {
                throw new \RuntimeException("Desktop product {$label} could not be committed atomically.");
            }
        } finally {
            if (is_resource($handle)) {
                fclose($handle);
            }
            if (is_file($temporary)) {
                @unlink($temporary);
            }
        }
    }

    private static function productOutboxPath(): string
    {
        $directory = realpath(__DIR__.'/storage');
        if (!is_string($directory) || is_link(__DIR__.'/storage')) {
            throw new \RuntimeException('Desktop product storage is unavailable or unsafe.');
        }
        return $directory.DIRECTORY_SEPARATOR.'product-outbox-v1.json';
    }

    private static function productTelemetryPath(): string
    {
        $directory = realpath(__DIR__.'/storage');
        if (!is_string($directory) || is_link(__DIR__.'/storage')) {
            throw new \RuntimeException('Desktop product storage is unavailable or unsafe.');
        }
        return $directory.DIRECTORY_SEPARATOR.'product-telemetry-v1.json';
    }

    private static function productEndpoint(string $variable, string $fallback): string
    {
        $endpoint = getenv($variable) ?: $fallback;
        $parts = parse_url($endpoint);
        $scheme = is_array($parts) && is_string($parts['scheme'] ?? null)
            ? strtolower($parts['scheme'])
            : null;
        $host = is_array($parts) ? ($parts['host'] ?? null) : null;
        $loopback = is_string($host) && in_array(strtolower($host), ['127.0.0.1', 'localhost', '::1'], true);
        if (!is_string($host) || ($scheme !== 'https' && !($scheme === 'http' && $loopback))) {
            throw new \RuntimeException("{$variable} must use HTTPS or loopback HTTP.");
        }
        return $endpoint;
    }
}

HelloApp::run();"#,
        "desktop product command",
    )?;
    replace_generated_once(
        &desktop.join("resources/index.html"),
        "            <section class=\"runtime-strip\" aria-label=\"Componentes da aplicação\">",
        r#"            <section class="product-console" aria-labelledby="product-console-title">
                <header class="product-console__header">
                    <div>
                        <span class="eyebrow">PRODUCT CONTROL CENTER · LIVE CONTRACT</span>
                        <h2 id="product-console-title">Um domínio.<br><strong>Três superfícies.</strong></h2>
                        <p id="product-headline">Consultando o contrato PHP compartilhado…</p>
                    </div>
                    <div class="product-console__actions">
                        <button id="product-refresh" type="button">Atualizar sinais</button>
                        <span id="product-sample-time">Nenhuma amostra concluída</span>
                    </div>
                </header>

                <div class="product-console__grid">
                    <section class="product-panel product-panel--surfaces" aria-labelledby="product-surfaces-title">
                        <div class="product-panel__heading">
                            <div>
                                <span>ECOSSISTEMA</span>
                                <h3 id="product-surfaces-title">Superfícies</h3>
                            </div>
                            <span class="product-summary" id="product-summary">0 de 3 confirmadas</span>
                        </div>
                        <ul class="surface-list" aria-label="Disponibilidade por superfície">
                            <li id="surface-server" data-state="checking">
                                <span class="surface-state" aria-hidden="true"></span>
                                <span><strong>Server</strong><small id="surface-server-detail">Verificando endpoint…</small></span>
                                <span class="surface-code">01</span>
                            </li>
                            <li id="surface-native" data-state="unknown">
                                <span class="surface-state" aria-hidden="true"></span>
                                <span><strong>Native</strong><small>Não monitorado nesta sessão Desktop</small></span>
                                <span class="surface-code">02</span>
                            </li>
                            <li id="surface-desktop" data-state="checking">
                                <span class="surface-state" aria-hidden="true"></span>
                                <span><strong>Desktop</strong><small id="surface-desktop-detail">Aguardando worker PHP…</small></span>
                                <span class="surface-code">03</span>
                            </li>
                        </ul>
                    </section>

                    <section class="product-panel product-panel--contract" aria-labelledby="product-contract-title">
                        <div class="product-panel__heading">
                            <div><span>COMPATIBILIDADE</span><h3 id="product-contract-title">Contrato</h3></div>
                            <span class="product-badge" id="product-contract-badge">verificando</span>
                        </div>
                        <dl class="product-metrics">
                            <div><dt>Versão</dt><dd id="product-version-code">—</dd></div>
                            <div><dt>Origem</dt><dd id="product-surface-code">—</dd></div>
                            <div><dt>Estado</dt><dd id="product-state-code">—</dd></div>
                            <div><dt>Latência</dt><dd><span id="product-latency">—</span><small> ms</small></dd></div>
                        </dl>
                        <p id="product-status" class="product-feedback" role="status" aria-live="polite">Aguardando o worker PHP.</p>
                    </section>

                    <section class="product-panel product-panel--outbox" aria-labelledby="product-outbox-title">
                        <div class="product-panel__heading">
                            <div><span>OFFLINE FIRST</span><h3 id="product-outbox-title">Outbox Desktop</h3></div>
                            <strong><span id="product-outbox-count">—</span><small> / 32</small></strong>
                        </div>
                        <meter id="product-outbox-meter" min="0" max="32" value="0" aria-labelledby="product-outbox-title" aria-describedby="product-outbox-status">0 de 32 operações</meter>
                        <p id="product-outbox-status" class="product-feedback" role="status" aria-live="polite">Carregando operações preservadas…</p>
                        <button id="product-check-in" class="secondary-action" type="button">Criar check-in resiliente</button>
                        <p class="product-privacy">Persistência privada e limitada; não armazene tokens, credenciais ou dados pessoais.</p>
                    </section>

                    <section class="product-panel product-panel--history" aria-labelledby="product-history-title">
                        <div class="product-panel__heading">
                            <div><span>ÚLTIMAS 24 AMOSTRAS</span><h3 id="product-history-title">Latência e disponibilidade</h3></div>
                            <span class="product-badge">local</span>
                        </div>
                        <p id="product-history-summary" class="product-history-summary" role="status" aria-live="polite">
                            Faça uma consulta para iniciar o histórico local.
                        </p>
                        <ol id="product-history-chart" class="product-history-chart" aria-label="Histórico cronológico de consultas ao Server">
                            <li class="product-history-empty">Nenhuma amostra disponível.</li>
                        </ol>
                        <div class="product-history-legend" aria-label="Legenda dos estados">
                            <span data-state="ready">Operacional</span>
                            <span data-state="degraded">Degradado</span>
                            <span data-state="offline">Offline</span>
                        </div>
                    </section>
                </div>
            </section>

            <section class="runtime-strip" aria-label="Componentes da aplicação">"#,
        "desktop product status surface",
    )?;
    let desktop_css = fs::read_to_string(desktop.join("resources/styles.css"))
        .map_err(|error| format!("cannot read generated desktop styles: {error}"))?;
    replace_generated(
        &desktop.join("resources/styles.css"),
        &(desktop_css
            + r##"

.product-console {
    margin: 0 0 32px;
    padding: clamp(20px, 3vw, 36px);
    border: 1px solid var(--line);
    border-radius: 24px;
    background: linear-gradient(145deg, var(--surface-raised), var(--ink));
    box-shadow: var(--shadow);
}

.product-console__header { display: flex; align-items: end; justify-content: space-between; gap: 32px; margin-bottom: 28px; }
.product-console__header h2 { margin: 8px 0 12px; font-size: clamp(30px, 4vw, 54px); line-height: .98; letter-spacing: -.04em; }
.product-console__header h2 strong { color: var(--cyan); font-family: Georgia, serif; font-weight: 500; }
.product-console__header p { max-width: 60ch; margin: 0; color: var(--text-soft); line-height: 1.55; }
.product-console__actions { display: grid; flex: 0 0 auto; gap: 10px; justify-items: end; }
.product-console button { min-height: 48px; padding: 0 18px; color: var(--run-ink); background: var(--run); border: 1px solid transparent; border-radius: 10px; font-weight: 700; cursor: pointer; transition: background-color 180ms ease, border-color 180ms ease, color 180ms ease; }
.product-console button:hover { background: #91efbd; }
.product-console button:active { background: #53ce8e; }
.product-console button:disabled { cursor: wait; opacity: .55; }
.product-console button:focus-visible { outline: 3px solid var(--cyan); outline-offset: 3px; }
#product-sample-time { color: var(--text-faint); font: 500 11px/1.4 "JetBrains Mono", monospace; }
.product-console__grid { display: grid; grid-template-columns: minmax(0, 1.25fr) minmax(260px, .75fr); gap: 16px; }
.product-panel { min-width: 0; padding: 22px; border: 1px solid var(--line); border-radius: 16px; background: var(--ink-soft); }
.product-panel--surfaces { grid-row: span 2; }
.product-panel__heading { display: flex; align-items: start; justify-content: space-between; gap: 16px; margin-bottom: 18px; }
.product-panel__heading span, .product-panel__heading small { color: var(--text-faint); font: 600 10px/1.3 "JetBrains Mono", monospace; letter-spacing: .08em; text-transform: uppercase; }
.product-panel__heading h3 { margin: 4px 0 0; font-size: 20px; letter-spacing: -.02em; }
.product-summary, .product-badge { padding: 7px 9px; border: 1px solid var(--line); border-radius: 999px; white-space: nowrap; }
.product-badge[data-state="ready"] { color: var(--run); border-color: rgba(103, 232, 165, .35); }
.product-badge[data-state="error"] { color: var(--coral); border-color: rgba(255, 146, 121, .4); }
.surface-list { display: grid; gap: 10px; margin: 0; padding: 0; list-style: none; }
.surface-list li { min-height: 72px; display: grid; grid-template-columns: 12px minmax(0, 1fr) auto; gap: 14px; align-items: center; padding: 14px 16px; border: 1px solid var(--line); border-radius: 12px; background: var(--surface-raised); }
.surface-list strong, .surface-list small { display: block; }
.surface-list small { margin-top: 4px; color: var(--text-soft); line-height: 1.4; }
.surface-state { width: 9px; height: 9px; border: 2px solid var(--text-faint); border-radius: 50%; }
.surface-list li[data-state="ready"] .surface-state { border-color: var(--run); background: var(--run); box-shadow: 0 0 0 4px rgba(103, 232, 165, .1); }
.surface-list li[data-state="degraded"] .surface-state { border-color: #f6b85f; background: #f6b85f; }
.surface-list li[data-state="offline"] .surface-state { border-color: var(--coral); background: var(--coral); }
.surface-code { color: var(--text-faint); font: 600 12px/1 "JetBrains Mono", monospace; font-variant-numeric: tabular-nums; }
.product-metrics { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); margin: 0; }
.product-metrics div { padding: 4px 12px; border-left: 1px solid var(--line); }
.product-metrics div:first-child { padding-left: 0; border-left: 0; }
.product-metrics dt { color: var(--text-faint); font-size: 11px; }
.product-metrics dd { margin: 7px 0 0; font: 600 26px/1 "JetBrains Mono", monospace; font-variant-numeric: tabular-nums; }
.product-metrics dd small { font-size: 10px; color: var(--text-faint); }
.product-feedback { min-height: 38px; margin: 18px 0 0; color: var(--text-soft); font-size: 12px; line-height: 1.5; }
.product-panel--outbox meter { width: 100%; height: 12px; accent-color: var(--run); }
.product-panel--outbox .secondary-action { width: 100%; margin-top: 14px; color: var(--text); border-color: var(--line-strong); background: transparent; }
.product-panel--outbox .secondary-action:hover { border-color: var(--cyan); background: rgba(104, 222, 210, .07); }
.product-privacy { margin: 12px 0 0; color: var(--text-faint); font-size: 11px; line-height: 1.5; }
.product-panel--history { grid-column: 1 / -1; }
.product-history-summary { margin: -4px 0 18px; color: var(--text-soft); line-height: 1.5; }
.product-history-chart { min-height: 180px; display: flex; align-items: end; gap: clamp(4px, .8vw, 10px); margin: 0; padding: 20px 4px 0; border-bottom: 1px solid var(--line-strong); list-style: none; }
.product-history-chart li:not(.product-history-empty) { min-width: 0; flex: 1 1 0; display: grid; grid-template-rows: 120px auto auto; gap: 7px; align-items: end; justify-items: center; }
.product-history-bar { width: min(100%, 20px); min-height: 4px; height: var(--sample-height); border: 1px solid currentColor; border-radius: 4px 4px 1px 1px; background: currentColor; opacity: .88; }
.product-history-chart li[data-state="ready"] { color: var(--run); }
.product-history-chart li[data-state="degraded"] { color: #f6b85f; }
.product-history-chart li[data-state="offline"] { color: var(--coral); }
.product-history-value { color: var(--text); font: 600 10px/1 "JetBrains Mono", monospace; font-variant-numeric: tabular-nums; }
.product-history-chart time { color: var(--text-faint); font: 500 9px/1 "JetBrains Mono", monospace; }
.product-history-empty { align-self: center; width: 100%; color: var(--text-faint); text-align: center; }
.product-history-legend { display: flex; flex-wrap: wrap; gap: 16px; margin-top: 14px; color: var(--text-soft); font-size: 11px; }
.product-history-legend span::before { width: 8px; height: 8px; display: inline-block; margin-right: 7px; border-radius: 2px; background: var(--text-faint); content: ""; }
.product-history-legend span[data-state="ready"]::before { background: var(--run); }
.product-history-legend span[data-state="degraded"]::before { background: #f6b85f; }
.product-history-legend span[data-state="offline"]::before { background: var(--coral); }
.product-visually-hidden { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }

@media (max-width: 880px) {
    .product-console__header { align-items: stretch; flex-direction: column; }
    .product-console__actions { justify-items: stretch; }
    .product-console__grid { grid-template-columns: 1fr; }
    .product-panel--surfaces { grid-row: auto; }
}

@media (max-width: 520px) {
    .product-console { padding: 18px; border-radius: 18px; }
    .product-panel { padding: 18px; }
    .product-metrics { grid-template-columns: repeat(2, 1fr); gap: 16px 0; }
    .product-metrics div:nth-child(3) { padding-left: 0; border-left: 0; }
    .product-panel__heading { align-items: stretch; flex-direction: column; }
    .product-summary, .product-badge { align-self: start; }
    .product-history-chart { gap: 3px; }
    .product-history-chart li:not(.product-history-empty) { grid-template-rows: 96px auto; }
    .product-history-chart time { display: none; }
}

@media (prefers-reduced-motion: reduce) {
    .product-console button { transition: none; }
}
"##),
    )?;
    let desktop_javascript = fs::read_to_string(desktop.join("resources/app.js"))
        .map_err(|error| format!("cannot read generated desktop JavaScript: {error}"))?;
    replace_generated(
        &desktop.join("resources/app.js"),
        &(desktop_javascript
            + r##"

(() => {
    "use strict";
    const button = document.querySelector("#product-refresh");
    const headline = document.querySelector("#product-headline");
    const versionCode = document.querySelector("#product-version-code");
    const surfaceCode = document.querySelector("#product-surface-code");
    const stateCode = document.querySelector("#product-state-code");
    const latency = document.querySelector("#product-latency");
    const status = document.querySelector("#product-status");
    const sampleTime = document.querySelector("#product-sample-time");
    const contractBadge = document.querySelector("#product-contract-badge");
    const summary = document.querySelector("#product-summary");
    const serverSurface = document.querySelector("#surface-server");
    const serverDetail = document.querySelector("#surface-server-detail");
    const desktopSurface = document.querySelector("#surface-desktop");
    const desktopDetail = document.querySelector("#surface-desktop-detail");
    const checkInButton = document.querySelector("#product-check-in");
    const outboxStatus = document.querySelector("#product-outbox-status");
    const outboxCount = document.querySelector("#product-outbox-count");
    const outboxMeter = document.querySelector("#product-outbox-meter");
    const historySummary = document.querySelector("#product-history-summary");
    const historyChart = document.querySelector("#product-history-chart");
    if (!window.pam || !button || !headline || !versionCode || !surfaceCode || !stateCode || !latency
        || !status || !sampleTime || !contractBadge || !summary || !serverSurface || !serverDetail
        || !desktopSurface || !desktopDetail || !checkInButton || !outboxStatus || !outboxCount
        || !outboxMeter || !historySummary || !historyChart) return;

    const themeQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const themeRoles = ["background", "surface", "surfaceRaised", "foreground", "mutedForeground",
        "border", "primary", "onPrimary", "success", "warning", "danger", "focus"];
    const cssRoles = {
        background: ["--ink", "--ink-soft"],
        surface: ["--surface"],
        surfaceRaised: ["--surface-raised"],
        foreground: ["--text"],
        mutedForeground: ["--text-soft", "--text-faint"],
        border: ["--line", "--line-strong"],
        primary: ["--run"],
        onPrimary: ["--run-ink"],
        success: ["--success"],
        warning: ["--warning"],
        danger: ["--coral"],
        focus: ["--cyan"],
    };
    let themeRevision = 0;
    const applyProductTheme = async () => {
        const revision = ++themeRevision;
        const modeCode = themeQuery.matches ? 2 : 1;
        const theme = await window.pam.invoke("product.theme", { modeCode }, { timeout: 2_000 });
        if (revision !== themeRevision) return;
        if (!theme || typeof theme !== "object" || Array.isArray(theme)
            || Object.keys(theme).join(",") !== "modeCode,name,colors"
            || theme.modeCode !== modeCode || theme.name !== (modeCode === 1 ? "light" : "dark")
            || !theme.colors || typeof theme.colors !== "object" || Array.isArray(theme.colors)
            || Object.keys(theme.colors).join(",") !== themeRoles.join(",")) {
            throw new Error("O contrato visual do Product é incompatível.");
        }
        for (const role of themeRoles) {
            const value = theme.colors[role];
            if (typeof value !== "string" || !/^#[0-9a-f]{6}$/.test(value)) {
                throw new Error("O contrato visual contém uma cor inválida.");
            }
            for (const property of cssRoles[role]) {
                document.documentElement.style.setProperty(property, value);
            }
        }
        document.documentElement.style.colorScheme = theme.name;
    };
    themeQuery.addEventListener("change", () => {
        void applyProductTheme().catch(() => {});
    });
    void applyProductTheme().catch(() => {});

    const stateName = (code) => ({ 1: "operacional", 2: "degradado", 3: "offline" })[code];
    const renderSummary = () => {
        const confirmed = [serverSurface, desktopSurface]
            .filter((surface) => surface.dataset.state === "ready").length;
        summary.textContent = `${confirmed} de 3 confirmadas`;
    };

    const renderOutbox = (result) => {
        if (!Number.isInteger(result.deliveryStateCode) || result.deliveryStateCode < 1 || result.deliveryStateCode > 2
            || !Number.isInteger(result.pendingCount) || result.pendingCount < 0 || result.pendingCount > 32) {
            throw new Error("O worker devolveu um estado de outbox incompatível.");
        }
        outboxCount.textContent = String(result.pendingCount);
        outboxMeter.value = result.pendingCount;
        outboxMeter.textContent = `${result.pendingCount} de 32 operações`;
        outboxStatus.textContent = result.deliveryStateCode === 1
            ? "Entregue · nenhuma operação pendente."
            : `Offline · ${result.pendingCount} operação(ões) preservada(s).`;
    };

    const renderHistory = (result) => {
        if (!result || typeof result !== "object" || Array.isArray(result)
            || Object.keys(result).length !== 2 || result.versionCode !== 1
            || !Array.isArray(result.samples) || result.samples.length > 24) {
            throw new Error("O histórico local é incompatível.");
        }
        let previousTimestamp = 0;
        for (const sample of result.samples) {
            if (!sample || typeof sample !== "object" || Array.isArray(sample)
                || Object.keys(sample).length !== 3
                || !Number.isSafeInteger(sample.observedAtUnixMs) || sample.observedAtUnixMs < previousTimestamp
                || !Number.isInteger(sample.latencyMs) || sample.latencyMs < 0 || sample.latencyMs > 30_000
                || !Number.isInteger(sample.stateCode) || sample.stateCode < 1 || sample.stateCode > 3) {
                throw new Error("Uma amostra do histórico local é incompatível.");
            }
            previousTimestamp = sample.observedAtUnixMs;
        }

        historyChart.replaceChildren();
        if (result.samples.length === 0) {
            const empty = document.createElement("li");
            empty.className = "product-history-empty";
            empty.textContent = "Nenhuma amostra disponível.";
            historyChart.append(empty);
            historySummary.textContent = "Faça uma consulta para iniciar o histórico local.";
            latency.textContent = "—";
            return;
        }

        const maximum = Math.max(1, ...result.samples.map((sample) => sample.latencyMs));
        const operational = result.samples.filter((sample) => sample.stateCode === 1).length;
        const orderedLatency = result.samples.map((sample) => sample.latencyMs).sort((left, right) => left - right);
        const median = orderedLatency[Math.floor((orderedLatency.length - 1) / 2)];
        const timeFormatter = new Intl.DateTimeFormat(undefined, {
            hour: "2-digit", minute: "2-digit"
        });
        for (const sample of result.samples) {
            const item = document.createElement("li");
            item.dataset.state = sample.stateCode === 1 ? "ready" : sample.stateCode === 2 ? "degraded" : "offline";
            const bar = document.createElement("span");
            bar.className = "product-history-bar";
            bar.setAttribute("aria-hidden", "true");
            bar.style.setProperty("--sample-height", `${Math.max(4, Math.round(sample.latencyMs / maximum * 100))}%`);
            const value = document.createElement("span");
            value.className = "product-history-value";
            value.textContent = `${sample.latencyMs} ms`;
            const time = document.createElement("time");
            const observedAt = new Date(sample.observedAtUnixMs);
            time.dateTime = observedAt.toISOString();
            time.textContent = timeFormatter.format(observedAt);
            const state = document.createElement("span");
            state.className = "product-visually-hidden";
            state.textContent = `Estado ${stateName(sample.stateCode)}`;
            item.append(bar, value, time, state);
            historyChart.append(item);
        }
        const availability = Math.round(operational / result.samples.length * 100);
        historySummary.textContent = `${availability}% operacional · mediana ${median} ms · ${result.samples.length} amostra(s)`;
        latency.textContent = String(result.samples[result.samples.length - 1].latencyMs);
    };

    const loadHistory = async () => {
        try {
            renderHistory(await window.pam.invoke("product.telemetry-history", null, { timeout: 2_000 }));
        } catch (error) {
            historySummary.textContent = error instanceof Error
                ? `Histórico indisponível: ${error.message}`
                : "Histórico local indisponível.";
        }
    };

    const refresh = async () => {
        button.disabled = true;
        button.setAttribute("aria-busy", "true");
        contractBadge.dataset.state = "checking";
        contractBadge.textContent = "verificando";
        serverSurface.dataset.state = "checking";
        desktopSurface.dataset.state = "checking";
        status.textContent = "Verificando no worker PHP…";
        try {
            const snapshot = await window.pam.invoke("product.server-status", null, { timeout: 7_000 });
            if (!Number.isInteger(snapshot.versionCode) || snapshot.versionCode !== 1
                || !Number.isInteger(snapshot.surfaceCode) || snapshot.surfaceCode !== 1
                || !Number.isInteger(snapshot.stateCode) || snapshot.stateCode < 1 || snapshot.stateCode > 3
                || typeof snapshot.headline !== "string" || snapshot.headline.length > 120) {
                throw new Error("O worker devolveu um contrato incompatível.");
            }
            headline.textContent = snapshot.headline;
            versionCode.textContent = String(snapshot.versionCode);
            surfaceCode.textContent = String(snapshot.surfaceCode);
            stateCode.textContent = String(snapshot.stateCode);
            sampleTime.textContent = `Amostra ${new Intl.DateTimeFormat(undefined, {
                hour: "2-digit", minute: "2-digit", second: "2-digit"
            }).format(new Date())}`;
            contractBadge.dataset.state = "ready";
            contractBadge.textContent = "compatível";
            desktopSurface.dataset.state = "ready";
            desktopDetail.textContent = "Worker PHP autenticado e responsivo";
            serverSurface.dataset.state = snapshot.stateCode === 1
                ? "ready" : snapshot.stateCode === 2 ? "degraded" : "offline";
            serverDetail.textContent = `Contrato compatível · ${stateName(snapshot.stateCode)}`;
            status.textContent = snapshot.stateCode === 1
                ? "Contrato operacional e compatível."
                : `Server ${stateName(snapshot.stateCode)}; consulte os diagnósticos do runtime.`;
        } catch (error) {
            latency.textContent = "—";
            sampleTime.textContent = "Última tentativa falhou";
            contractBadge.dataset.state = "error";
            contractBadge.textContent = "indisponível";
            desktopSurface.dataset.state = "ready";
            desktopDetail.textContent = "Worker PHP respondeu com erro recuperável";
            serverSurface.dataset.state = "offline";
            serverDetail.textContent = "Sem amostra válida do endpoint";
            status.textContent = error instanceof Error ? error.message : "Falha ao verificar o contrato.";
        } finally {
            await loadHistory();
            renderSummary();
            button.disabled = false;
            button.removeAttribute("aria-busy");
        }
    };
    button.addEventListener("click", () => void refresh());
    checkInButton.addEventListener("click", async () => {
        checkInButton.disabled = true;
        checkInButton.setAttribute("aria-busy", "true");
        outboxStatus.textContent = "Persistindo antes de transmitir…";
        try {
            renderOutbox(await window.pam.invoke("product.check-in", null, { timeout: 7_000 }));
        } catch (error) {
            outboxStatus.textContent = error instanceof Error ? error.message : "Falha no check-in.";
        } finally {
            checkInButton.disabled = false;
            checkInButton.removeAttribute("aria-busy");
        }
    });
    void refresh();
    void window.pam.invoke("product.outbox.replay", null, { timeout: 7_000 })
        .then(renderOutbox)
        .catch(() => { outboxStatus.textContent = "Offline · o worker preservará operações pendentes."; });
})();
"##),
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
- Desktop asks its authenticated PHP worker to fetch and validate the same
  Server endpoint; the renderer never receives ambient network authority.
- Desktop renders a responsive Product Control Center from verified data only:
  Server readiness, local worker availability, contract version, request
  latency, sample time, and bounded outbox occupancy. Native is explicitly
  marked as unmonitored until a trustworthy telemetry channel is connected.
- The PHP worker records at most 24 Server observations in a versioned 16 KiB
  local history using atomic replacement writes. It stores only integer state,
  latency, and observation time; the renderer receives the bounded history
  through an authenticated command and provides exact text alongside its chart.
- Desktop persists a 32-item, 64 KiB outbox with private temporary files and
  replacement writes under its capability-scoped `storage` root. The worker
  replays it on launch; the renderer receives only delivery codes and counts.
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

fn replace_generated_once(
    path: &Path,
    needle: &str,
    replacement: &str,
    label: &str,
) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read generated {label} {}: {error}", path.display()))?;
    if source.matches(needle).count() != 1 {
        return Err(format!(
            "generated {label} marker must occur exactly once in {}",
            path.display()
        ));
    }
    replace_generated(path, &source.replacen(needle, replacement, 1))
}

fn write_desktop_inspector(directory: &Path) -> Result<(), String> {
    write_new(
        &directory.join("resources/inspector.html"),
        r##"<!doctype html>
<html lang="pt-BR">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <meta name="color-scheme" content="dark">
    <meta name="theme-color" content="#071018">
    <title>Pam Desktop · Runtime Inspector</title>
    <link rel="stylesheet" href="/inspector.css">
    <script src="/_pam/bridge.js" defer></script>
    <script src="/inspector.js" defer></script>
</head>
<body>
    <main>
        <header>
            <div class="identity">
                <svg viewBox="0 0 32 32" aria-hidden="true">
                    <path d="M7 23V9h8.1c4.2 0 6.9 2.2 6.9 5.8 0 3.7-2.7 5.9-6.9 5.9h-3.4V23H7Z"/>
                    <path class="spark" d="m23.8 7.4.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8.8-2.2Z"/>
                </svg>
                <div>
                    <span>janela secundária</span>
                    <h1>Runtime Inspector</h1>
                </div>
            </div>
            <button id="hide-button" type="button" aria-label="Ocultar Runtime Inspector">
                <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="m7 7 10 10M17 7 7 17"/>
                </svg>
            </button>
        </header>

        <section class="summary" aria-labelledby="summary-title">
            <div>
                <span class="eyebrow">PAM DESKTOP 0.5</span>
                <h2 id="summary-title">Uma runtime.<br><strong>Múltiplas janelas.</strong></h2>
            </div>
            <span class="online"><i aria-hidden="true"></i> worker online</span>
        </section>

        <section class="metrics" aria-label="Estado da runtime">
            <article>
                <span>window id</span>
                <strong id="window-id">—</strong>
                <small>isolamento por contexto</small>
            </article>
            <article>
                <span>protocol</span>
                <strong>IPC v5</strong>
                <small>contrato tipado</small>
            </article>
            <article>
                <span>renderer</span>
                <strong>Servo</strong>
                <small>host Rust nativo</small>
            </article>
        </section>

        <section class="event-log" aria-labelledby="events-title">
            <div class="section-heading">
                <div>
                    <span>STREAM</span>
                    <h2 id="events-title">Eventos da aplicação</h2>
                </div>
                <span class="live"><i aria-hidden="true"></i> live</span>
            </div>
            <ol id="event-list" aria-live="polite">
                <li>
                    <time>agora</time>
                    <span>inspector.ready</span>
                    <small>aguardando eventos do PHP</small>
                </li>
            </ol>
        </section>
    </main>
</body>
</html>
"##,
    )?;
    write_new(
        &directory.join("resources/inspector.css"),
        r#":root {
    --ink: #071018;
    --surface: #0d1b27;
    --surface-raised: #132737;
    --text: #f3f7f8;
    --text-soft: #9fb3be;
    --text-faint: #718792;
    --violet: #a69aff;
    --cyan: #68ded2;
    --coral: #ff9279;
    --line: rgba(176, 209, 220, 0.14);
    --line-strong: rgba(176, 209, 220, 0.26);
    color: var(--text);
    background: var(--ink);
    font-family: "IBM Plex Sans", "Segoe UI", system-ui, sans-serif;
}

* {
    box-sizing: border-box;
}

html {
    min-width: 320px;
    min-height: 100%;
    background: var(--ink);
}

body {
    min-height: 100vh;
    margin: 0;
    background:
        radial-gradient(circle at 78% 10%, rgba(166, 154, 255, 0.13), transparent 24rem),
        linear-gradient(rgba(104, 222, 210, 0.025) 1px, transparent 1px),
        linear-gradient(90deg, rgba(104, 222, 210, 0.025) 1px, transparent 1px),
        var(--ink);
    background-size: auto, 36px 36px, 36px 36px, auto;
}

button {
    font: inherit;
    -webkit-tap-highlight-color: transparent;
}

:focus-visible {
    outline: 3px solid var(--cyan);
    outline-offset: 4px;
}

main {
    width: min(100%, 760px);
    min-height: 100vh;
    margin: 0 auto;
    padding: 0 clamp(20px, 5vw, 48px) 40px;
}

header {
    min-height: 82px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--line);
}

.identity {
    display: flex;
    align-items: center;
    gap: 12px;
}

.identity svg {
    width: 32px;
    fill: none;
    stroke: var(--text);
    stroke-width: 2;
    stroke-linejoin: round;
}

.identity .spark {
    stroke: var(--cyan);
    stroke-width: 1.5;
}

.identity span,
.eyebrow,
.section-heading span,
.metrics span {
    color: var(--text-faint);
    font: 600 10px/1.3 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
    letter-spacing: 0.12em;
    text-transform: uppercase;
}

.identity h1 {
    margin: 3px 0 0;
    font-size: 16px;
    letter-spacing: -0.02em;
}

header button {
    width: 44px;
    height: 44px;
    display: grid;
    place-items: center;
    color: var(--text-soft);
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(13, 27, 39, 0.7);
    cursor: pointer;
    transition: color 180ms ease, border-color 180ms ease, background 180ms ease;
}

header button:hover {
    color: var(--text);
    border-color: var(--line-strong);
    background: var(--surface-raised);
}

header button svg {
    width: 19px;
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-width: 1.8;
}

.summary {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 24px;
    padding: 42px 0 30px;
}

.summary h2 {
    margin: 10px 0 0;
    font-size: clamp(34px, 7vw, 54px);
    font-weight: 650;
    line-height: 0.98;
    letter-spacing: -0.055em;
}

.summary h2 strong {
    color: var(--violet);
    font-weight: inherit;
}

.online,
.live {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    white-space: nowrap;
}

.online {
    min-height: 38px;
    padding: 0 12px;
    color: var(--cyan);
    border: 1px solid rgba(104, 222, 210, 0.2);
    border-radius: 999px;
    background: rgba(104, 222, 210, 0.06);
    font: 600 10px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
}

.online i,
.live i {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--cyan);
    box-shadow: 0 0 12px rgba(104, 222, 210, 0.8);
}

.metrics {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: rgba(13, 27, 39, 0.74);
}

.metrics article {
    min-width: 0;
    padding: 20px;
}

.metrics article + article {
    border-left: 1px solid var(--line);
}

.metrics strong,
.metrics small {
    display: block;
}

.metrics strong {
    margin-top: 11px;
    overflow: hidden;
    font-size: 18px;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.metrics small {
    margin-top: 5px;
    color: var(--text-faint);
    font-size: 11px;
}

.event-log {
    margin-top: 20px;
    padding: 22px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: rgba(13, 27, 39, 0.58);
}

.section-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
}

.section-heading h2 {
    margin: 5px 0 0;
    font-size: 17px;
}

.section-heading .live {
    color: var(--cyan);
}

ol {
    margin: 20px 0 0;
    padding: 0;
    list-style: none;
}

li {
    display: grid;
    grid-template-columns: 54px minmax(0, 1fr);
    gap: 4px 12px;
    padding: 13px 0;
    border-top: 1px solid var(--line);
}

li time {
    grid-row: span 2;
    color: var(--text-faint);
    font: 500 10px/1.5 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
}

li span {
    color: var(--text);
    font: 600 12px/1.3 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
}

li small {
    overflow: hidden;
    color: var(--text-faint);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
}

@media (max-width: 560px) {
    .summary {
        align-items: flex-start;
        flex-direction: column;
    }

    .metrics {
        grid-template-columns: 1fr;
    }

    .metrics article + article {
        border-top: 1px solid var(--line);
        border-left: 0;
    }
}

@media (prefers-reduced-motion: reduce) {
    *,
    *::before,
    *::after {
        animation-duration: 0.01ms !important;
        animation-iteration-count: 1 !important;
        transition-duration: 0.01ms !important;
    }
}
"#,
    )?;
    write_new(
        &directory.join("resources/inspector.js"),
        r##"(() => {
    "use strict";

    const list = document.querySelector("#event-list");
    const hideButton = document.querySelector("#hide-button");
    const windowId = document.querySelector("#window-id");

    const appendEvent = (name, detail) => {
        const item = document.createElement("li");
        const time = document.createElement("time");
        const title = document.createElement("span");
        const description = document.createElement("small");

        time.textContent = new Intl.DateTimeFormat("pt-BR", {
            hour: "2-digit",
            minute: "2-digit",
            second: "2-digit",
        }).format(new Date());
        title.textContent = name;
        description.textContent = detail;
        item.append(time, title, description);
        list.prepend(item);

        while (list.children.length > 5) {
            list.lastElementChild.remove();
        }
    };

    if (!window.pam) {
        appendEvent("bridge.error", "A bridge Pam não foi carregada.");
        hideButton.disabled = true;
        return;
    }

    windowId.textContent = window.pam.windowId;
    window.pam.on("runtime.ready", ({ protocol }) => {
        appendEvent("runtime.ready", `IPC v${protocol} conectado`);
    });
    window.pam.on("pam.dev.reloaded", ({ kind }) => {
        appendEvent("pam.dev.reloaded", kind === 1 ? "assets" : "worker PHP");
    });
    window.pam.on("pam.dev.error", ({ message }) => {
        appendEvent("pam.dev.error", message);
    });

    hideButton.addEventListener("click", async () => {
        hideButton.disabled = true;
        try {
            await window.pam.invoke("inspector.hide", null, { timeout: 3_000 });
        } catch (error) {
            appendEvent(
                "inspector.hide.failed",
                error instanceof Error ? error.message : "Falha desconhecida",
            );
            hideButton.disabled = false;
        }
    });

    void window.pam.emit("client.ready", {
        loadedAt: new Date().toISOString(),
    }, { timeout: 2_000 }).catch((error) => {
        appendEvent(
            "client.ready.failed",
            error instanceof Error ? error.message : "Falha desconhecida",
        );
    });
})();
"##,
    )?;
    Ok(())
}

fn init_desktop(directory: &Path) -> Result<(), String> {
    let mut manifest = serde_json::json!({
        "name": "app/pam-desktop-project",
        "description": "A PHP-first desktop application powered by Pam, Rust, and Servo.",
        "type": "project",
        "license": "proprietary",
        "require": {
            "php": "^8.4",
            "pushinbr/pam-desktop": "^1.2"
        },
        "autoload": {
            "psr-4": {
                "App\\": "src/"
            }
        },
        "config": {
            "platform-check": true,
            "sort-packages": true
        },
        "scripts": {
            "desktop:build": "pam desktop build .",
            "desktop:dev": "pam desktop dev .",
            "desktop:doctor": "pam desktop doctor ."
        }
    });
    if let Some(repository) = local_desktop_repository() {
        manifest["repositories"] = serde_json::json!([repository]);
    }
    write_new(
        &directory.join("composer.json"),
        &(serde_json::to_string_pretty(&manifest)
            .map_err(|error| format!("cannot serialize Composer manifest: {error}"))?
            + "\n"),
    )?;
    write_new(
        &directory.join("app.php"),
        r#"<?php

declare(strict_types=1);

use Pam\Desktop\App;
use Pam\Desktop\ApplicationCategory;
use Pam\Desktop\Attributes\Command;
use Pam\Desktop\Attributes\Desktop as DesktopApplication;
use Pam\Desktop\Attributes\Listen;
use Pam\Desktop\Attributes\Window as DesktopWindowDefinition;
use Pam\Desktop\Desktop;
use Pam\Desktop\DesktopWindow;
use Pam\Desktop\Events;
use Pam\Desktop\Permissions;
use Pam\Desktop\WindowHandle;
use Pam\Desktop\WindowTheme;

require __DIR__.'/vendor/autoload.php';

#[DesktopWindowDefinition(
    name: 'inspector',
    title: 'Pam Desktop · Runtime Inspector',
    page: 'resources/inspector.html',
    width: 680,
    height: 520,
    minimumWidth: 480,
    minimumHeight: 360,
    theme: WindowTheme::Dark,
)]
final readonly class InspectorWindow extends DesktopWindow
{
}

#[DesktopApplication(
    id: 'com.pushin.pam-hello',
    name: 'Pam Hello',
    version: '1.0.0',
    description: 'Uma aplicação desktop elegante, gerenciada em PHP.',
    publisher: 'Pushin',
    category: ApplicationCategory::Development,
    theme: WindowTheme::Dark,
)]
final class HelloApp extends App
{
    protected function configure(Desktop $desktop): void
    {
        $desktop
            ->permissions(static fn (Permissions $permissions) => $permissions
                ->filesystem('data', __DIR__.'/storage', read: true, write: true)
                ->dialogs()
                ->clipboard()
                ->notifications()
                ->dragAndDrop())
            ->timeout(10_000);
    }

    protected function windows(): array
    {
        return [InspectorWindow::class];
    }

    #[Command]
    public function greet(
        WindowHandle $window,
        Events $events,
        string $name = 'mundo',
    ): array {
        $name = trim($name);
        $name = $name !== '' ? mb_substr($name, 0, 40) : 'mundo';
        $window->title("Pam Desktop · {$name}");
        $events->emit('hello.completed', compact('name'));

        return [
            'message' => "Olá, {$name}.",
            'detail' => 'Esta resposta saiu do PHP, atravessou o host Rust e chegou ao Servo.',
        ];
    }

    #[Command('inspector.open')]
    public function openInspector(
        InspectorWindow $inspector,
        WindowHandle $window,
        Events $events,
    ): array {
        $inspector->show()->focus();
        $events->emit('inspector.opened', ['sourceWindowId' => $window->id]);

        return ['windowId' => $inspector->id()];
    }

    #[Command('inspector.hide')]
    public function hideInspector(InspectorWindow $inspector): void
    {
        $inspector->hide();
    }

    #[Listen('client.ready')]
    public function clientReady(WindowHandle $window, Events $events): void
    {
        $events->emit('runtime.ready', [
            'windowId' => $window->id,
            'protocol' => 6,
        ]);
    }
}

HelloApp::run();
"#,
    )?;
    write_new(
        &directory.join(".gitignore"),
        "/vendor/\n/.pam/\n/target/\n/storage/*\n!/storage/.gitkeep\n",
    )?;
    fs::create_dir_all(directory.join("resources"))
        .map_err(|error| format!("cannot create desktop resources: {error}"))?;
    fs::create_dir_all(directory.join("storage"))
        .map_err(|error| format!("cannot create desktop storage: {error}"))?;
    write_new(&directory.join("storage/.gitkeep"), "")?;
    write_new(
        &directory.join("resources/icon.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
    <title>Pam Hello</title>
    <defs>
        <linearGradient id="background" x1="64" y1="40" x2="448" y2="472" gradientUnits="userSpaceOnUse">
            <stop stop-color="#16364a"/>
            <stop offset="0.52" stop-color="#0b1d2a"/>
            <stop offset="1" stop-color="#071018"/>
        </linearGradient>
        <linearGradient id="signal" x1="140" y1="118" x2="378" y2="394" gradientUnits="userSpaceOnUse">
            <stop stop-color="#9ff5eb"/>
            <stop offset="1" stop-color="#43b8c5"/>
        </linearGradient>
    </defs>
    <rect x="20" y="20" width="472" height="472" rx="116" fill="url(#background)"/>
    <rect x="42" y="42" width="428" height="428" rx="96" fill="none" stroke="#68ded2" stroke-opacity=".22" stroke-width="4"/>
    <path d="M152 386V126h120c72 0 118 40 118 102 0 64-48 106-122 106h-48v52h-68Zm68-112h47c34 0 54-16 54-44 0-27-20-43-54-43h-47v87Z" fill="url(#signal)"/>
    <circle cx="389" cy="128" r="18" fill="#f6b85f"/>
    <path d="M389 160v50M357 128h-42M421 128h28" stroke="#f6b85f" stroke-linecap="round" stroke-width="10"/>
</svg>
"##,
    )?;
    write_new(
        &directory.join("resources/index.html"),
        r##"<!doctype html>
<html lang="pt-BR">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <meta name="color-scheme" content="dark">
    <meta name="theme-color" content="#071018">
    <title>Pam Desktop · Hello</title>
    <link rel="stylesheet" href="/styles.css">
    <script src="/_pam/bridge.js" defer></script>
    <script src="/app.js" defer></script>
</head>
<body>
    <a class="skip-link" href="#main-content">Pular para o conteúdo</a>

    <div class="app-shell">
        <header class="topbar" aria-label="Barra da aplicação">
            <a class="brand" href="/" aria-label="Pam Desktop, início">
                <svg class="brand-mark" viewBox="0 0 32 32" aria-hidden="true">
                    <path d="M7 23V9h8.1c4.2 0 6.9 2.2 6.9 5.8 0 3.7-2.7 5.9-6.9 5.9h-3.4V23H7Z"/>
                    <path class="brand-spark" d="m23.8 7.4.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8.8-2.2Z"/>
                </svg>
                <span>Pam <strong>Desktop</strong></span>
            </a>

            <div class="runtime-status" aria-label="Estado da runtime">
                <span class="status-pulse" aria-hidden="true"></span>
                <span id="runtime-status-text">runtime conectando</span>
                <kbd>API 1</kbd>
            </div>
        </header>

        <main id="main-content">
            <section class="hero" aria-labelledby="hero-title">
                <div class="hero-copy">
                    <p class="eyebrow">
                        <span>PHP</span>
                        <svg viewBox="0 0 20 10" aria-hidden="true">
                            <path d="M1 5h16M13 1l4 4-4 4"/>
                        </svg>
                        <span>Rust</span>
                        <svg viewBox="0 0 20 10" aria-hidden="true">
                            <path d="M1 5h16M13 1l4 4-4 4"/>
                        </svg>
                        <span>Servo</span>
                    </p>

                    <h1 id="hero-title">
                        PHP na direção.<br>
                        <span>Rust no ritmo.</span><br>
                        Servo na tela.
                    </h1>

                    <p class="hero-description">
                        Uma aplicação desktop de verdade, com sua lógica em PHP,
                        isolamento por processo e uma engine web inteira escrita para o futuro.
                    </p>

                    <form class="hello-form" id="hello-form">
                        <div class="field">
                            <label for="name">Como devemos te chamar?</label>
                            <div class="input-wrap">
                                <svg viewBox="0 0 24 24" aria-hidden="true">
                                    <circle cx="12" cy="8" r="4"/>
                                    <path d="M4.5 20c.8-4 3.3-6 7.5-6s6.7 2 7.5 6"/>
                                </svg>
                                <input
                                    id="name"
                                    name="name"
                                    maxlength="40"
                                    autocomplete="name"
                                    aria-describedby="name-hint"
                                    placeholder="Seu nome"
                                    required
                                >
                            </div>
                            <small id="name-hint" class="field-hint">Até 40 caracteres; enviado somente ao comando local.</small>
                        </div>
                        <button id="hello-button" type="submit">
                            <span>Conversar com o PHP</span>
                            <svg viewBox="0 0 24 24" aria-hidden="true">
                                <path d="M5 12h13M14 7l5 5-5 5"/>
                            </svg>
                        </button>
                    </form>

                    <div class="demo-actions">
                        <button id="inspector-button" type="button">
                            <svg viewBox="0 0 24 24" aria-hidden="true">
                                <rect x="4" y="5" width="16" height="14" rx="2"/>
                                <path d="M8 9h8M8 13h5"/>
                            </svg>
                            <span>Abrir Runtime Inspector</span>
                        </button>
                        <span id="event-status" role="status" aria-live="polite">
                            eventos conectando…
                        </span>
                    </div>

                    <section class="native-lab" aria-labelledby="native-lab-title">
                        <div class="native-lab-heading">
                            <div>
                                <span>NATIVE AUTHORITY · API 1</span>
                                <h2 id="native-lab-title">Native Lab</h2>
                            </div>
                            <span class="capability-lock">autorizado no PHP</span>
                        </div>
                        <div class="native-actions" aria-label="Demonstrações nativas">
                            <button id="save-note-button" type="button">Salvar nota</button>
                            <button id="open-file-button" type="button">Abrir texto</button>
                            <button id="copy-button" type="button">Copiar olá</button>
                            <button id="notify-button" type="button">Notificar</button>
                        </div>
                        <div id="drop-zone" class="drop-zone" aria-label="Área para soltar arquivos">
                            <strong>Arraste um arquivo para a janela</strong>
                            <span>O host entrega um grant temporário, nunca o caminho do sistema.</span>
                        </div>
                        <p id="native-status" role="status" aria-live="polite">
                            Nenhuma capability usada ainda.
                        </p>
                    </section>

                    <section class="update-console" aria-labelledby="update-title">
                        <div>
                            <span>SIGNED UPDATES · API 1</span>
                            <h2 id="update-title">Atualizações com rollback</h2>
                            <p id="update-status" role="status" aria-live="polite">
                                Desativadas por padrão; a chave pública fica no manifesto PHP.
                            </p>
                        </div>
                        <button id="update-button" type="button">Verificar estado</button>
                    </section>

                    <div class="response" id="response" role="status" aria-live="polite" aria-atomic="true" tabindex="-1">
                        <span class="response-label">aguardando comando</span>
                        <p id="response-message">
                            A primeira resposta da sua aplicação vai aparecer aqui.
                        </p>
                        <small id="response-detail">
                            Nenhuma ponte global para Node. Apenas comandos explícitos.
                        </small>
                    </div>
                </div>

                <div class="runtime-visual" aria-label="Fluxo entre Servo, Rust e PHP">
                    <div class="orbit orbit-outer" aria-hidden="true"></div>
                    <div class="orbit orbit-middle" aria-hidden="true"></div>
                    <div class="orbit orbit-inner" aria-hidden="true"></div>

                    <div class="core">
                        <svg viewBox="0 0 32 32" aria-hidden="true">
                            <path d="M8 24V8h8.6c4.8 0 7.7 2.5 7.7 6.6 0 4.2-2.9 6.7-7.7 6.7h-3.7V24H8Z"/>
                        </svg>
                        <span>PAM</span>
                    </div>

                    <div class="runtime-node node-servo">
                        <span class="node-index">01</span>
                        <strong>Servo</strong>
                        <small>render</small>
                    </div>
                    <div class="runtime-node node-rust">
                        <span class="node-index">02</span>
                        <strong>Rust</strong>
                        <small>host</small>
                    </div>
                    <div class="runtime-node node-php">
                        <span class="node-index">03</span>
                        <strong>PHP</strong>
                        <small>logic</small>
                    </div>

                    <div class="signal signal-a" aria-hidden="true"></div>
                    <div class="signal signal-b" aria-hidden="true"></div>

                    <aside class="security-note">
                        <svg viewBox="0 0 24 24" aria-hidden="true">
                            <path d="M12 3 5 6v5c0 4.6 2.8 8.1 7 10 4.2-1.9 7-5.4 7-10V6l-7-3Z"/>
                            <path d="m9 12 2 2 4-4"/>
                        </svg>
                        <div>
                            <strong>Bridge protegido</strong>
                            <span>origin + token efêmero</span>
                        </div>
                    </aside>
                </div>
            </section>

            <section class="runtime-strip" aria-label="Componentes da aplicação">
                <article>
                    <span>renderer</span>
                    <strong>Servo LTS</strong>
                    <small>HTML, CSS e JavaScript</small>
                </article>
                <article>
                    <span>orchestrator</span>
                    <strong>Rust host</strong>
                    <small>janela, IPC e segurança</small>
                </article>
                <article>
                    <span>application</span>
                    <strong>PHP 8.4</strong>
                    <small>Composer e domínio</small>
                </article>
                <article>
                    <span>contract</span>
                    <strong>IPC v6</strong>
                    <small>tipado e versionado</small>
                </article>
            </section>
        </main>

        <footer>
            <span>Feito com Pam Desktop</span>
            <span class="footer-command"><kbd>pam desktop dev .</kbd></span>
        </footer>
    </div>
</body>
</html>
"##,
    )?;
    write_new(
        &directory.join("resources/styles.css"),
        r#":root {
    --ink: #071018;
    --ink-soft: #0a151f;
    --surface: #0d1b27;
    --surface-raised: #132737;
    --text: #f3f7f8;
    --text-soft: #9fb3be;
    --text-faint: #718792;
    --violet: #a69aff;
    --cyan: #68ded2;
    --run: #67e8a5;
    --run-ink: #062315;
    --coral: #ff9279;
    --line: rgba(176, 209, 220, 0.14);
    --line-strong: rgba(176, 209, 220, 0.26);
    --shadow: 0 24px 70px rgba(0, 0, 0, 0.34);
    color: var(--text);
    background: var(--ink);
    font-family: "IBM Plex Sans", "Segoe UI", system-ui, sans-serif;
    font-synthesis: none;
}

* {
    box-sizing: border-box;
}

html {
    min-width: 320px;
    min-height: 100%;
    background: var(--ink);
}

body {
    min-height: 100vh;
    min-height: 100dvh;
    margin: 0;
    overflow-x: hidden;
    background:
        radial-gradient(circle at 77% 39%, rgba(166, 154, 255, 0.11), transparent 28rem),
        linear-gradient(rgba(104, 222, 210, 0.025) 1px, transparent 1px),
        linear-gradient(90deg, rgba(104, 222, 210, 0.025) 1px, transparent 1px),
        var(--ink);
    background-size: auto, 42px 42px, 42px 42px, auto;
}

button,
input {
    font: inherit;
}

button,
a {
    -webkit-tap-highlight-color: transparent;
}

.skip-link {
    position: fixed;
    z-index: 100;
    top: 12px;
    left: 12px;
    padding: 10px 14px;
    color: var(--ink);
    background: var(--run);
    border-radius: 8px;
    transform: translateY(-160%);
}

.skip-link:focus {
    transform: translateY(0);
}

:focus-visible {
    outline: 3px solid var(--run);
    outline-offset: 4px;
}

.app-shell {
    width: min(100%, 1440px);
    min-height: 100vh;
    margin: 0 auto;
    padding: 0 clamp(24px, 4vw, 64px);
}

.topbar {
    height: 84px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--line);
}

.brand {
    display: inline-flex;
    align-items: center;
    gap: 11px;
    color: var(--text);
    text-decoration: none;
    letter-spacing: -0.02em;
    font-size: 17px;
}

.brand strong {
    font-weight: 650;
}

.brand-mark {
    width: 30px;
    height: 30px;
    overflow: visible;
    fill: none;
    stroke: var(--text);
    stroke-width: 2.1;
    stroke-linejoin: round;
}

.brand-mark .brand-spark {
    stroke: var(--cyan);
    stroke-width: 1.45;
}

.runtime-status {
    min-height: 38px;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 0 8px 0 13px;
    color: var(--text-soft);
    border: 1px solid var(--line);
    border-radius: 999px;
    background: rgba(13, 27, 39, 0.72);
    font: 500 11px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
    letter-spacing: 0.04em;
}

.runtime-status kbd {
    padding: 7px 9px;
    color: var(--cyan);
    border: 1px solid rgba(104, 222, 210, 0.19);
    border-radius: 999px;
    background: rgba(104, 222, 210, 0.07);
    font: inherit;
}

.status-pulse {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--cyan);
    box-shadow: 0 0 0 4px rgba(104, 222, 210, 0.1), 0 0 18px rgba(104, 222, 210, 0.8);
}

.runtime-status[data-state="ready"] .status-pulse {
    background: var(--run);
    box-shadow: 0 0 0 4px rgba(103, 232, 165, 0.1), 0 0 18px rgba(103, 232, 165, 0.72);
}

.runtime-status[data-state="error"] .status-pulse {
    background: var(--coral);
    box-shadow: 0 0 0 4px rgba(255, 146, 121, 0.12);
}

.hero {
    min-height: calc(100vh - 208px);
    display: grid;
    grid-template-columns: minmax(0, 1.08fr) minmax(420px, 0.92fr);
    gap: clamp(48px, 7vw, 110px);
    align-items: center;
    padding: clamp(52px, 7vh, 94px) 0;
}

.hero-copy {
    position: relative;
    z-index: 3;
}

.eyebrow {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 0 24px;
    color: var(--cyan);
    font: 600 11px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
    letter-spacing: 0.15em;
    text-transform: uppercase;
}

.eyebrow svg {
    width: 20px;
    fill: none;
    stroke: var(--text-faint);
    stroke-width: 1.2;
}

h1 {
    max-width: 720px;
    margin: 0;
    color: var(--text);
    font-size: clamp(46px, 6.1vw, 84px);
    font-weight: 630;
    line-height: 0.94;
    letter-spacing: -0.065em;
}

h1 span {
    color: transparent;
    background: linear-gradient(105deg, var(--run) 8%, #baf7d4 52%, var(--cyan));
    background-clip: text;
}

.hero-description {
    max-width: 620px;
    margin: 30px 0 0;
    color: var(--text-soft);
    font-size: 17px;
    line-height: 1.65;
}

.hello-form {
    max-width: 620px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    align-items: end;
    margin-top: 34px;
}

.field label {
    display: block;
    margin: 0 0 9px 2px;
    color: var(--text-soft);
    font-size: 12px;
    font-weight: 600;
}

.field-hint {
    display: block;
    margin: 8px 2px 0;
    color: var(--text-faint);
    font-size: 11px;
    line-height: 1.45;
}

.input-wrap {
    height: 54px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 16px;
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    background: rgba(13, 27, 39, 0.8);
    transition: border-color 180ms ease, background 180ms ease, box-shadow 180ms ease;
}

.input-wrap:focus-within {
    border-color: var(--violet);
    background: var(--surface);
    box-shadow: 0 0 0 4px rgba(166, 154, 255, 0.1);
}

.input-wrap svg {
    width: 19px;
    flex: 0 0 auto;
    fill: none;
    stroke: var(--text-faint);
    stroke-width: 1.7;
    stroke-linecap: round;
}

.input-wrap input {
    min-width: 0;
    width: 100%;
    color: var(--text);
    border: 0;
    outline: 0;
    background: transparent;
    font-size: 15px;
}

.input-wrap input::placeholder {
    color: var(--text-faint);
}

.hello-form button {
    min-height: 54px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 0 20px;
    color: var(--run-ink);
    border: 0;
    border-radius: 12px;
    background: linear-gradient(120deg, var(--run), #8ef0c0);
    box-shadow: 0 12px 28px rgba(45, 190, 116, 0.2);
    cursor: pointer;
    font-size: 14px;
    font-weight: 700;
    transition: filter 180ms ease, transform 180ms ease, box-shadow 180ms ease;
}

.hello-form button:hover {
    filter: brightness(1.08);
    box-shadow: 0 16px 34px rgba(45, 190, 116, 0.3);
    transform: translateY(-1px);
}

.hello-form button:active {
    transform: translateY(0);
}

.hello-form button:disabled {
    cursor: wait;
    filter: saturate(0.5);
    opacity: 0.72;
    transform: none;
}

.hello-form button svg {
    width: 19px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
}

.demo-actions {
    max-width: 620px;
    min-height: 48px;
    display: flex;
    align-items: center;
    gap: 14px;
    margin-top: 14px;
}

.demo-actions button {
    min-height: 44px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 9px;
    padding: 0 15px;
    color: var(--text);
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: rgba(19, 39, 55, 0.72);
    cursor: pointer;
    font-size: 12px;
    font-weight: 650;
    transition: border-color 180ms ease, background 180ms ease, transform 180ms ease;
}

.demo-actions button:hover {
    border-color: rgba(104, 222, 210, 0.56);
    background: var(--surface-raised);
    transform: translateY(-1px);
}

.demo-actions button:active {
    transform: translateY(0);
}

.demo-actions button:disabled {
    cursor: wait;
    opacity: 0.6;
    transform: none;
}

.demo-actions button svg {
    width: 18px;
    fill: none;
    stroke: var(--cyan);
    stroke-width: 1.7;
    stroke-linecap: round;
    stroke-linejoin: round;
}

.demo-actions > span {
    color: var(--text-faint);
    font: 500 10px/1.4 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
}

.native-lab {
    max-width: 620px;
    margin-top: 18px;
    padding: 18px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: rgba(13, 27, 39, 0.64);
}

.native-lab-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
}

.native-lab-heading span {
    color: var(--cyan);
    font: 600 9px/1.3 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
    letter-spacing: 0.12em;
    text-transform: uppercase;
}

.native-lab-heading h2 {
    margin: 5px 0 0;
    font-size: 18px;
}

.native-lab-heading .capability-lock {
    min-height: 30px;
    display: inline-flex;
    align-items: center;
    padding: 0 10px;
    color: var(--text-soft);
    border: 1px solid var(--line);
    border-radius: 999px;
    letter-spacing: 0.04em;
    text-transform: none;
}

.native-actions {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
    margin-top: 16px;
}

.native-actions button {
    min-height: 44px;
    padding: 0 10px;
    color: var(--text);
    border: 1px solid var(--line-strong);
    border-radius: 9px;
    background: var(--surface-raised);
    cursor: pointer;
    font-size: 11px;
    font-weight: 650;
    transition: border-color 180ms ease, background 180ms ease, transform 180ms ease;
}

.native-actions button:hover {
    border-color: var(--cyan);
    background: rgba(104, 222, 210, 0.09);
    transform: translateY(-1px);
}

.native-actions button:disabled {
    cursor: wait;
    opacity: 0.55;
    transform: none;
}

.drop-zone {
    min-height: 76px;
    display: flex;
    align-items: center;
    justify-content: center;
    flex-direction: column;
    gap: 5px;
    margin-top: 10px;
    padding: 14px;
    text-align: center;
    border: 1px dashed var(--line-strong);
    border-radius: 10px;
    transition: border-color 180ms ease, background 180ms ease;
}

.drop-zone strong {
    font-size: 12px;
}

.drop-zone span,
#native-status {
    color: var(--text-faint);
    font-size: 10px;
    line-height: 1.45;
}

.drop-zone[data-active="true"] {
    border-color: var(--cyan);
    background: rgba(104, 222, 210, 0.07);
}

#native-status {
    min-height: 15px;
    margin: 10px 2px 0;
}

.update-console {
    max-width: 620px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    margin-top: 12px;
    padding: 15px 16px;
    border: 1px solid rgba(166, 154, 255, 0.24);
    border-radius: 12px;
    background: linear-gradient(115deg, rgba(166, 154, 255, 0.09), rgba(104, 222, 210, 0.04));
}

.update-console span {
    color: var(--violet);
    font: 650 9px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
    letter-spacing: 0.12em;
}

.update-console h2 {
    margin: 6px 0 3px;
    font-size: 14px;
}

.update-console p {
    margin: 0;
    color: var(--text-faint);
    font-size: 10px;
    line-height: 1.45;
}

.update-console button {
    min-width: 118px;
    min-height: 40px;
    color: var(--text);
    border: 1px solid rgba(166, 154, 255, 0.34);
    border-radius: 9px;
    background: rgba(166, 154, 255, 0.1);
    cursor: pointer;
    font-size: 10px;
    font-weight: 700;
}

.update-console button:disabled {
    cursor: wait;
    opacity: 0.55;
}

.response {
    max-width: 620px;
    min-height: 112px;
    margin-top: 16px;
    padding: 18px 20px;
    border: 1px solid var(--line);
    border-left: 2px solid var(--violet);
    border-radius: 4px 12px 12px 4px;
    background: rgba(13, 27, 39, 0.58);
    box-shadow: inset 0 1px rgba(255, 255, 255, 0.02);
}

.response-label {
    display: block;
    margin-bottom: 8px;
    color: var(--violet);
    font: 600 10px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
    letter-spacing: 0.13em;
    text-transform: uppercase;
}

.response p {
    margin: 0;
    color: var(--text);
    font-size: 15px;
    font-weight: 600;
    line-height: 1.5;
}

.response small {
    display: block;
    margin-top: 5px;
    color: var(--text-faint);
    font-size: 12px;
    line-height: 1.5;
}

.response[data-state="success"] {
    border-left-color: var(--cyan);
}

.response[data-state="success"] .response-label {
    color: var(--cyan);
}

.response[data-state="error"] {
    border-left-color: var(--coral);
}

.response[data-state="error"] .response-label {
    color: var(--coral);
}

.runtime-visual {
    position: relative;
    width: min(100%, 520px);
    aspect-ratio: 1;
    justify-self: center;
    isolation: isolate;
}

.runtime-visual::before {
    content: "";
    position: absolute;
    inset: 15%;
    z-index: -2;
    border-radius: 50%;
    background: radial-gradient(circle, rgba(166, 154, 255, 0.13), transparent 66%);
    filter: blur(18px);
}

.orbit {
    position: absolute;
    inset: 50%;
    border: 1px solid var(--line);
    border-radius: 50%;
    transform: translate(-50%, -50%);
}

.orbit::before {
    content: "";
    position: absolute;
    width: 7px;
    height: 7px;
    top: -4px;
    left: calc(50% - 4px);
    border-radius: 50%;
    background: var(--cyan);
    box-shadow: 0 0 16px rgba(104, 222, 210, 0.9);
}

.orbit-outer {
    width: 89%;
    height: 89%;
    border-style: dashed;
    border-color: rgba(166, 154, 255, 0.22);
    animation: orbit-spin 34s linear infinite;
}

.orbit-middle {
    width: 66%;
    height: 66%;
    animation: orbit-spin 23s linear reverse infinite;
}

.orbit-inner {
    width: 39%;
    height: 39%;
    border-color: rgba(104, 222, 210, 0.28);
    animation: orbit-spin 14s linear infinite;
}

.core {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 112px;
    height: 112px;
    display: grid;
    place-items: center;
    border: 1px solid rgba(166, 154, 255, 0.32);
    border-radius: 31px;
    background: linear-gradient(145deg, rgba(28, 45, 62, 0.98), rgba(10, 21, 31, 0.98));
    box-shadow:
        0 22px 50px rgba(0, 0, 0, 0.4),
        inset 0 1px rgba(255, 255, 255, 0.08),
        0 0 60px rgba(166, 154, 255, 0.1);
    transform: translate(-50%, -50%) rotate(-7deg);
}

.core svg {
    width: 38px;
    margin-top: 8px;
    fill: none;
    stroke: var(--text);
    stroke-width: 2;
    stroke-linejoin: round;
}

.core span {
    margin-top: -20px;
    color: var(--violet);
    font: 700 10px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
    letter-spacing: 0.18em;
}

.runtime-node {
    position: absolute;
    z-index: 2;
    min-width: 112px;
    padding: 12px 14px;
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    background: rgba(13, 27, 39, 0.94);
    box-shadow: var(--shadow);
}

.runtime-node strong,
.runtime-node small {
    display: block;
}

.runtime-node strong {
    color: var(--text);
    font-size: 14px;
}

.runtime-node small {
    margin-top: 4px;
    color: var(--text-faint);
    font: 500 10px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
}

.node-index {
    position: absolute;
    top: 12px;
    right: 12px;
    color: var(--text-faint);
    font: 500 9px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
}

.node-servo {
    top: 7%;
    left: 9%;
    border-top-color: var(--violet);
}

.node-rust {
    top: 25%;
    right: 0;
    border-top-color: var(--coral);
}

.node-php {
    right: 9%;
    bottom: 8%;
    border-top-color: var(--cyan);
}

.signal {
    position: absolute;
    z-index: 1;
    width: 9px;
    height: 9px;
    border: 2px solid var(--cyan);
    border-radius: 50%;
    box-shadow: 0 0 14px rgba(104, 222, 210, 0.75);
}

.signal-a {
    top: 18%;
    right: 22%;
}

.signal-b {
    bottom: 22%;
    left: 16%;
    border-color: var(--violet);
}

.security-note {
    position: absolute;
    bottom: 4%;
    left: 1%;
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 11px 13px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(7, 16, 24, 0.88);
    box-shadow: 0 16px 38px rgba(0, 0, 0, 0.3);
}

.security-note svg {
    width: 23px;
    fill: none;
    stroke: var(--cyan);
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
}

.security-note strong,
.security-note span {
    display: block;
}

.security-note strong {
    font-size: 11px;
}

.security-note span {
    margin-top: 3px;
    color: var(--text-faint);
    font: 500 9px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
}

.runtime-strip {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    border: 1px solid var(--line);
    border-radius: 14px;
    background: rgba(10, 21, 31, 0.66);
}

.runtime-strip article {
    min-width: 0;
    padding: 18px 20px;
}

.runtime-strip article + article {
    border-left: 1px solid var(--line);
}

.runtime-strip span,
.runtime-strip strong,
.runtime-strip small {
    display: block;
}

.runtime-strip span {
    color: var(--text-faint);
    font: 600 9px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
    letter-spacing: 0.12em;
    text-transform: uppercase;
}

.runtime-strip strong {
    margin-top: 9px;
    color: var(--text);
    font-size: 13px;
}

.runtime-strip small {
    margin-top: 4px;
    overflow: hidden;
    color: var(--text-faint);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
}

footer {
    min-height: 84px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--text-faint);
    font-size: 11px;
}

footer kbd {
    padding: 7px 10px;
    color: var(--text-soft);
    border: 1px solid var(--line);
    border-radius: 7px;
    background: var(--ink-soft);
    font: 500 10px/1 "JetBrains Mono", "SFMono-Regular", Consolas, monospace;
}

@keyframes orbit-spin {
    to {
        transform: translate(-50%, -50%) rotate(360deg);
    }
}

@media (max-width: 980px) {
    .hero {
        grid-template-columns: 1fr;
    }

    .runtime-visual {
        width: min(82vw, 520px);
        grid-row: 1;
    }

    .runtime-strip {
        grid-template-columns: repeat(2, 1fr);
    }

    .runtime-strip article:nth-child(3) {
        border-top: 1px solid var(--line);
        border-left: 0;
    }

    .runtime-strip article:nth-child(4) {
        border-top: 1px solid var(--line);
    }
}

@media (max-height: 800px) and (min-width: 981px) {
    .topbar {
        height: 68px;
    }

    .hero {
        min-height: auto;
        padding: 30px 0 26px;
    }

    .eyebrow {
        margin-bottom: 16px;
    }

    h1 {
        font-size: clamp(44px, 5.3vw, 68px);
    }

    .hero-description {
        margin-top: 20px;
        font-size: 15px;
        line-height: 1.5;
    }

    .hello-form {
        margin-top: 20px;
    }

    .response {
        min-height: 96px;
        padding: 14px 18px;
    }

    footer {
        min-height: 64px;
    }
}

@media (max-width: 620px) {
    .app-shell {
        padding: 0 18px;
    }

    .topbar {
        height: 72px;
    }

    .runtime-status > span:not(.status-pulse) {
        position: absolute;
        width: 1px;
        height: 1px;
        overflow: hidden;
        clip: rect(0 0 0 0);
        clip-path: inset(50%);
        white-space: nowrap;
    }

    .hero {
        gap: 40px;
        padding: 42px 0;
    }

    h1 {
        font-size: clamp(42px, 14vw, 62px);
    }

    .hello-form {
        grid-template-columns: 1fr;
    }

    .demo-actions {
        align-items: flex-start;
        flex-direction: column;
    }

    .native-actions {
        grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .native-lab-heading {
        flex-direction: column;
    }

    .runtime-visual {
        width: min(100%, 440px);
    }

    .runtime-node {
        min-width: 96px;
        padding: 10px 12px;
    }

    .security-note {
        display: none;
    }

    .runtime-strip {
        grid-template-columns: 1fr;
    }

    .runtime-strip article + article {
        border-top: 1px solid var(--line);
        border-left: 0;
    }

    footer {
        align-items: flex-start;
        flex-direction: column;
        justify-content: center;
        gap: 10px;
    }
}

@media (prefers-reduced-motion: reduce) {
    *,
    *::before,
    *::after {
        scroll-behavior: auto !important;
        animation-duration: 0.01ms !important;
        animation-iteration-count: 1 !important;
        transition-duration: 0.01ms !important;
    }
}

@media (prefers-contrast: more) {
    :root {
        --text-soft: #d4e0e5;
        --text-faint: #b5c8d0;
        --line: rgba(226, 241, 246, 0.34);
        --line-strong: rgba(226, 241, 246, 0.58);
    }
}

@media (forced-colors: active) {
    :focus-visible {
        outline-color: Highlight;
    }

    .status-pulse,
    .runtime-status[data-state="ready"] .status-pulse,
    .runtime-status[data-state="error"] .status-pulse {
        background: CanvasText;
        box-shadow: none;
    }
}
"#,
    )?;
    write_new(
        &directory.join("resources/app.js"),
        r##"(() => {
    "use strict";

    const form = document.querySelector("#hello-form");
    const name = document.querySelector("#name");
    const button = document.querySelector("#hello-button");
    const response = document.querySelector("#response");
    const label = response.querySelector(".response-label");
    const message = document.querySelector("#response-message");
    const detail = document.querySelector("#response-detail");
    const inspectorButton = document.querySelector("#inspector-button");
    const runtimeStatus = document.querySelector(".runtime-status");
    const runtimeStatusText = document.querySelector("#runtime-status-text");
    const eventStatus = document.querySelector("#event-status");
    const nativeStatus = document.querySelector("#native-status");
    const dropZone = document.querySelector("#drop-zone");
    const saveNoteButton = document.querySelector("#save-note-button");
    const openFileButton = document.querySelector("#open-file-button");
    const copyButton = document.querySelector("#copy-button");
    const notifyButton = document.querySelector("#notify-button");
    const updateButton = document.querySelector("#update-button");
    const updateStatus = document.querySelector("#update-status");

    const setState = (state, title, body, supportingText) => {
        response.dataset.state = state;
        response.setAttribute("aria-live", state === "error" ? "assertive" : "polite");
        label.textContent = title;
        message.textContent = body;
        detail.textContent = supportingText;
    };

    const runNative = async (button, operation) => {
        button.disabled = true;
        try {
            await operation();
        } catch (error) {
            nativeStatus.textContent = error instanceof Error
                ? `Falhou · ${error.message}`
                : "A operação nativa falhou.";
        } finally {
            button.disabled = false;
        }
    };

    if (!window.pam) {
        runtimeStatus.dataset.state = "error";
        runtimeStatusText.textContent = "runtime indisponível";
        setState(
            "error",
            "bridge indisponível",
            "A bridge Pam não foi carregada.",
            "Abra este projeto com `pam desktop dev .`.",
        );
        document.querySelectorAll("button, input").forEach((element) => {
            element.disabled = true;
        });
        return;
    }

    window.pam.on("runtime.ready", ({ protocol }) => {
        runtimeStatus.dataset.state = "ready";
        runtimeStatusText.textContent = "runtime online";
        eventStatus.textContent = `eventos online · IPC v${protocol}`;
    });
    window.pam.on("hello.completed", ({ name: completedName }) => {
        eventStatus.textContent = `hello.completed · ${completedName}`;
    });
    window.pam.on("inspector.opened", () => {
        eventStatus.textContent = "janela inspector aberta";
    });
    window.pam.on("pam.dev.reloaded", ({ kind }) => {
        eventStatus.textContent = kind === 1
            ? "assets recarregados"
            : "worker PHP reiniciado";
    });
    window.pam.on("pam.dev.error", ({ message: reloadError }) => {
        eventStatus.textContent = `hot reload falhou · ${reloadError}`;
    });
    window.pam.on("pam.drag.enter", ({ name }) => {
        dropZone.dataset.active = "true";
        nativeStatus.textContent = `Pronto para receber ${name}.`;
    });
    window.pam.on("pam.drag.leave", () => {
        delete dropZone.dataset.active;
        nativeStatus.textContent = "O arquivo saiu da janela.";
    });
    window.pam.on("pam.drag.drop", async ({ files }) => {
        delete dropZone.dataset.active;
        const [file] = files;
        if (!file) return;
        try {
            if (file.kind === 1) {
                const contents = await window.pam.fs.readText(file);
                nativeStatus.textContent = `${file.name} · ${contents.slice(0, 90)}`;
            } else {
                const entries = await window.pam.fs.list(file);
                nativeStatus.textContent = `${file.name} · ${entries.length} itens`;
            }
        } catch (error) {
            nativeStatus.textContent = error instanceof Error
                ? `Drop bloqueado · ${error.message}`
                : "Não foi possível ler o item solto.";
        }
    });
    window.pam.on("pam.drag.error", ({ message: dragError }) => {
        delete dropZone.dataset.active;
        nativeStatus.textContent = `Drop bloqueado · ${dragError}`;
    });
    window.pam.on("pam.update.changed", ({ state, availableVersion }) => {
        updateStatus.textContent = state === 4
            ? `Versão ${availableVersion} disponível e assinada.`
            : `Estado do updater · ${state}`;
    });
    window.pam.on("pam.update.error", ({ message: updateError }) => {
        updateStatus.textContent = `Updater · ${updateError}`;
    });

    void window.pam.emit("client.ready", {
        loadedAt: new Date().toISOString(),
    }, { timeout: 2_000 }).catch((error) => {
        eventStatus.textContent = error instanceof Error
            ? error.message
            : "eventos indisponíveis";
    });

    inspectorButton.addEventListener("click", async () => {
        inspectorButton.disabled = true;
        try {
            await window.pam.invoke("inspector.open", null, { timeout: 3_000 });
        } catch (error) {
            setState(
                "error",
                "janela não abriu",
                error instanceof Error ? error.message : "Não foi possível abrir o inspector.",
                "O worker continua ativo; tente novamente.",
            );
        } finally {
            inspectorButton.disabled = false;
        }
    });

    saveNoteButton.addEventListener("click", () => {
        void runNative(saveNoteButton, async () => {
            const target = { root: "data", path: "hello.txt" };
            const text = `Olá de Pam Desktop em ${new Date().toLocaleString("pt-BR")}.`;
            await window.pam.fs.writeText(target, text);
            const persisted = await window.pam.fs.readText(target);
            nativeStatus.textContent = `storage/hello.txt · ${persisted}`;
        });
    });

    openFileButton.addEventListener("click", () => {
        void runNative(openFileButton, async () => {
            const file = await window.pam.dialog.openFile({
                title: "Abrir um texto com Pam Desktop",
                filters: [{ name: "Texto", extensions: ["txt", "md", "json"] }],
            });
            if (!file) {
                nativeStatus.textContent = "Seleção cancelada.";
                return;
            }
            const contents = await window.pam.fs.readText(file);
            nativeStatus.textContent = `${file.name} · ${contents.slice(0, 90)}`;
        });
    });

    copyButton.addEventListener("click", () => {
        void runNative(copyButton, async () => {
            const greeting = `Olá, ${name.value.trim() || "mundo"}!`;
            await window.pam.clipboard.writeText(greeting);
            nativeStatus.textContent = `Clipboard · ${greeting}`;
        });
    });

    notifyButton.addEventListener("click", () => {
        void runNative(notifyButton, async () => {
            await window.pam.notification.show({
                title: "Pam Desktop",
                body: "PHP autorizou; Rust entregou.",
                urgency: 2,
            });
            nativeStatus.textContent = "Notificação entregue ao sistema.";
        });
    });

    updateButton.addEventListener("click", async () => {
        updateButton.disabled = true;
        try {
            const update = await window.pam.updater.status();
            updateStatus.textContent = update.state === 1
                ? "Updater desativado. Configure Updates::from() no manifesto PHP."
                : `Estado ${update.state} · versão atual ${update.currentVersion}`;
        } catch (error) {
            updateStatus.textContent = error instanceof Error
                ? `Updater · ${error.message}`
                : "Não foi possível consultar o updater.";
        } finally {
            updateButton.disabled = false;
        }
    });

    form.addEventListener("submit", async (event) => {
        event.preventDefault();
        button.disabled = true;
        button.setAttribute("aria-busy", "true");
        setState(
            "loading",
            "executando no PHP",
            "Enviando um comando tipado para o worker…",
            "O host mantém a interface responsiva durante a operação.",
        );

        try {
            const result = await window.pam.invoke("greet", {
                name: name.value.trim(),
            }, { timeout: 5_000 });
            setState("success", "resposta recebida", result.message, result.detail);
        } catch (error) {
            setState(
                "error",
                "comando interrompido",
                error instanceof Error ? error.message : "Não foi possível executar o comando.",
                "Confira o worker PHP e tente novamente.",
            );
            response.focus();
        } finally {
            button.disabled = false;
            button.removeAttribute("aria-busy");
        }
    });
})();
"##,
    )?;
    write_desktop_inspector(directory)?;
    Ok(())
}

fn init_mobile(
    directory: &Path,
    with_official_ui: bool,
    options: &InitOptions,
) -> Result<(), String> {
    let native_repository = local_native_repository();
    let native_package = native_repository
        .as_ref()
        .map(|repository| repository.package.as_str())
        .unwrap_or("pushinbr/pam-native");
    let mut requirements = serde_json::json!({
        "php": "^8.4"
    });
    requirements[native_package] = serde_json::json!("^0.6");
    if with_official_ui {
        requirements["pushinbr/pam-mobile-ui"] = serde_json::json!("^0.4");
    }
    let mut manifest = serde_json::json!({
        "name": if with_official_ui {
            "app/pam-mobile-ui-project"
        } else {
            "app/pam-native-project"
        },
        "description": if with_official_ui {
            "A native Android application powered by PHP and PAM Mobile UI."
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
            "platform-check": true,
            "sort-packages": true
        },
        "scripts": {
            "mobile:doctor": "pam mobile doctor .",
            "mobile:dev": "pam mobile dev .",
            "mobile:build": "pam mobile build . --release",
            "mobile:benchmark": "pam mobile benchmark .",
            "mobile:profile": "pam mobile profile .",
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
    let native_manifest = serde_json::json!({
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
use Pam\MobileUi\Enum\ThemeMode;
use Pam\MobileUi\PamUI;
use Pam\Native\App;

require __DIR__.'/vendor/autoload.php';

App::components(__DIR__.'/src', __DIR__.'/.pam-native/components');
PamUI::mode(ThemeMode::System);
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
                        ->style(new Style(fontSize: 28)),
                    Button::make('Native taps: '.$this->count)
                        ->onPress($this->increment(...)),
                )->style(new Style(flexGrow: 1, padding: 24, gap: 16)),
            ),
        );
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
        let content = match starter {
            MobileStarter::Blank => {
                r#"
                <Card class="w-full max-w-md gap-6 p-6">
                    <Heading size="2xl">__APP_NAME__</Heading>
                    <Text class="text-muted-foreground">Your native PHP application is ready.</Text>
                    <Button size="lg" on:press="increment">
                        <ButtonText>Native taps: {{ $count }}</ButtonText>
                    </Button>
                </Card>"#
            }
            MobileStarter::Tabs => {
                r#"
                <VStack class="w-full flex-1 justify-between p-6">
                    <VStack class="gap-3">
                        <Heading size="2xl">Home</Heading>
                        <Text class="text-muted-foreground">A production-ready tabs starter.</Text>
                    </VStack>
                    <Row class="w-full justify-between gap-3">
                        <Button variant="secondary">
                            <ButtonText>Home</ButtonText>
                        </Button>
                        <Button variant="ghost">
                            <ButtonText>Search</ButtonText>
                        </Button>
                        <Button variant="ghost">
                            <ButtonText>Profile</ButtonText>
                        </Button>
                    </Row>
                </VStack>"#
            }
            MobileStarter::Authentication => {
                r#"
                <Card class="w-full max-w-md gap-4 p-6">
                    <Heading size="2xl">Welcome back</Heading>
                    <Input bind:value="email" placeholder="Email" />
                    <Input bind:value="password" placeholder="Password" secureTextEntry />
                    <Button size="lg" on:press="submit">
                        <ButtonText>Sign in</ButtonText>
                    </Button>
                </Card>"#
            }
            MobileStarter::Ecommerce => {
                r#"
                <VStack class="w-full flex-1 gap-5 p-6">
                    <Heading size="2xl">__APP_NAME__</Heading>
                    <Input bind:value="query" placeholder="Search products" />
                    <Card class="gap-3 p-5">
                        <Badge variant="secondary">
                            <BadgeText>Featured</BadgeText>
                        </Badge>
                        <Heading>Native commerce starter</Heading>
                        <Text class="text-muted-foreground">
                            Catalog, cart, and checkout foundations.
                        </Text>
                        <Button on:press="increment">
                            <ButtonText>Add to cart · {{ $count }}</ButtonText>
                        </Button>
                    </Card>
                </VStack>"#
            }
            MobileStarter::Chat => {
                r#"
                <VStack class="w-full flex-1 justify-between gap-4 p-6">
                    <VStack class="gap-2">
                        <Heading size="xl">Team chat</Heading>
                        <Text class="text-muted-foreground">Messages stay above the keyboard.</Text>
                    </VStack>
                    <Row class="w-full items-center gap-3">
                        <Input class="flex-1" bind:value="message" placeholder="Message" />
                        <Button on:press="send">
                            <ButtonText>Send</ButtonText>
                        </Button>
                    </Row>
                </VStack>"#
            }
            MobileStarter::Showcase => {
                r#"
                <ScrollView class="w-full flex-1">
                    <VStack class="gap-5 p-6">
                        <Heading size="2xl">Component showcase</Heading>
                        <Badge variant="secondary">
                            <BadgeText>PAM Mobile UI</BadgeText>
                        </Badge>
                        <Card class="gap-3 p-5">
                            <Text>State, components, accessibility, and native layout.</Text>
                            <Button on:press="increment">
                                <ButtonText>Counter {{ $count }}</ButtonText>
                            </Button>
                        </Card>
                    </VStack>
                </ScrollView>"#
            }
        };
        r#"<?php

declare(strict_types=1);

namespace App;

use Pam\Native\Attributes\State;
use Pam\Native\Component;

final class Hello extends Component
{
    #[State]
    public int $count = 0;

    #[State]
    public string $email = '';

    #[State]
    public string $password = '';

    #[State]
    public string $query = '';

    #[State]
    public string $message = '';

    public function increment(): void { $this->count++; }
    public function submit(): void { $this->count++; }
    public function send(): void { $this->message = ''; }
}
?>

<template>
    <PamUIProvider mode="system">
        <SafeAreaView class="flex-1 ui-surface">
            <Center class="flex-1 px-6">__CONTENT__
            </Center>
        </SafeAreaView>
    </PamUIProvider>
</template>
"#
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
    print_init_success(directory, InitTemplate::Laravel, options.socket);
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
        .join("packages/api/composer.json")
        .is_file()
        .then(|| {
            serde_json::json!({
                "type": "path",
                "url": packages.to_string_lossy(),
                "options": {
                    "symlink": false,
                    "versions": {
                        "pushinbr/pam-core-api": "1.0.0",
                        "pushinbr/pam-api": "2.0.0",
                        "pushinbr/pam-psr-bridge": "1.0.0",
                        "pushinbr/pam-socket": "1.0.0",
                        "pushinbr/pam-testing": "1.0.0"
                    }
                }
            })
        })
}

fn local_desktop_repository() -> Option<serde_json::Value> {
    let manifest_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let configured = std::env::var_os("PAM_DESKTOP_PACKAGE_PATH").map(PathBuf::from);
    let candidates = [
        configured,
        Some(manifest_root.join("../pam-desktop/packages/desktop")),
        Some(manifest_root.join("pam-desktop/packages/desktop")),
    ];
    candidates
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
                        "pushinbr/pam-desktop": "1.2.0"
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
    let manifest_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let configured = std::env::var_os("PAM_NATIVE_PACKAGE_PATH").map(PathBuf::from);
    let installed = std::env::current_exe().ok().and_then(|executable| {
        executable
            .parent()
            .map(|binary| binary.join("../share/pam/native/packages/native"))
    });
    let candidates = [
        configured,
        installed,
        Some(manifest_root.join("pam-native/packages/native")),
        Some(manifest_root.join("../pam-native/packages/native")),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|path| path.join("composer.json").is_file())
        .and_then(|path| {
            let path = fs::canonicalize(&path).unwrap_or(path);
            let composer = fs::read(path.join("composer.json")).ok()?;
            let manifest = serde_json::from_slice::<serde_json::Value>(&composer).ok()?;
            let package = manifest.get("name")?.as_str()?.to_owned();
            let version = path
                .parent()?
                .parent()
                .and_then(|root| cargo_manifest_version(&root.join("Cargo.toml")))?;
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
    let manifest_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let configured = std::env::var_os("PAM_MOBILE_UI_PACKAGE_PATH").map(PathBuf::from);
    let installed = std::env::current_exe().ok().and_then(|executable| {
        executable
            .parent()
            .map(|binary| binary.join("../share/pam/mobile-ui"))
    });
    let candidates = [
        configured,
        installed,
        Some(manifest_root.join("pam-mobile-ui")),
        Some(manifest_root.join("../pam-mobile-ui")),
    ];
    candidates
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
                        "pushinbr/pam-mobile-ui": "0.1.0"
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

fn print_init_success(directory: &Path, template: InitTemplate, socket: bool) {
    let preset = match (template, socket) {
        (InitTemplate::Raw, false) => "raw",
        (InitTemplate::Raw, true) => "raw + Socket",
        (InitTemplate::Api, false) => "API",
        (InitTemplate::Api, true) => "API + Socket",
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
    } else if template == InitTemplate::Desktop {
        format!("cd {} && pam desktop dev .", directory.display())
    } else if matches!(template, InitTemplate::Mobile | InitTemplate::MobileUi) {
        format!("cd {} && pam mobile dev .", directory.display())
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
    use super::{
        HttpEndpoint, current_worker_lag_seconds, parse_control_plane_diagnostics,
        visible_top_metric,
    };

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
