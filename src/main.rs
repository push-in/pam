use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod cluster;
mod commands;
mod composer;
mod control_plane;
mod desktop;
mod dev;
mod doctor;
mod mobile;
mod package_coordinates;
mod php;
mod sandbox;
mod server;
mod terminal;
mod worker_state;

const EX_USAGE: u8 = 64;
const EX_NOINPUT: u8 = 66;
const EX_SOFTWARE: u8 = 70;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let ui = terminal::Terminal::stderr();
            eprintln!("{} {}", ui.danger("× ERROR"), error);
            if matches!(error, CliError::Usage | CliError::Commands(_)) {
                eprintln!(
                    "{}",
                    ui.muted("  Run `pam --help` to see commands and examples.")
                );
            }
            ExitCode::from(error.exit_code())
        }
    }
}

fn run() -> Result<u8, CliError> {
    let mut raw_args = env::args_os();
    let executable = raw_args.next().unwrap_or_else(|| OsString::from("pam"));
    let Some(mut script_arg) = raw_args.next() else {
        print_usage(&executable);
        return Err(CliError::Usage);
    };

    let mut ini_entries = Vec::new();
    loop {
        let directive = if script_arg == "-d" {
            raw_args
                .next()
                .ok_or_else(|| CliError::Commands("-d requires name=value".to_owned()))?
        } else if script_arg.to_string_lossy().starts_with("-d")
            && script_arg.to_string_lossy().len() > 2
        {
            OsString::from(&script_arg.to_string_lossy()[2..])
        } else {
            break;
        };
        let directive = directive
            .into_string()
            .map_err(|_| CliError::Commands("-d requires valid UTF-8".to_owned()))?;
        if directive.is_empty()
            || directive.contains('\n')
            || directive.contains('\r')
            || directive.contains('\0')
        {
            return Err(CliError::Commands("invalid -d directive".to_owned()));
        }
        ini_entries.push(directive);
        script_arg = raw_args.next().ok_or_else(|| {
            CliError::Commands("a PHP script or -r is required after -d".to_owned())
        })?;
    }
    if !ini_entries.is_empty() {
        // SAFETY: CLI parsing happens before PHP or the Tokio runtime starts threads.
        unsafe { env::set_var("PAM_INI_ENTRIES", ini_entries.join("\n") + "\n") };
    }

    if script_arg == "--help" || script_arg == "-h" {
        print_usage(&executable);
        return Ok(0);
    }

    if script_arg == "help" {
        let command = raw_args.next();
        if let Some(command) = command {
            let command = command
                .into_string()
                .map_err(|_| CliError::Commands("help command must be valid UTF-8".to_owned()))?;
            if !terminal::print_command_help(&executable, &command) {
                return Err(CliError::Commands(format!(
                    "no focused help is available for {command:?}"
                )));
            }
        } else {
            print_usage(&executable);
        }
        return Ok(0);
    }

    if script_arg == "--version" || script_arg == "-V" {
        println!("pam {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }

    if script_arg == "-r" {
        let source = raw_args
            .next()
            .ok_or_else(|| CliError::Commands("-r requires PHP code".to_owned()))?;
        let source = source
            .into_string()
            .map_err(|_| CliError::Commands("-r code must be valid UTF-8".to_owned()))?;
        let logical_script = env::current_dir()
            .map_err(|error| {
                CliError::Commands(format!("cannot resolve current directory: {error}"))
            })?
            .join("Command line code");
        let arguments = raw_args.collect::<Vec<_>>();
        let mut php = php::PhpRuntime::initialize(&executable, &logical_script, &arguments)
            .map_err(CliError::Runtime)?;
        return php.execute_code(&source).map_err(CliError::Runtime);
    }

    if script_arg == "dev" {
        let dev_script = raw_args
            .next()
            .unwrap_or_else(|| OsString::from("index.php"));
        if dev_script == "--help" || dev_script == "-h" {
            print_dev_usage(&executable);
            return Ok(0);
        }

        let script = resolve_script(&dev_script)?;
        let script_args = raw_args.collect::<Vec<_>>();
        return dev::run(&script, &script_args).map_err(CliError::Dev);
    }

    if script_arg == "doctor" {
        let target = raw_args.next().unwrap_or_else(|| OsString::from("."));
        let target = resolve_target(&target)?;
        return doctor::run(&executable, &target).map_err(CliError::Doctor);
    }

    if script_arg == "mobile" {
        return mobile::run(raw_args.collect()).map_err(CliError::Commands);
    }

    if script_arg == "desktop" {
        return desktop::run(&executable, raw_args).map_err(CliError::Commands);
    }

    if script_arg == "start" {
        let mut options = cluster::StartOptions::parse(raw_args).map_err(CliError::Cluster)?;
        options.script = resolve_script(options.script.as_os_str())?;
        return cluster::run(&executable, options).map_err(CliError::Cluster);
    }

    if script_arg == "artisan" {
        let script = resolve_script(OsStr::new("artisan"))?;
        let arguments = raw_args.collect::<Vec<_>>();
        // SAFETY: Artisan owns this single-threaded PHP lifecycle and the runtime
        // has not started Tokio or any worker threads yet.
        unsafe {
            env::set_var("PAM_CLI_MODE", "1");
            env::set_var("APP_RUNNING_IN_CONSOLE", "true");
        }
        return run_script(&executable, &script, arguments);
    }

    if matches!(
        script_arg.to_string_lossy().as_ref(),
        "up" | "status"
            | "restart"
            | "stop"
            | "check-production"
            | "compatibility"
            | "health"
            | "leaks"
            | "capacity"
            | "deploy"
            | "remote"
            | "rollback"
            | "logs"
            | "workers"
            | "queues"
            | "scheduler"
            | "scale"
            | "nightwatch"
            | "autoscale"
            | "mcp"
            | "forge-script"
    ) {
        let command = script_arg.to_string_lossy();
        let remaining_arguments = raw_args.collect::<Vec<_>>();
        if remaining_arguments
            .first()
            .is_some_and(|argument| argument == "--help" || argument == "-h")
        {
            if terminal::print_command_help(&executable, command.as_ref()) {
                return Ok(0);
            }
            return Err(CliError::Commands(format!(
                "no focused help is available for {command:?}"
            )));
        }
        let mut arguments = if matches!(command.as_ref(), "up" | "status" | "restart" | "stop") {
            vec![
                OsString::from("pam:process"),
                OsString::from(command.as_ref()),
            ]
        } else if matches!(
            command.as_ref(),
            "rollback" | "logs" | "workers" | "queues" | "scheduler" | "scale"
        ) {
            vec![
                OsString::from("pam:remote"),
                OsString::from(command.as_ref()),
            ]
        } else {
            vec![OsString::from(format!("pam:{command}"))]
        };
        arguments.extend(remaining_arguments);
        let script = resolve_script(OsStr::new("artisan"))?;
        // SAFETY: Artisan owns this single-threaded PHP lifecycle before worker startup.
        unsafe {
            env::set_var("PAM_CLI_MODE", "1");
            env::set_var("APP_RUNNING_IN_CONSOLE", "true");
        }
        return run_script(&executable, &script, arguments);
    }

    if script_arg == "exec" {
        let script = raw_args
            .next()
            .ok_or_else(|| CliError::Commands("exec requires a PHP script".to_owned()))?;
        let script = resolve_script(&script)?;
        return run_script(&executable, &script, raw_args.collect());
    }

    if script_arg == "sandbox" {
        let manifest = raw_args.next().ok_or_else(|| {
            CliError::Commands("sandbox requires a capability manifest".to_owned())
        })?;
        if manifest == "--help" || manifest == "-h" {
            terminal::print_command_help(&executable, "sandbox");
            return Ok(0);
        }
        let separator = raw_args.next().ok_or_else(|| {
            CliError::Commands("sandbox requires `-- <script.php> [arguments...]`".to_owned())
        })?;
        if separator != "--" {
            return Err(CliError::Commands(
                "sandbox requires `--` before the PHP entry point".to_owned(),
            ));
        }
        let script = raw_args
            .next()
            .ok_or_else(|| CliError::Commands("sandbox requires a PHP entry point".to_owned()))?;
        let manifest = resolve_script(&manifest)?;
        let script = resolve_script(&script)?;
        let policy = sandbox::Policy::load(&manifest).map_err(CliError::Commands)?;
        policy.apply().map_err(CliError::Commands)?;
        return run_script(&executable, &script, raw_args.collect());
    }

    if script_arg == "routes" || script_arg == "inspect" {
        let script = commands::default_script(raw_args.next());
        let script = resolve_script(script.as_os_str())?;
        let arguments = raw_args.collect::<Vec<_>>();
        return if script_arg == "routes" {
            commands::routes(&executable, &script, &arguments).map_err(CliError::Commands)
        } else {
            commands::inspect(&executable, &script, &arguments).map_err(CliError::Commands)
        };
    }

    if script_arg == "test" {
        let mut arguments = raw_args.collect::<Vec<_>>();
        let target = if arguments
            .first()
            .is_some_and(|argument| !argument.to_string_lossy().starts_with('-'))
        {
            resolve_target(&arguments.remove(0))?
        } else {
            resolve_target(OsStr::new("."))?
        };
        return commands::test(&executable, &target, arguments).map_err(CliError::Commands);
    }

    if script_arg == "record" {
        let mut script = OsString::from("index.php");
        let mut output = PathBuf::from(".pam/recordings/latest.jsonl");
        let mut max_body_bytes = None;
        let mut max_bytes = None;
        let mut positional = false;
        let mut script_arguments = Vec::new();
        let mut passthrough = false;
        while let Some(argument) = raw_args.next() {
            if passthrough {
                script_arguments.push(argument);
                continue;
            }
            match argument.to_string_lossy().as_ref() {
                "--help" | "-h" => {
                    terminal::print_command_help(&executable, "record");
                    return Ok(0);
                }
                "--" => passthrough = true,
                "--output" => {
                    output = PathBuf::from(raw_args.next().ok_or_else(|| {
                        CliError::Commands("--output requires a JSONL file".to_owned())
                    })?);
                }
                "--max-body-bytes" => {
                    let value = raw_args.next().ok_or_else(|| {
                        CliError::Commands("--max-body-bytes requires an integer".to_owned())
                    })?;
                    let parsed = value.to_string_lossy().parse::<u64>().map_err(|_| {
                        CliError::Commands(
                            "--max-body-bytes requires a positive integer".to_owned(),
                        )
                    })?;
                    if parsed == 0 {
                        return Err(CliError::Commands(
                            "--max-body-bytes requires a positive integer".to_owned(),
                        ));
                    }
                    max_body_bytes = Some(value);
                }
                "--max-bytes" => {
                    let value = raw_args.next().ok_or_else(|| {
                        CliError::Commands("--max-bytes requires an integer".to_owned())
                    })?;
                    let parsed = value.to_string_lossy().parse::<u64>().map_err(|_| {
                        CliError::Commands("--max-bytes requires a positive integer".to_owned())
                    })?;
                    if parsed == 0 {
                        return Err(CliError::Commands(
                            "--max-bytes requires a positive integer".to_owned(),
                        ));
                    }
                    max_bytes = Some(value);
                }
                option if option.starts_with('-') => {
                    return Err(CliError::Commands(format!(
                        "unknown record option: {option}"
                    )));
                }
                _ if !positional => {
                    script = argument;
                    positional = true;
                }
                _ => {
                    return Err(CliError::Commands(
                        "put PHP script arguments after `--`".to_owned(),
                    ));
                }
            }
        }
        let script = resolve_script(&script)?;
        let recording =
            commands::prepare_recording(&output, &script).map_err(CliError::Commands)?;
        // SAFETY: recorder configuration is finalized before PHP or Tokio starts.
        unsafe {
            env::set_var("PAM_RECORD_PATH", &recording);
            if let Some(value) = max_body_bytes {
                env::set_var("PAM_RECORD_MAX_BODY_BYTES", value);
            }
            if let Some(value) = max_bytes {
                env::set_var("PAM_RECORD_MAX_BYTES", value);
            }
        }
        eprintln!("Pam flight recorder writing to {}", recording.display());
        return run_script(&executable, &script, script_arguments);
    }

    if script_arg == "replay" {
        let mut recording = None;
        let mut url = OsString::from("http://127.0.0.1:3000");
        let mut secrets = std::collections::BTreeMap::new();
        while let Some(argument) = raw_args.next() {
            match argument.to_string_lossy().as_ref() {
                "--help" | "-h" => {
                    terminal::print_command_help(&executable, "replay");
                    return Ok(0);
                }
                "--url" => {
                    url = raw_args.next().ok_or_else(|| {
                        CliError::Commands("--url requires an HTTP base URL".to_owned())
                    })?;
                }
                "--secret-env" => {
                    let mapping = raw_args.next().ok_or_else(|| {
                        CliError::Commands("--secret-env requires NAME=ENV_VAR".to_owned())
                    })?;
                    let mapping = mapping.to_string_lossy();
                    let (name, environment) = mapping.split_once('=').ok_or_else(|| {
                        CliError::Commands("--secret-env requires NAME=ENV_VAR".to_owned())
                    })?;
                    if name.is_empty() || environment.is_empty() {
                        return Err(CliError::Commands(
                            "--secret-env requires NAME=ENV_VAR".to_owned(),
                        ));
                    }
                    let value = env::var(environment).map_err(|_| {
                        CliError::Commands(format!(
                            "environment variable {environment:?} is not set"
                        ))
                    })?;
                    secrets.insert(name.to_owned(), value);
                }
                option if option.starts_with('-') => {
                    return Err(CliError::Commands(format!(
                        "unknown replay option: {option}"
                    )));
                }
                _ if recording.is_none() => recording = Some(argument),
                _ => {
                    return Err(CliError::Commands(
                        "replay accepts one recording file".to_owned(),
                    ));
                }
            }
        }
        let recording = recording
            .ok_or_else(|| CliError::Commands("replay requires a recording file".to_owned()))?;
        let recording = resolve_target(&recording)?;
        return commands::replay(&recording, &url.to_string_lossy(), &secrets)
            .map_err(CliError::Commands);
    }

    if matches!(
        script_arg.to_string_lossy().as_ref(),
        "diagnostics" | "heap" | "fibers" | "connections" | "profile" | "trace"
    ) {
        let command = script_arg.to_string_lossy().into_owned();
        if command == "profile" {
            // SAFETY: CLI parsing happens before PHP or the Tokio runtime starts any threads.
            unsafe { env::set_var("PAM_PROFILE", "1") };
        } else if command == "trace" {
            // SAFETY: CLI parsing happens before PHP or the Tokio runtime starts any threads.
            unsafe { env::set_var("PAM_TRACE", "1") };
        } else {
            // SAFETY: CLI parsing happens before PHP or the Tokio runtime starts any threads.
            unsafe { env::set_var("PAM_DIAGNOSTICS", "1") };
        }
        let script = commands::default_script(raw_args.next());
        let script = resolve_script(script.as_os_str())?;
        let arguments = raw_args.collect::<Vec<_>>();
        let section = match command.as_str() {
            "heap" => Some("memory"),
            "fibers" => Some("fibers"),
            "connections" => Some("connections"),
            _ => None,
        };
        return commands::diagnostics(&executable, &script, &arguments, section)
            .map_err(CliError::Commands);
    }

    if script_arg == "top" {
        let address = raw_args
            .next()
            .unwrap_or_else(|| OsString::from("http://127.0.0.1:3010"));
        let mut iterations = 10_usize;
        let mut interval_ms = 1000_u64;
        while let Some(option) = raw_args.next() {
            let value = raw_args.next().ok_or_else(|| {
                CliError::Commands(format!("{} requires a value", option.to_string_lossy()))
            })?;
            match option.to_string_lossy().as_ref() {
                "--iterations" => {
                    iterations = value.to_string_lossy().parse().map_err(|_| {
                        CliError::Commands("--iterations requires a positive integer".to_owned())
                    })?;
                }
                "--interval-ms" => {
                    interval_ms = value.to_string_lossy().parse().map_err(|_| {
                        CliError::Commands("--interval-ms requires a positive integer".to_owned())
                    })?;
                }
                unknown => {
                    return Err(CliError::Commands(format!("unknown top option: {unknown}")));
                }
            }
        }
        return commands::top(
            &address.to_string_lossy(),
            iterations,
            std::time::Duration::from_millis(interval_ms),
        )
        .map_err(CliError::Commands);
    }

    if script_arg == "init" {
        let mut directory = PathBuf::from(".");
        let mut positional = false;
        let mut template = None;
        let mut socket = false;
        let mut install = true;
        let mut interaction = true;
        while let Some(argument) = raw_args.next() {
            match argument.to_string_lossy().as_ref() {
                "--help" | "-h" => {
                    print_init_usage(&executable);
                    return Ok(0);
                }
                "--template" => {
                    let value = raw_args.next().ok_or_else(|| {
                        CliError::Commands(
                            "--template requires raw, api, laravel, desktop, mobile, or mobile-ui"
                                .to_owned(),
                        )
                    })?;
                    template = Some(
                        commands::InitTemplate::parse(&value.to_string_lossy())
                            .map_err(CliError::Commands)?,
                    );
                }
                "--socket" => socket = true,
                "--no-install" => install = false,
                "--no-interaction" => interaction = false,
                option if option.starts_with('-') => {
                    return Err(CliError::Commands(format!("unknown init option: {option}")));
                }
                _ if !positional => {
                    directory = PathBuf::from(argument);
                    positional = true;
                }
                _ => {
                    return Err(CliError::Commands(
                        "init accepts at most one project directory".to_owned(),
                    ));
                }
            }
        }
        return commands::init(
            &executable,
            commands::InitOptions {
                directory,
                template,
                socket,
                install,
                interaction,
            },
        )
        .map_err(CliError::Commands);
    }

    if script_arg == "composer" {
        let arguments = raw_args.collect::<Vec<_>>();
        return composer::run(&executable, &arguments).map_err(CliError::Commands);
    }

    if script_arg == "build" {
        let mut target = OsString::from(".");
        let mut output = OsString::from("dist");
        let mut entry = OsString::from("index.php");
        let mut signing_key = None;
        let mut positional = false;
        while let Some(argument) = raw_args.next() {
            match argument.to_string_lossy().as_ref() {
                "--output" => {
                    output = raw_args.next().ok_or_else(|| {
                        CliError::Commands("--output requires a directory".to_owned())
                    })?;
                }
                "--entry" => {
                    entry = raw_args.next().ok_or_else(|| {
                        CliError::Commands("--entry requires a PHP file".to_owned())
                    })?;
                }
                "--signing-key" => {
                    signing_key = Some(raw_args.next().ok_or_else(|| {
                        CliError::Commands("--signing-key requires an Ed25519 key".to_owned())
                    })?);
                }
                option if option.starts_with('-') => {
                    return Err(CliError::Commands(format!(
                        "unknown build option: {option}"
                    )));
                }
                _ if !positional => {
                    target = argument;
                    positional = true;
                }
                _ => {
                    return Err(CliError::Commands(
                        "build accepts at most one project directory".to_owned(),
                    ));
                }
            }
        }
        let target = resolve_target(&target)?;
        let output = if Path::new(&output).is_absolute() {
            PathBuf::from(output)
        } else {
            target.join(output)
        };
        let signing_key = signing_key.as_deref().map(resolve_script).transpose()?;
        return commands::build(&target, &output, Path::new(&entry), signing_key.as_deref())
            .map_err(CliError::Commands);
    }

    if script_arg == "verify" {
        let mut target = OsString::from("dist");
        let mut positional = false;
        let mut public_key = None;
        let mut require_signature = false;
        while let Some(argument) = raw_args.next() {
            match argument.to_string_lossy().as_ref() {
                "--help" | "-h" => {
                    terminal::print_command_help(&executable, "verify");
                    return Ok(0);
                }
                "--public-key" => {
                    public_key = Some(raw_args.next().ok_or_else(|| {
                        CliError::Commands("--public-key requires an Ed25519 key".to_owned())
                    })?);
                }
                "--require-signature" => require_signature = true,
                option if option.starts_with('-') => {
                    return Err(CliError::Commands(format!(
                        "unknown verify option: {option}"
                    )));
                }
                _ if !positional => {
                    target = argument;
                    positional = true;
                }
                _ => {
                    return Err(CliError::Commands(
                        "verify accepts one bundle directory".to_owned(),
                    ));
                }
            }
        }
        let target = resolve_target(&target)?;
        let public_key = public_key.as_deref().map(resolve_script).transpose()?;
        return commands::verify_bundle(&target, public_key.as_deref(), require_signature)
            .map_err(CliError::Commands);
    }

    if script_arg == "benchmark" {
        let url = raw_args
            .next()
            .ok_or_else(|| CliError::Commands("benchmark requires an HTTP URL".to_owned()))?;
        let (requests, concurrency) = benchmark_options(raw_args).map_err(CliError::Commands)?;
        return commands::benchmark(&url.to_string_lossy(), requests, concurrency)
            .map_err(CliError::Commands);
    }

    if script_arg == "__worker" {
        let worker_script = raw_args
            .next()
            .ok_or_else(|| CliError::Cluster("worker script is missing".to_owned()))?;
        let script = resolve_script(&worker_script)?;
        return run_script(&executable, &script, raw_args.collect());
    }

    let script = resolve_script(&script_arg)?;
    let script_args = raw_args.collect::<Vec<_>>();
    run_script(&executable, &script, script_args)
}

fn run_script(
    executable: &OsStr,
    script: &Path,
    script_args: Vec<OsString>,
) -> Result<u8, CliError> {
    let mut php =
        php::PhpRuntime::initialize(executable, script, &script_args).map_err(CliError::Runtime)?;
    let status = php.execute_file(script).map_err(CliError::Runtime)?;

    if status != 0 {
        return Ok(status);
    }

    if let Some(config) = php.server_config().map_err(CliError::Runtime)? {
        server::run(config).map_err(CliError::Server)?;
    }

    Ok(0)
}

fn resolve_script(script: &OsStr) -> Result<PathBuf, CliError> {
    let path = Path::new(script);
    let metadata = fs::metadata(path).map_err(|source| CliError::ScriptUnavailable {
        path: path.to_path_buf(),
        source,
    })?;

    if !metadata.is_file() {
        return Err(CliError::NotAFile(path.to_path_buf()));
    }

    fs::canonicalize(path).map_err(|source| CliError::ScriptUnavailable {
        path: path.to_path_buf(),
        source,
    })
}

fn resolve_target(target: &OsStr) -> Result<PathBuf, CliError> {
    let path = Path::new(target);
    fs::canonicalize(path).map_err(|source| CliError::ScriptUnavailable {
        path: path.to_path_buf(),
        source,
    })
}

fn benchmark_options(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<(usize, usize), String> {
    let mut requests = 100;
    let mut concurrency = 10;
    while let Some(option) = arguments.next() {
        let value = arguments
            .next()
            .ok_or_else(|| format!("{} requires a value", option.to_string_lossy()))?;
        let value = value
            .to_string_lossy()
            .parse::<usize>()
            .map_err(|_| format!("{} requires a positive integer", option.to_string_lossy()))?;
        if value == 0 {
            return Err(format!(
                "{} requires a positive integer",
                option.to_string_lossy()
            ));
        }
        match option.to_string_lossy().as_ref() {
            "--requests" => requests = value,
            "--concurrency" => concurrency = value,
            unknown => return Err(format!("unknown benchmark option: {unknown}")),
        }
    }
    Ok((requests, concurrency))
}

fn print_usage(executable: &OsStr) {
    terminal::print_help(executable);
}

fn print_init_usage(executable: &OsStr) {
    terminal::print_command_help(executable, "init");
}

fn print_dev_usage(executable: &OsStr) {
    terminal::print_command_help(executable, "dev");
}

#[derive(Debug)]
enum CliError {
    Usage,
    ScriptUnavailable {
        path: PathBuf,
        source: std::io::Error,
    },
    NotAFile(PathBuf),
    Dev(String),
    Cluster(String),
    Commands(String),
    Doctor(String),
    Runtime(String),
    Server(server::ServerError),
}

impl CliError {
    fn exit_code(&self) -> u8 {
        match self {
            Self::Usage => EX_USAGE,
            Self::ScriptUnavailable { .. } | Self::NotAFile(_) => EX_NOINPUT,
            Self::Dev(_)
            | Self::Cluster(_)
            | Self::Commands(_)
            | Self::Doctor(_)
            | Self::Runtime(_)
            | Self::Server(_) => EX_SOFTWARE,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage => write!(formatter, "a PHP script is required"),
            Self::ScriptUnavailable { path, source } => {
                write!(formatter, "cannot open {}: {source}", path.display())
            }
            Self::NotAFile(path) => write!(formatter, "{} is not a file", path.display()),
            Self::Dev(error) => formatter.write_str(error),
            Self::Cluster(error) => formatter.write_str(error),
            Self::Commands(error) => formatter.write_str(error),
            Self::Doctor(error) => formatter.write_str(error),
            Self::Runtime(error) => formatter.write_str(error),
            Self::Server(error) => error.fmt(formatter),
        }
    }
}
