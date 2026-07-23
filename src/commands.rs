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
use crate::php::PhpRuntime;
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
    for iteration in 0..iterations {
        if iteration > 0 {
            std::thread::sleep(interval);
        }
        let body = endpoint.response_body()?;
        print!(
            "\x1b[2J\x1b[HPam top — sample {}/{}\n\n",
            iteration + 1,
            iterations
        );
        for line in body.lines().filter(|line| {
            line.starts_with("pam_http_")
                || line.starts_with("pam_websocket_")
                || line.starts_with("pam_event_loop_")
                || line.starts_with("pam_process_")
                || line.starts_with("pam_php_")
                || line.starts_with("pam_cluster_")
        }) {
            println!("{line}");
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
}

impl InitTemplate {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "raw" | "pure" => Ok(Self::Raw),
            "api" => Ok(Self::Api),
            "laravel" => Ok(Self::Laravel),
            _ => Err(format!(
                "unknown init template {value:?}; expected raw, api, or laravel"
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
            "vendor/autoload.php is missing; run composer install before pam build".to_owned(),
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

    println!("Built production bundle at {}", output.display());
    println!("Run: {}/bin/pam-run", output.display());
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

    println!("Create a Pam project:\n");
    println!("  1) Raw Pam runtime");
    println!("  2) Pam API");
    println!("  3) Pam API + Socket");
    println!("  4) Laravel on Pam");
    println!("  5) Laravel on Pam + Socket");
    print!("\nChoose a preset [2]: ");
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
    let version = "^0.1";
    let mut require = serde_json::Map::from_iter([
        ("php".to_owned(), serde_json::json!("^8.4")),
        ("pam/api".to_owned(), serde_json::json!(version)),
    ]);
    if socket {
        require.insert("pam/socket".to_owned(), serde_json::json!(version));
    }
    let mut manifest = serde_json::json!({
        "name": "app/pam-project",
        "type": "project",
        "require": require,
        "require-dev": {
            "pam/testing": version,
            "phpunit/phpunit": "^12.5"
        },
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
            serde_json::json!({
                "type": "path",
                "url": packages.to_string_lossy(),
                "options": {
                    "symlink": true,
                    "versions": {
                        "pam/core-api": "0.1.0",
                        "pam/api": "0.1.0",
                        "pam/psr-bridge": "0.1.0",
                        "pam/socket": "0.1.0",
                        "pam/testing": "0.1.0"
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
    };
    let entry = if template == InitTemplate::Laravel {
        "pam.php"
    } else {
        "index.php"
    };
    println!("Created Pam {preset} project in {}", directory.display());
    println!("Next: cd {} && pam dev {entry}", directory.display());
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
    println!("requests: {requests}");
    println!("successful: {successful}");
    println!("failed: {}", requests - successful);
    println!(
        "requests/sec: {:.2}",
        requests as f64 / elapsed.max(f64::EPSILON)
    );
    println!("latency p50: {:.3} ms", percentile(0.50));
    println!("latency p95: {:.3} ms", percentile(0.95));
    println!("latency p99: {:.3} ms", percentile(0.99));
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
