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
use std::time::Instant;

use crate::composer;
use crate::package_coordinates;
use crate::php::PhpRuntime;
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

pub fn top(address: &str, iterations: usize, interval: std::time::Duration) -> Result<u8, String> {
    if iterations == 0 || interval.is_zero() {
        return Err("top iterations and interval must be positive".to_owned());
    }
    let endpoint = HttpEndpoint::parse(&format!("{}/metrics", address.trim_end_matches('/')))?;
    let ui = Terminal::stdout();
    for iteration in 0..iterations {
        if iteration > 0 {
            std::thread::sleep(interval);
        }
        let body = endpoint.response_body()?;
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
        for line in body.lines().filter(|line| {
            line.starts_with("pam_http_")
                || line.starts_with("pam_websocket_")
                || line.starts_with("pam_event_loop_")
                || line.starts_with("pam_process_")
                || line.starts_with("pam_php_")
                || line.starts_with("pam_cluster_")
        }) {
            if let Some((metric, value)) = line.split_once(' ') {
                println!("  {} {}", ui.accent(format!("{metric:<48}")), value);
            } else {
                println!("  {}", ui.muted(line));
            }
        }
    }
    Ok(0)
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
            _ => Err(format!(
                "unknown init template {value:?}; expected raw, api, laravel, desktop, mobile, or mobile-ui"
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
        InitTemplate::Desktop | InitTemplate::Mobile | InitTemplate::MobileUi
    ) && socket
    {
        return Err(
            "the selected desktop or mobile preset does not use --socket; it exposes its own native event system".to_owned(),
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
    } else if matches!(template, InitTemplate::Mobile | InitTemplate::MobileUi) {
        init_mobile(directory, template == InitTemplate::MobileUi)?;
    } else {
        init_api(directory, socket)?;
    }

    if options.install && directory.join("composer.json").is_file() {
        run_composer_in(executable, directory, &["install", "--no-interaction"])?;
    }
    print_init_success(directory, template, socket);
    Ok(0)
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
    let php_library_name = php_library
        .file_name()
        .ok_or_else(|| "linked PHP library has no filename".to_owned())?;
    fs::copy(&php_library, library_directory.join(php_library_name))
        .map_err(|error| format!("cannot copy {}: {error}", php_library.display()))?;

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
        php_library: format!("lib/{}", php_library_name.to_string_lossy()),
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

fn linked_php_library(executable: &Path) -> Result<PathBuf, String> {
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
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let line = line.trim();
            if !line.starts_with("libphp") {
                return None;
            }
            let path = line.split("=>").nth(1)?.split_whitespace().next()?;
            Some(PathBuf::from(path))
        })
        .filter(|path| path.is_file())
        .ok_or_else(|| "cannot locate the PHP Embed shared library linked by Pam".to_owned())
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
            if path == output {
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
    let version = package_coordinates::VERSION_CONSTRAINT;
    let mut require = serde_json::Map::from_iter([
        ("php".to_owned(), serde_json::json!("^8.4")),
        (
            package_coordinates::API.to_owned(),
            serde_json::json!(version),
        ),
    ]);
    if socket {
        require.insert(
            package_coordinates::SOCKET.to_owned(),
            serde_json::json!(version),
        );
    }
    let require_dev = serde_json::Map::from_iter([
        (
            package_coordinates::TESTING.to_owned(),
            serde_json::json!(version),
        ),
        ("phpunit/phpunit".to_owned(), serde_json::json!("^12.5")),
    ]);
    let mut manifest = serde_json::json!({
        "name": "app/pam-project",
        "description": "Application powered by the Pam persistent PHP runtime.",
        "type": "project",
        "license": "proprietary",
        "require": require,
        "require-dev": require_dev,
        "autoload": {"psr-4": {"App\\": "src/"}},
        "config": {"platform-check": true, "sort-packages": true},
        "scripts": {
            "dev": "pam dev index.php",
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

use Pam\App;
use Pam\Http\Request;
use Pam\Http\Response;

$app = new App();
{socket_setup}
$app->get('/api/ping', static fn (Request $request, Response $response): Response => $response->json([
    'message' => 'pong',
]));
$app->listen((int) (getenv('PAM_PORT') ?: 3000));
"#
        ),
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
    fs::create_dir_all(directory.join("tests"))
        .map_err(|error| format!("cannot create test directory: {error}"))?;
    write_new(
        &directory.join("tests/ApplicationTest.php"),
        r#"<?php

declare(strict_types=1);

use Pam\App;
use Pam\Http\Request;
use Pam\Http\Response;
use Pam\Testing\TestClient;
use PHPUnit\Framework\TestCase;

final class ApplicationTest extends TestCase
{
    public function testPingEndpoint(): void
    {
        $app = new App(discoverPackages: false);
        $app->get('/api/ping', static fn (Request $request, Response $response): Response =>
            $response->json(['message' => 'pong']));

        (new TestClient($app))
            ->get('/api/ping')
            ->assertSuccessful()
            ->assertJson(['message' => 'pong']);
        self::addToAssertionCount(1);
    }
}
"#,
    )?;
    Ok(())
}

fn write_desktop_inspector(directory: &Path) -> Result<(), String> {
    write_new(
        &directory.join("resources/inspector.html"),
        r##"<!doctype html>
<html lang="en">
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
                    <span>secondary window</span>
                    <h1>Runtime Inspector</h1>
                </div>
            </div>
            <button id="hide-button" type="button" aria-label="Hide Runtime Inspector">
                <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="m7 7 10 10M17 7 7 17"/>
                </svg>
            </button>
        </header>

        <section class="summary" aria-labelledby="summary-title">
            <div>
                <span class="eyebrow">PAM DESKTOP 1.1</span>
                <h2 id="summary-title">One runtime.<br><strong>Multiple windows.</strong></h2>
            </div>
            <span class="online"><i aria-hidden="true"></i> worker online</span>
        </section>

        <section class="metrics" aria-label="Runtime state">
            <article>
                <span>window id</span>
                <strong id="window-id">—</strong>
                <small>context isolation</small>
            </article>
            <article>
                <span>protocol</span>
                <strong>IPC v6</strong>
                <small>typed contract</small>
            </article>
            <article>
                <span>renderer</span>
                <strong>Servo</strong>
                <small>native Rust host</small>
            </article>
        </section>

        <section class="event-log" aria-labelledby="events-title">
            <div class="section-heading">
                <div>
                    <span>STREAM</span>
                    <h2 id="events-title">Application events</h2>
                </div>
                <span class="live"><i aria-hidden="true"></i> live</span>
            </div>
            <ol id="event-list" aria-live="polite">
                <li>
                    <time>now</time>
                    <span>inspector.ready</span>
                    <small>waiting for PHP events</small>
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

        time.textContent = new Intl.DateTimeFormat("en-US", {
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
        appendEvent("bridge.error", "The Pam bridge did not load.");
        hideButton.disabled = true;
        return;
    }

    windowId.textContent = window.pam.windowId;
    window.pam.on("runtime.ready", ({ apiVersion, protocol }) => {
        appendEvent("runtime.ready", `API v${apiVersion} · IPC v${protocol}`);
    });
    window.pam.on("pam.dev.reloaded", ({ kind }) => {
        appendEvent("pam.dev.reloaded", kind === 1 ? "assets" : "PHP worker");
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
                error instanceof Error ? error.message : "Unknown failure",
            );
            hideButton.disabled = false;
        }
    });

    void window.pam.emit("client.ready", {
        loadedAt: new Date().toISOString(),
    }, { timeout: 2_000 }).catch((error) => {
        appendEvent(
            "client.ready.failed",
            error instanceof Error ? error.message : "Unknown failure",
        );
    });
})();
"##,
    )?;
    Ok(())
}

fn init_desktop(directory: &Path) -> Result<(), String> {
    let require = serde_json::Map::from_iter([
        ("php".to_owned(), serde_json::json!("^8.4")),
        (
            package_coordinates::DESKTOP.to_owned(),
            serde_json::json!(package_coordinates::DESKTOP_VERSION_CONSTRAINT),
        ),
    ]);
    let mut manifest = serde_json::json!({
        "name": "app/pam-desktop-project",
        "description": "A PHP-first desktop application powered by Pam, Rust, and Servo.",
        "type": "project",
        "license": "proprietary",
        "require": require,
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

use Pam\Desktop\Application;
use Pam\Desktop\ApplicationCategory;
use Pam\Desktop\BackgroundJob;
use Pam\Desktop\Capabilities;
use Pam\Desktop\ClientEvent;
use Pam\Desktop\CommandContext;
use Pam\Desktop\CommandResult;
use Pam\Desktop\EventContext;
use Pam\Desktop\FileSystemRoot;
use Pam\Desktop\GlobalShortcut;
use Pam\Desktop\JobContext;
use Pam\Desktop\Menu;
use Pam\Desktop\MenuItem;
use Pam\Desktop\Shell;
use Pam\Desktop\ShellEffect;
use Pam\Desktop\Tray;
use Pam\Desktop\TrayCloseBehavior;
use Pam\Desktop\Window;
use Pam\Desktop\WindowEffect;
use Pam\Desktop\WindowTheme;

require __DIR__.'/vendor/autoload.php';

$app = Application::make(
    id: 'com.pushin.pam-hello',
    name: 'Pam Hello',
    version: '1.0.0',
    window: Window::create('Pam Desktop · Hello')
        ->load('resources/index.html')
        ->size(1120, 720)
        ->minimumSize(720, 520)
        ->theme(WindowTheme::Dark),
)
    ->description('An elegant desktop application orchestrated in PHP.')
    ->publisher('Pushin')
    ->category(ApplicationCategory::Development)
    ->excludeFromBundle('storage/hello.txt')
    ->window(
        'inspector',
        Window::create('Pam Desktop · Runtime Inspector')
            ->load('resources/inspector.html')
            ->minimumSize(480, 360)
            ->size(680, 520)
            ->visible(false)
            ->theme(WindowTheme::Dark),
    )
    ->capabilities(
        Capabilities::none()
            ->filesystem(FileSystemRoot::readWrite('data', __DIR__.'/storage'))
            ->dialogs()
            ->clipboard()
            ->notifications()
            ->dragAndDrop(),
    )
    ->shell(
        Shell::none()
            ->menu(Menu::create(
                'application',
                'Pam Hello',
                MenuItem::command('app.show', 'Show window', 'CmdOrCtrl+Shift+KeyP'),
                MenuItem::command('inspector.show', 'Runtime Inspector'),
                MenuItem::checkbox('background.enabled', 'Run in background', true),
                MenuItem::separator(),
                MenuItem::command('app.quit', 'Quit'),
            ))
            ->tray(
                Tray::create('application', 'Pam Desktop · Hello')
                    ->closeBehavior(TrayCloseBehavior::Hide),
            )
            ->shortcut(
                GlobalShortcut::create('app.show', 'CmdOrCtrl+Shift+KeyP'),
            ),
    )
    ->plugin(new App\HelloPlugin())
    ->job(
        'runtime.heartbeat',
        BackgroundJob::every(30_000)->timeout(3_000),
        static fn (JobContext $job): CommandResult =>
            CommandResult::success([
                'runId' => $job->runId,
                'startedAtMs' => $job->startedAtMilliseconds,
            ])->event(new ClientEvent(
                name: 'runtime.heartbeat',
                payload: ['runId' => $job->runId],
            )),
    )
    ->commandTimeout(10_000);

$app->command('greet', static function (CommandContext $command): CommandResult {
    $name = trim((string) $command->string('name', 'world'));
    $name = $name !== '' ? mb_substr($name, 0, 40) : 'world';

    return CommandResult::success([
        'message' => "Hello, {$name}.",
        'detail' => 'This response left PHP, crossed the Rust host, and reached Servo.',
    ])
        ->effect(WindowEffect::title("Pam Desktop · {$name}", $command->windowId))
        ->event(new ClientEvent(
            name: 'hello.completed',
            payload: ['name' => $name],
            windowId: $command->windowId,
        ));
});

$app->command('inspector.open', static fn (CommandContext $command): CommandResult =>
    CommandResult::success(['windowId' => 'inspector'])
        ->effect(WindowEffect::visible(true, 'inspector'))
        ->effect(WindowEffect::focus('inspector'))
        ->event(new ClientEvent(
            name: 'inspector.opened',
            payload: ['sourceWindowId' => $command->windowId],
            windowId: $command->windowId,
        )));

$app->command('inspector.hide', static fn (): CommandResult =>
    CommandResult::success()
        ->effect(WindowEffect::visible(false, 'inspector')));

$app->on('client.ready', static fn (EventContext $event): CommandResult =>
    CommandResult::success()
        ->event(new ClientEvent(
            name: 'runtime.ready',
            payload: [
                'windowId' => $event->windowId,
                'apiVersion' => Application::API_VERSION,
                'protocol' => Application::PROTOCOL_VERSION,
            ],
            windowId: $event->windowId,
        )));

$app->on('pam.menu.selected', static function (EventContext $event): CommandResult {
    static $backgroundEnabled = true;

    return match ($event->string('id')) {
        'app.show' => CommandResult::success()
            ->effect(WindowEffect::visible(true))
            ->effect(WindowEffect::focus()),
        'inspector.show' => CommandResult::success()
            ->effect(WindowEffect::visible(true, 'inspector'))
            ->effect(WindowEffect::focus('inspector')),
        'background.enabled' => CommandResult::success([
            'enabled' => $backgroundEnabled = !$backgroundEnabled,
        ])->effect(ShellEffect::menuChecked('background.enabled', $backgroundEnabled)),
        'app.quit' => CommandResult::success()->effect(WindowEffect::close()),
        default => CommandResult::success(),
    };
});

$app->on('pam.shortcut.changed', static fn (EventContext $event): CommandResult =>
    $event->integer('state') === 1
        ? CommandResult::success()
            ->effect(WindowEffect::visible(true))
            ->effect(WindowEffect::focus())
        : CommandResult::success());

$app->run();
"#,
    )?;
    fs::create_dir_all(directory.join("src"))
        .map_err(|error| format!("cannot create desktop source directory: {error}"))?;
    write_new(
        &directory.join("src/HelloPlugin.php"),
        r#"<?php

declare(strict_types=1);

namespace App;

use Pam\Desktop\Application;
use Pam\Desktop\CommandContext;
use Pam\Desktop\Plugin;

final class HelloPlugin implements Plugin
{
    public function identifier(): string
    {
        return 'hello.runtime';
    }

    public function register(Application $application): void
    {
        $application->command(
            'runtime.snapshot',
            static fn (CommandContext $command): array => [
                'php' => PHP_VERSION,
                'os' => PHP_OS_FAMILY,
                'architecture' => php_uname('m'),
                'windowId' => $command->windowId,
                'plugin' => 'hello.runtime',
            ],
        );
    }
}
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
                <span>runtime online</span>
                <kbd>1.0</kbd>
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
                                    placeholder="Seu nome"
                                    value="David"
                                >
                            </div>
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
                                <span>CAPABILITIES 0.3</span>
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

                    <section class="update-console" aria-labelledby="extension-title">
                        <div>
                            <span>STABLE API · 1.0</span>
                            <h2 id="extension-title">Plugins PHP + Rust isolado</h2>
                            <p id="extension-status" role="status" aria-live="polite">
                                PHP compõe a aplicação; plugins Rust rodam em processos supervisionados.
                            </p>
                        </div>
                        <button id="extension-button" type="button">Consultar plugin</button>
                    </section>

                    <section class="update-console" aria-labelledby="update-title">
                        <div>
                            <span>SIGNED UPDATES</span>
                            <h2 id="update-title">Atualizações com rollback</h2>
                            <p id="update-status" role="status" aria-live="polite">
                                Desativadas por padrão; a chave pública fica no manifesto PHP.
                            </p>
                        </div>
                        <button id="update-button" type="button">Verificar estado</button>
                    </section>

                    <div class="response" id="response" role="status" aria-live="polite">
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
                    <strong>Servo 0.4</strong>
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
                    <strong>API v1 · IPC v6</strong>
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
    background: var(--cyan);
    border-radius: 8px;
    transform: translateY(-160%);
}

.skip-link:focus {
    transform: translateY(0);
}

:focus-visible {
    outline: 3px solid var(--cyan);
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
    background: linear-gradient(105deg, var(--violet) 10%, #d7d1ff 54%, var(--cyan));
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
    color: #10131d;
    border: 0;
    border-radius: 12px;
    background: linear-gradient(120deg, #c4bbff, var(--violet));
    box-shadow: 0 12px 28px rgba(119, 101, 235, 0.23);
    cursor: pointer;
    font-size: 14px;
    font-weight: 700;
    transition: filter 180ms ease, transform 180ms ease, box-shadow 180ms ease;
}

.hello-form button:hover {
    filter: brightness(1.08);
    box-shadow: 0 16px 34px rgba(119, 101, 235, 0.32);
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
        display: none;
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
        width: min(96vw, 440px);
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
    const eventStatus = document.querySelector("#event-status");
    const nativeStatus = document.querySelector("#native-status");
    const dropZone = document.querySelector("#drop-zone");
    const saveNoteButton = document.querySelector("#save-note-button");
    const openFileButton = document.querySelector("#open-file-button");
    const copyButton = document.querySelector("#copy-button");
    const notifyButton = document.querySelector("#notify-button");
    const extensionButton = document.querySelector("#extension-button");
    const extensionStatus = document.querySelector("#extension-status");
    const updateButton = document.querySelector("#update-button");
    const updateStatus = document.querySelector("#update-status");

    const setState = (state, title, body, supportingText) => {
        response.dataset.state = state;
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
                ? `Failed · ${error.message}`
                : "The native operation failed.";
        } finally {
            button.disabled = false;
        }
    };

    if (!window.pam) {
        setState(
            "error",
            "bridge unavailable",
            "The Pam bridge did not load.",
            "Open this project with composer desktop:dev.",
        );
        form.querySelectorAll("button, input").forEach((element) => {
            element.disabled = true;
        });
        inspectorButton.disabled = true;
        extensionButton.disabled = true;
        updateButton.disabled = true;
        return;
    }

    if (window.pam.apiVersion !== 1) {
        setState(
            "error",
            "incompatible API",
            `This application requires API v1; the host provided v${window.pam.apiVersion}.`,
            "Install a PAM Desktop 1.x host to continue.",
        );
        form.querySelectorAll("button, input").forEach((element) => {
            element.disabled = true;
        });
        inspectorButton.disabled = true;
        extensionButton.disabled = true;
        updateButton.disabled = true;
        return;
    }

    eventStatus.textContent = "API v1 · events connecting…";
    window.pam.on("runtime.ready", ({ apiVersion, protocol }) => {
        eventStatus.textContent = `API v${apiVersion} · IPC v${protocol} online`;
    });
    window.pam.on("hello.completed", ({ name: completedName }) => {
        eventStatus.textContent = `hello.completed · ${completedName}`;
    });
    window.pam.on("inspector.opened", () => {
        eventStatus.textContent = "inspector window opened";
    });
    window.pam.on("pam.dev.reloaded", ({ kind }) => {
        eventStatus.textContent = kind === 1
            ? "assets reloaded"
            : "PHP worker restarted";
    });
    window.pam.on("pam.dev.error", ({ message: reloadError }) => {
        eventStatus.textContent = `hot reload failed · ${reloadError}`;
    });
    window.pam.on("pam.menu.selected", ({ id }) => {
        eventStatus.textContent = `native menu · ${id}`;
    });
    window.pam.on("pam.tray.activated", ({ button: trayButton }) => {
        eventStatus.textContent = `tray activated · button ${trayButton}`;
    });
    window.pam.on("pam.shortcut.changed", ({ id, state }) => {
        if (state === 1) {
            eventStatus.textContent = `global shortcut · ${id}`;
        }
    });
    window.pam.on("pam.job.completed", ({ id, runId }) => {
        extensionStatus.textContent = `${id} · run #${runId} completed`;
    });
    window.pam.on("pam.drag.enter", ({ name }) => {
        dropZone.dataset.active = "true";
        nativeStatus.textContent = `Ready to receive ${name}.`;
    });
    window.pam.on("pam.drag.leave", () => {
        delete dropZone.dataset.active;
        nativeStatus.textContent = "The file left the window.";
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
                nativeStatus.textContent = `${file.name} · ${entries.length} items`;
            }
        } catch (error) {
            nativeStatus.textContent = error instanceof Error
                ? `Drop blocked · ${error.message}`
                : "The dropped item could not be read.";
        }
    });
    window.pam.on("pam.drag.error", ({ message: dragError }) => {
        delete dropZone.dataset.active;
        nativeStatus.textContent = `Drop blocked · ${dragError}`;
    });
    window.pam.on("pam.update.changed", ({ state, availableVersion }) => {
        updateStatus.textContent = state === 4
            ? `Version ${availableVersion} is available and signed.`
            : `Updater state · ${state}`;
    });
    window.pam.on("pam.update.error", ({ message: updateError }) => {
        updateStatus.textContent = `Updater · ${updateError}`;
    });

    void window.pam.emit("client.ready", {
        loadedAt: new Date().toISOString(),
    }, { timeout: 2_000 }).catch((error) => {
        eventStatus.textContent = error instanceof Error
            ? error.message
            : "events unavailable";
    });

    inspectorButton.addEventListener("click", async () => {
        inspectorButton.disabled = true;
        try {
            await window.pam.invoke("inspector.open", null, { timeout: 3_000 });
        } catch (error) {
            setState(
                "error",
                "window did not open",
                error instanceof Error ? error.message : "The inspector could not be opened.",
                "The worker is still running; try again.",
            );
        } finally {
            inspectorButton.disabled = false;
        }
    });

    saveNoteButton.addEventListener("click", () => {
        void runNative(saveNoteButton, async () => {
            const target = { root: "data", path: "hello.txt" };
            const text = `Hello from Pam Desktop at ${new Date().toLocaleString("en-US")}.`;
            await window.pam.fs.writeText(target, text);
            const persisted = await window.pam.fs.readText(target);
            nativeStatus.textContent = `storage/hello.txt · ${persisted}`;
        });
    });

    openFileButton.addEventListener("click", () => {
        void runNative(openFileButton, async () => {
            const file = await window.pam.dialog.openFile({
                title: "Open a text file with Pam Desktop",
                filters: [{ name: "Text", extensions: ["txt", "md", "json"] }],
            });
            if (!file) {
                nativeStatus.textContent = "Selection cancelled.";
                return;
            }
            const contents = await window.pam.fs.readText(file);
            nativeStatus.textContent = `${file.name} · ${contents.slice(0, 90)}`;
        });
    });

    copyButton.addEventListener("click", () => {
        void runNative(copyButton, async () => {
            const greeting = `Hello, ${name.value.trim() || "world"}!`;
            await window.pam.clipboard.writeText(greeting);
            nativeStatus.textContent = `Clipboard · ${greeting}`;
        });
    });

    notifyButton.addEventListener("click", () => {
        void runNative(notifyButton, async () => {
            await window.pam.notification.show({
                title: "Pam Desktop",
                body: "Authorized by PHP. Delivered by Rust.",
                urgency: 2,
            });
            nativeStatus.textContent = "Notification delivered to the system.";
        });
    });

    extensionButton.addEventListener("click", async () => {
        extensionButton.disabled = true;
        try {
            const snapshot = await window.pam.invoke(
                "runtime.snapshot",
                null,
                { timeout: 3_000 },
            );
            extensionStatus.textContent =
                `${snapshot.plugin} · PHP ${snapshot.php} · ${snapshot.os}/${snapshot.architecture}`;
        } catch (error) {
            extensionStatus.textContent = error instanceof Error
                ? `Plugin PHP · ${error.message}`
                : "The PHP plugin could not be queried.";
        } finally {
            extensionButton.disabled = false;
        }
    });

    updateButton.addEventListener("click", async () => {
        updateButton.disabled = true;
        try {
            const update = await window.pam.updater.status();
            updateStatus.textContent = update.state === 1
                ? "Updater disabled. Configure Updates::from() in the PHP manifest."
                : `State ${update.state} · current version ${update.currentVersion}`;
        } catch (error) {
            updateStatus.textContent = error instanceof Error
                ? `Updater · ${error.message}`
                : "The updater could not be queried.";
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
            "running in PHP",
            "Sending a typed command to the worker…",
            "The host keeps the interface responsive during the operation.",
        );

        try {
            const result = await window.pam.invoke("greet", {
                name: name.value.trim(),
            }, { timeout: 5_000 });
            setState("success", "response received", result.message, result.detail);
        } catch (error) {
            setState(
                "error",
                "command interrupted",
                error instanceof Error ? error.message : "The command could not be executed.",
                "Check the PHP worker and try again.",
            );
        } finally {
            button.disabled = false;
            button.removeAttribute("aria-busy");
        }
    });
})();
"##,
    )?;
    fs::write(
        directory.join("resources/index.html"),
        include_str!("templates/desktop/index.html"),
    )
    .map_err(|error| format!("cannot write desktop interface: {error}"))?;
    fs::write(
        directory.join("resources/styles.css"),
        include_str!("templates/desktop/styles.css"),
    )
    .map_err(|error| format!("cannot write desktop styles: {error}"))?;
    write_desktop_inspector(directory)?;
    Ok(())
}

fn init_mobile(directory: &Path, with_official_ui: bool) -> Result<(), String> {
    let native_repository = local_native_repository();
    let native_package = native_repository
        .as_ref()
        .map(|repository| repository.package.clone())
        .unwrap_or_else(|| package_coordinates::NATIVE.to_string());
    let mut requirements = serde_json::json!({
        "php": "^8.4"
    });
    requirements[&native_package] =
        serde_json::json!(package_coordinates::NATIVE_VERSION_CONSTRAINT);
    if with_official_ui {
        requirements[package_coordinates::MOBILE_UI] =
            serde_json::json!(package_coordinates::MOBILE_UI_VERSION_CONSTRAINT);
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
            "mobile:devtools": "pam mobile devtools ."
        }
    });
    if with_official_ui && native_package != package_coordinates::NATIVE {
        // Source checkouts may still expose the legacy package identity while
        // the public pushinbr namespace migration is being completed.
        manifest["replace"] = serde_json::json!({
            package_coordinates::NATIVE: package_coordinates::NATIVE_LOCAL_VERSION
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
    write_new(
        &directory.join("pam-native.json"),
        &format!(
            r#"{{
    "$schema": "vendor/{native_package}/resources/pam-native.schema.json",
    "version": 1,
    "applicationId": "app.pam.hello",
    "name": "Pam Hello",
    "entry": "index.php",
    "versionCode": 1,
    "versionName": "0.1.0",
    "android": {{
        "minSdk": 26,
        "targetSdk": 36,
        "permissions": []
    }},
    "modules": [],
    "views": []
}}
"#,
        ),
    )?;
    let entry = if with_official_ui {
        r#"<?php

declare(strict_types=1);

use App\Hello;
use Pam\MobileUi\Enum\ThemeMode;
use Pam\MobileUi\MobileUi;
use Pam\Native\App;

require __DIR__.'/vendor/autoload.php';

App::components(__DIR__.'/src', __DIR__.'/.pam-native/components');
MobileUi::mode(ThemeMode::System);
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
    let (hello_path, hello) = if with_official_ui {
        (
            directory.join("src/Hello.pam.php"),
            r#"<?php

declare(strict_types=1);

namespace App;

use Pam\Native\Attributes\State;
use Pam\Native\Component;

final class Hello extends Component
{
    #[State]
    public int $count = 0;

    public function increment(): void
    {
        $this->count++;
    }
}
?>

<template>
    <PamUIProvider mode="system">
        <SafeAreaView class="flex-1 ui-surface">
            <Center class="flex-1 px-6">
                <Card class="w-full max-w-md gap-6 p-6">
                    <VStack class="gap-2">
                        <Badge variant="secondary">
                            <BadgeText>PAM Mobile UI</BadgeText>
                        </Badge>
                        <Heading size="2xl">Build native apps with PHP</Heading>
                        <Text class="text-muted-foreground">
                            Accessible official components on the PAM Native renderer.
                        </Text>
                    </VStack>

                    <Button size="lg" on:press="increment">
                        <ButtonText>Native taps: {{ $count }}</ButtonText>
                    </Button>
                </Card>
            </Center>
        </SafeAreaView>
    </PamUIProvider>
</template>
"#,
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
"#,
        )
    };
    write_new(&hello_path, hello)?;
    write_new(
        &directory.join(".gitignore"),
        "/vendor/\n/.pam/\n/.pam-native/\n",
    )?;
    fs::create_dir_all(directory.join(".vscode"))
        .map_err(|error| format!("cannot create VS Code settings directory: {error}"))?;
    write_new(
        &directory.join(".vscode/settings.json"),
        &format!(
            r#"{{
    "files.associations": {{
        "*.pam": "html"
    }},
    "html.customData": [
        "./vendor/{native_package}/resources/pam-native.custom-data.json"
    ]
}}
"#,
        ),
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
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("packages/*");
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("packages/api/composer.json")
        .is_file()
        .then(|| {
            let versions = package_coordinates::ALL
                .into_iter()
                .filter(|package| {
                    *package != package_coordinates::SKELETON
                        && *package != package_coordinates::DESKTOP
                })
                .map(|package| {
                    (
                        package.to_owned(),
                        serde_json::json!(package_coordinates::LOCAL_VERSION),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            serde_json::json!({
                "type": "path",
                "url": packages.to_string_lossy(),
                "options": {
                    "symlink": true,
                    "versions": versions
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
                    "symlink": true,
                    "versions": {
                        package_coordinates::DESKTOP:
                            package_coordinates::DESKTOP_LOCAL_VERSION
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
            let definition = serde_json::json!({
                "type": "path",
                "url": path.to_string_lossy(),
                "options": {
                    "symlink": false,
                    "versions": {
                        package.clone(): package_coordinates::NATIVE_LOCAL_VERSION
                    }
                }
            });
            Some(LocalComposerRepository {
                package,
                definition,
            })
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
                        package_coordinates::MOBILE_UI:
                            package_coordinates::MOBILE_UI_LOCAL_VERSION
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
    let next = if template == InitTemplate::Desktop {
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
        })
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
        write!(
            stream,
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            self.target, self.host
        )
        .map_err(|error| error.to_string())?;
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .map_err(|error| error.to_string())?;
        Ok(response)
    }
}

pub fn default_script(target: Option<OsString>) -> PathBuf {
    PathBuf::from(target.unwrap_or_else(|| OsString::from("index.php")))
}
