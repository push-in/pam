use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod admin_auth;
mod catalog;
mod cluster;
mod commands;
mod composer;
mod control_plane;
mod desktop;
mod desktop_transaction;
mod dev;
mod dev_event;
mod distribution;
mod doctor;
mod doctor_contract;
mod ecosystem;
mod editor;
mod ingress;
mod manager_dashboard;
mod mobile;
mod octane;
mod otlp;
mod php;
mod plugin_registry;
mod process_manager;
mod project;
mod prometheus;
mod protocol;
mod quality;
mod resource_monitor;
mod self_update;
mod server;
mod ship;
mod support;
mod terminal;
mod timeline;
mod traffic;
mod worker_state;

const EX_NOINPUT: u8 = 66;
const EX_SOFTWARE: u8 = 70;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if json_errors_requested() {
                println!("{}", error.json());
                return ExitCode::from(error.exit_code());
            }
            let ui = terminal::Terminal::stderr();
            eprintln!(
                "{} {} {}",
                ui.danger("× ERROR"),
                ui.muted(format!("[PAM-E{:03}]", error.code() as u8)),
                error
            );
            eprintln!("{}", ui.muted(format!("  Fix: {}", error.remediation())));
            eprintln!(
                "{}",
                ui.muted(format!("  Verify: {}", error.verification_command()))
            );
            ExitCode::from(error.exit_code())
        }
    }
}

fn json_errors_requested() -> bool {
    env::args_os()
        .nth(1)
        .is_some_and(|argument| argument == "--json-errors")
        || env::var("PAM_ERROR_FORMAT").is_ok_and(|value| value == "json")
}

fn run() -> Result<u8, CliError> {
    let mut raw_args = env::args_os().peekable();
    let executable = raw_args.next().unwrap_or_else(|| OsString::from("pam"));
    let Some(mut script_arg) = raw_args.next() else {
        if terminal::Terminal::stdout().interactive() {
            return terminal::launcher(&executable).map_err(CliError::Commands);
        }
        print_usage(&executable);
        return Ok(0);
    };
    if script_arg == "--json-errors" {
        script_arg = raw_args.next().ok_or_else(|| {
            CliError::Commands("--json-errors requires a command or PHP script".to_owned())
        })?;
    }

    let mut ini_entries = Vec::new();
    let mut ini_file = None::<OsString>;
    loop {
        if script_arg == "-c" {
            ini_file = Some(
                raw_args
                    .next()
                    .ok_or_else(|| CliError::Commands("-c requires a php.ini path".to_owned()))?,
            );
            script_arg = raw_args.next().ok_or_else(|| {
                CliError::Commands("a PHP script or -r is required after -c".to_owned())
            })?;
            continue;
        }
        if script_arg.to_string_lossy().starts_with("-c") && script_arg.to_string_lossy().len() > 2
        {
            ini_file = Some(OsString::from(&script_arg.to_string_lossy()[2..]));
            script_arg = raw_args.next().ok_or_else(|| {
                CliError::Commands("a PHP script or -r is required after -c".to_owned())
            })?;
            continue;
        }
        let directive = if script_arg == "-d" || script_arg == "--define" {
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
    if let Some(ini_file) = ini_file {
        let ini_file = ini_file
            .into_string()
            .map_err(|_| CliError::Commands("-c path must be valid UTF-8".to_owned()))?;
        if ini_file.is_empty() || ini_file.contains(['\n', '\r', '\0']) {
            return Err(CliError::Commands("invalid -c php.ini path".to_owned()));
        }
        // SAFETY: CLI parsing occurs before PHP or worker threads are initialized.
        unsafe { env::set_var("PHPRC", ini_file) };
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

    if script_arg == "completion" {
        let shell = raw_args.next().ok_or_else(|| {
            CliError::Commands("completion requires bash, zsh, fish, or powershell".to_owned())
        })?;
        if raw_args.next().is_some() {
            return Err(CliError::Commands(
                "completion accepts one shell".to_owned(),
            ));
        }
        print!(
            "{}",
            catalog::completion(&shell.to_string_lossy()).map_err(CliError::Commands)?
        );
        return Ok(0);
    }

    if script_arg == "catalog" {
        let arguments = raw_args.collect::<Vec<_>>();
        if arguments.as_slice() == ["--json"] {
            println!("{}", catalog::json());
            return Ok(0);
        }
        if arguments.as_slice() == ["--schema"] {
            print!("{}", catalog::schema());
            return Ok(0);
        }
        if arguments.as_slice() == ["--compat-schema"] {
            print!("{}", catalog::compatibility_schema());
            return Ok(0);
        }
        if matches!(arguments.first(), Some(option) if option == "--validate")
            && matches!(arguments.len(), 2 | 3)
            && (arguments.len() == 2 || arguments[2] == "--json")
        {
            let path = PathBuf::from(&arguments[1]);
            let command_count = catalog::validate_file(&path).map_err(CliError::Commands)?;
            if arguments.len() == 3 {
                println!(
                    "{}",
                    serde_json::json!({
                        "schemaVersion": 1,
                        "valid": true,
                        "commandCount": command_count,
                    })
                );
            } else {
                println!(
                    "CLI catalog valid (schema 1, {command_count} commands): {}",
                    path.display()
                );
            }
            return Ok(0);
        }
        if matches!(arguments.first(), Some(option) if option == "--compat")
            && matches!(arguments.len(), 3 | 4)
            && (arguments.len() == 3 || arguments[3] == "--json")
        {
            let report = catalog::compare_files(
                &PathBuf::from(&arguments[1]),
                &PathBuf::from(&arguments[2]),
            )
            .map_err(CliError::Commands)?;
            let compatible = report.compatible();
            if arguments.len() == 4 {
                println!("{}", report.json());
            } else if compatible {
                println!(
                    "CLI catalogs compatible ({} baseline, {} candidate commands)",
                    report.baseline_command_count, report.candidate_command_count
                );
            } else {
                println!("CLI catalogs incompatible:");
                for change in &report.changes {
                    println!(
                        "  changeCode={} command={}",
                        change.change_code as u8, change.command
                    );
                }
            }
            return Ok(if compatible { 0 } else { 1 });
        }
        return Err(CliError::Commands(
            "catalog requires `--json`, `--schema`, `--compat-schema`, `--validate FILE [--json]`, or `--compat BASELINE CANDIDATE [--json]`".to_owned(),
        ));
    }

    if script_arg == "editor:install" {
        return editor::install(raw_args).map_err(CliError::Commands);
    }

    if script_arg == "clean" {
        let mut all = false;
        let mut dry_run = false;
        let mut json = false;
        let mut target = None;
        for argument in raw_args {
            match argument.to_string_lossy().as_ref() {
                "--all" => all = true,
                "--dry-run" => dry_run = true,
                "--json" => json = true,
                option if option.starts_with('-') => {
                    return Err(CliError::Commands(format!(
                        "unknown clean option: {option}"
                    )));
                }
                _ if target.is_none() => target = Some(argument),
                _ => {
                    return Err(CliError::Commands(
                        "clean accepts at most one project path".to_owned(),
                    ));
                }
            }
        }
        let target = resolve_target(&target.unwrap_or_else(|| OsString::from(".")))?;
        let context = project::discover_cleanable(&target).ok_or_else(|| {
            CliError::Commands("`pam clean` must target a PAM project or Rust workspace".to_owned())
        })?;
        return project::clean(&context, all, dry_run, json).map_err(CliError::Commands);
    }

    if script_arg == "self-update" {
        return self_update::run(raw_args).map_err(CliError::Commands);
    }

    if script_arg == "support" {
        return support::run(&executable, raw_args).map_err(CliError::Commands);
    }

    if script_arg == "docs:generate" {
        let mut check = false;
        let mut path = PathBuf::from("docs/cli-reference.md");
        for argument in raw_args {
            if argument == "--check" {
                check = true;
            } else if path == Path::new("docs/cli-reference.md") {
                path = PathBuf::from(argument);
            } else {
                return Err(CliError::Commands(
                    "docs:generate accepts one output path".to_owned(),
                ));
            }
        }
        return catalog::write_reference(&path, check).map_err(CliError::Commands);
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

    if script_arg == "new" {
        script_arg = OsString::from("init");
    }

    if script_arg == "dev" {
        let cleanable_context = current_project().or_else(|| {
            env::current_dir()
                .ok()
                .and_then(|directory| project::discover_cleanable(&directory))
        });
        let artifact_budget = if let Some(context) = cleanable_context.as_ref() {
            let budget = project::dev_artifact_budget().map_err(CliError::Commands)?;
            project::enforce_dev_artifact_budget(context, budget).map_err(CliError::Commands)?;
            Some(budget)
        } else {
            None
        };
        if let Some(context) = current_project()
            && context.kind == project::ProjectKind::Native
        {
            if project::native_platforms(&context).map_err(CliError::Commands)? == [2] {
                let mut arguments = vec![
                    OsString::from("ios:dev"),
                    context.root.clone().into_os_string(),
                ];
                arguments.extend(raw_args);
                let outcome = mobile::run(arguments).map_err(CliError::Commands);
                return finish_dev_with_artifact_budget(Some(&context), artifact_budget, outcome);
            }
            let mut arguments = vec![OsString::from("dev"), context.root.clone().into_os_string()];
            arguments.extend(raw_args);
            let outcome = mobile::run(arguments).map_err(CliError::Commands);
            return finish_dev_with_artifact_budget(Some(&context), artifact_budget, outcome);
        }
        if let Some(context) = current_project()
            && context.kind == project::ProjectKind::Desktop
        {
            let mut arguments = vec![OsString::from("dev"), context.root.clone().into_os_string()];
            arguments.extend(raw_args);
            let outcome = desktop::run(&executable, arguments).map_err(CliError::Commands);
            return finish_dev_with_artifact_budget(Some(&context), artifact_budget, outcome);
        }
        let dev_script = raw_args
            .next()
            .unwrap_or_else(|| OsString::from("index.php"));
        if dev_script == "--help" || dev_script == "-h" {
            print_dev_usage(&executable);
            return Ok(0);
        }

        let script = resolve_script(&dev_script)?;
        let script_args = raw_args.collect::<Vec<_>>();
        let outcome = dev::run(&script, &script_args).map_err(CliError::Dev);
        return finish_dev_with_artifact_budget(
            cleanable_context.as_ref(),
            artifact_budget,
            outcome,
        );
    }

    if script_arg == "doctor" {
        let mut fix = false;
        let mut ci = false;
        let mut json = false;
        let mut schema = false;
        let mut validate = None;
        let mut target = None;
        while let Some(argument) = raw_args.next() {
            match argument.to_string_lossy().as_ref() {
                "--fix" => fix = true,
                "--ci" => ci = true,
                "--json" => json = true,
                "--schema" => schema = true,
                "--validate" => {
                    if validate.is_some() {
                        return Err(CliError::Commands(
                            "doctor --validate accepts exactly one report path".to_owned(),
                        ));
                    }
                    validate = Some(raw_args.next().ok_or_else(|| {
                        CliError::Commands("doctor --validate requires a report path".to_owned())
                    })?);
                }
                option if option.starts_with('-') => {
                    return Err(CliError::Commands(format!(
                        "unknown doctor option: {option}"
                    )));
                }
                _ if target.is_none() => target = Some(argument),
                _ => {
                    return Err(CliError::Commands(
                        "doctor accepts at most one project path".to_owned(),
                    ));
                }
            }
        }
        if schema {
            if fix || ci || json || validate.is_some() || target.is_some() {
                return Err(CliError::Commands(
                    "doctor --schema must be used alone".to_owned(),
                ));
            }
            let document: serde_json::Value =
                serde_json::from_str(include_str!("../docs/schemas/doctor-report.schema.json"))
                    .map_err(|error| {
                        CliError::Commands(format!("invalid embedded doctor schema: {error}"))
                    })?;
            println!(
                "{}",
                serde_json::to_string_pretty(&document)
                    .map_err(|error| CliError::Commands(error.to_string()))?
            );
            return Ok(0);
        }
        if let Some(report) = validate {
            if fix || ci || json || target.is_some() {
                return Err(CliError::Commands(
                    "doctor --validate must be used with exactly one report path".to_owned(),
                ));
            }
            let report = PathBuf::from(report);
            doctor_contract::validate_file(&report).map_err(CliError::Commands)?;
            println!("Doctor report valid (schema 1): {}", report.display());
            return Ok(0);
        }
        if ci {
            // SAFETY: command parsing occurs before any runtime worker threads exist.
            unsafe { env::set_var("PAM_COLOR", "never") };
        }
        let target = target.unwrap_or_else(|| OsString::from("."));
        let target = resolve_target(&target)?;
        if json {
            if fix {
                return Err(CliError::Commands(
                    "doctor --json cannot be combined with --fix; repair first, then audit"
                        .to_owned(),
                ));
            }
            let output = std::process::Command::new(&executable)
                .args(["doctor", "--ci"])
                .arg(&target)
                .output()
                .map_err(|error| {
                    CliError::Commands(format!("cannot run structured doctor audit: {error}"))
                })?;
            let context = project::discover(&target);
            let project = context
                .as_ref()
                .map(project::diagnostic_context)
                .transpose()
                .map_err(CliError::Commands)?;
            let healthy = output.status.success();
            let next_actions = if healthy && context.is_some() {
                serde_json::json!([
                    {
                        "actionCode": 1,
                        "summary": "Start the contextual development session",
                        "command": "pam dev",
                        "arguments": ["dev"],
                        "verificationCommand": "pam doctor --json"
                    }
                ])
            } else if healthy {
                let target_argument = target.to_string_lossy();
                serde_json::json!([
                    {
                        "actionCode": 1,
                        "summary": "Run the verified PHP target",
                        "command": format!("pam {}", target.display()),
                        "arguments": [target_argument],
                        "verificationCommand": format!("pam doctor {} --json", target.display())
                    }
                ])
            } else if context.is_some() {
                serde_json::json!([
                    {
                        "actionCode": 2,
                        "summary": "Repair the active project",
                        "command": "pam doctor --fix",
                        "arguments": ["doctor", "--fix"],
                        "verificationCommand": "pam doctor --json"
                    }
                ])
            } else {
                let target_argument = target.to_string_lossy();
                serde_json::json!([
                    {
                        "actionCode": 3,
                        "summary": "Inspect the target diagnostics",
                        "command": format!("pam doctor {}", target.display()),
                        "arguments": ["doctor", target_argument],
                        "verificationCommand": format!("pam doctor {} --json", target.display())
                    }
                ])
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema": 1,
                    "schemaVersion": 1,
                    "resultCode": if healthy { 1 } else { 2 },
                    "healthy": healthy,
                    "exitCode": output.status.code().unwrap_or(1),
                    "target": target,
                    "root": context.as_ref().map(|context| &context.root),
                    "projectType": context.as_ref().map(|context| context.kind as u8),
                    "project": project,
                    "nextActions": next_actions,
                    "diagnostics": String::from_utf8_lossy(&output.stdout).trim_end(),
                    "errors": String::from_utf8_lossy(&output.stderr).trim_end(),
                }))
                .map_err(|error| CliError::Commands(error.to_string()))?
            );
            return Ok(if output.status.success() { 0 } else { 1 });
        }
        let context = project::discover(&target);
        if fix {
            let context = context.as_ref().ok_or_else(|| {
                CliError::Commands(
                    "`pam doctor --fix` requires a recognizable PAM project".to_owned(),
                )
            })?;
            if project::ensure_manifest(context).map_err(CliError::Commands)? {
                println!("Created {}", context.root.join("pam.json").display());
            }
            if ecosystem::repair_dependencies(&executable, &context.root)
                .map_err(CliError::Commands)?
            {
                println!("Installed the project's locked Composer dependencies.");
            }
            if context.kind == project::ProjectKind::Native {
                if project::native_platforms(context).map_err(CliError::Commands)? != [2] {
                    mobile::repair_android(&context.root).map_err(CliError::Commands)?;
                }
                ecosystem::refresh_native(&executable, &context.root)
                    .map_err(CliError::Commands)?;
                println!("Regenerated PAM Native bindings and plugin integration.");
            }
        }
        if let Some(context) = context.as_ref() {
            project::validate_context(context).map_err(CliError::Commands)?;
        }
        if let Some(context) = context.as_ref()
            && context.kind == project::ProjectKind::Native
        {
            if project::native_platforms(context).map_err(CliError::Commands)? == [2] {
                return mobile::run(vec![
                    OsString::from("ios:doctor"),
                    context.root.clone().into_os_string(),
                ])
                .map_err(CliError::Commands);
            }
            return mobile::run(vec![
                OsString::from("doctor"),
                context.root.clone().into_os_string(),
            ])
            .map_err(CliError::Commands);
        }
        if let Some(context) = context.as_ref()
            && context.kind == project::ProjectKind::Desktop
        {
            return desktop::run(
                &executable,
                vec![
                    OsString::from("doctor"),
                    context.root.clone().into_os_string(),
                ],
            )
            .map_err(CliError::Commands);
        }
        return doctor::run(&executable, &target).map_err(CliError::Doctor);
    }

    if script_arg == "mobile" {
        return mobile::run(raw_args.collect()).map_err(CliError::Commands);
    }

    if script_arg == "desktop" {
        return desktop::run(&executable, raw_args).map_err(CliError::Commands);
    }

    if script_arg == "registry" {
        return plugin_registry::run(raw_args.collect()).map_err(CliError::Commands);
    }

    let managed_logs = script_arg == "logs"
        && raw_args
            .peek()
            .is_some_and(|argument| !argument.to_string_lossy().starts_with('-'));
    if matches!(
        script_arg.to_string_lossy().as_ref(),
        "up" | "ps"
            | "status"
            | "describe"
            | "reload"
            | "restart"
            | "scale"
            | "stop"
            | "delete"
            | "save"
            | "resurrect"
            | "startup"
            | "monit"
            | "dashboard"
            | "apply"
            | "config:check"
            | "deploy"
            | "deploy:history"
            | "rollback"
            | "traffic:start"
            | "traffic:set"
            | "traffic:promote"
            | "traffic:abort"
            | "traffic:status"
            | "traffic:evaluate"
            | "traffic:stop"
            | "daemon"
            | "__pamd"
            | "__manager_local"
            | "__traffic_proxy"
    ) || managed_logs
    {
        let command = script_arg.to_string_lossy();
        return process_manager::run(&executable, &command, raw_args).map_err(CliError::Commands);
    }

    if script_arg == "timeline" {
        return timeline::run(raw_args).map_err(CliError::Commands);
    }

    if script_arg == "start" {
        let mut options = cluster::StartOptions::parse(raw_args).map_err(CliError::Cluster)?;
        options.script = resolve_script(options.script.as_os_str())?;
        enable_server_opcache();
        if options.script.file_name() == Some(OsStr::new("artisan")) {
            // SAFETY: the master has not initialized PHP or started worker
            // threads. Children inherit the console identity before booting
            // Laravel, just like `pam artisan`.
            unsafe {
                env::set_var("PAM_CLI_MODE", "1");
                env::set_var("APP_RUNNING_IN_CONSOLE", "true");
            }
        }
        return cluster::run(&executable, options).map_err(CliError::Cluster);
    }

    if script_arg == "artisan" {
        let script = resolve_script(OsStr::new("artisan"))?;
        let arguments = raw_args.collect::<Vec<_>>();
        if arguments.first() == Some(&OsString::from("pam:octane")) {
            enable_server_opcache();
        }
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
        "octane:start" | "octane:status" | "octane:reload" | "octane:stop"
    ) {
        let context = current_project().ok_or_else(|| {
            CliError::Commands("PAM Octane commands must run inside a Laravel project".to_owned())
        })?;
        if context.kind != project::ProjectKind::Laravel {
            return Err(CliError::Commands(format!(
                "PAM Octane requires a Laravel project; found {}",
                context.kind.label()
            )));
        }
        return match script_arg.to_string_lossy().as_ref() {
            "octane:start" => {
                let script = resolve_script(context.root.join("artisan").as_os_str())?;
                enable_server_opcache();
                // SAFETY: workers inherit this identity before PHP starts.
                unsafe {
                    env::set_var("PAM_CLI_MODE", "1");
                    env::set_var("APP_RUNNING_IN_CONSOLE", "true");
                }
                octane::start(&executable, script, &context.root, raw_args)
                    .map_err(CliError::Cluster)
            }
            "octane:status" => octane::status(&context.root).map_err(CliError::Cluster),
            "octane:reload" => octane::reload(&context.root).map_err(CliError::Cluster),
            "octane:stop" => octane::stop(&context.root).map_err(CliError::Cluster),
            _ => unreachable!(),
        };
    }

    if script_arg == "exec" {
        let script = raw_args
            .next()
            .ok_or_else(|| CliError::Commands("exec requires a PHP script".to_owned()))?;
        let script = resolve_script(&script)?;
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
        if let Some(context) = current_project()
            && context.kind == project::ProjectKind::Desktop
            && !context.root.join("vendor/bin/pest").is_file()
            && !context.root.join("vendor/bin/phpunit").is_file()
        {
            if !arguments.is_empty() {
                return Err(CliError::Commands(
                    "this desktop project does not have a configured PHP test runner".to_owned(),
                ));
            }
            println!(
                "No application test runner is configured; PAM Desktop shell contracts are validated by its release artifact."
            );
            return Ok(0);
        }
        if let Some(context) = current_project()
            && context.kind == project::ProjectKind::Raw
            && !context.root.join("composer.json").is_file()
        {
            if !arguments.is_empty() {
                return Err(CliError::Commands(
                    "raw projects without Composer do not have a configured test runner".to_owned(),
                ));
            }
            println!(
                "No test runner is configured for this raw PAM project; runtime contracts are covered by `pam doctor`."
            );
            return Ok(0);
        }
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

    if matches!(
        script_arg.to_string_lossy().as_ref(),
        "diagnostics" | "heap" | "fibers" | "connections" | "profile" | "trace"
    ) {
        let command = script_arg.to_string_lossy().into_owned();
        if command == "diagnostics"
            && let Some(context) = current_project()
            && context.kind == project::ProjectKind::Native
        {
            let diagnostics_command =
                if project::native_platforms(&context).map_err(CliError::Commands)? == [2] {
                    "ios:diagnostics"
                } else {
                    "diagnostics"
                };
            let mut arguments = vec![
                OsString::from(diagnostics_command),
                context.root.into_os_string(),
            ];
            arguments.extend(raw_args);
            return mobile::run(arguments).map_err(CliError::Commands);
        }
        if command == "diagnostics"
            && let Some(context) = current_project()
            && context.kind == project::ProjectKind::Desktop
        {
            let mut arguments = vec![OsString::from("diagnostics"), context.root.into_os_string()];
            arguments.extend(raw_args);
            return desktop::run(&executable, arguments).map_err(CliError::Commands);
        }
        if command == "profile"
            && let Some(context) = current_project()
            && context.kind == project::ProjectKind::Native
        {
            let mut arguments = vec![OsString::from("profile"), context.root.into_os_string()];
            arguments.extend(raw_args);
            return mobile::run(arguments).map_err(CliError::Commands);
        }
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
        let mut address = OsString::from("http://127.0.0.1:3010");
        let mut address_set = false;
        let mut iterations = 10_usize;
        let mut interval_ms = 1000_u64;
        let mut lag_warn_ms = 10_u64;
        let mut json = false;
        while let Some(option) = raw_args.next() {
            if option == "--json" {
                json = true;
                continue;
            }
            if !option.to_string_lossy().starts_with('-') {
                if address_set {
                    return Err(CliError::Commands(
                        "top accepts only one admin URL".to_owned(),
                    ));
                }
                address = option;
                address_set = true;
                continue;
            }
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
                "--lag-warn-ms" => {
                    lag_warn_ms = value.to_string_lossy().parse().map_err(|_| {
                        CliError::Commands("--lag-warn-ms requires a positive integer".to_owned())
                    })?;
                    if lag_warn_ms == 0 || lag_warn_ms > 60_000 {
                        return Err(CliError::Commands(
                            "--lag-warn-ms must be between 1 and 60000".to_owned(),
                        ));
                    }
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
            std::time::Duration::from_millis(lag_warn_ms),
            json,
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
        let mut application_id = None;
        let mut application_name = None;
        let mut mobile_starter = None;
        let mut mobile_platforms = Vec::new();
        while let Some(argument) = raw_args.next() {
            match argument.to_string_lossy().as_ref() {
                "--help" | "-h" => {
                    print_init_usage(&executable);
                    return Ok(0);
                }
                "--template" => {
                    let value = raw_args.next().ok_or_else(|| {
                        CliError::Commands(
                            "--template requires raw, api, laravel, desktop, mobile, mobile-ui, or product"
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
                "--application-id" => {
                    application_id = Some(
                        raw_args
                            .next()
                            .ok_or_else(|| {
                                CliError::Commands("--application-id requires a value".to_owned())
                            })?
                            .into_string()
                            .map_err(|_| {
                                CliError::Commands("application ID must be valid UTF-8".to_owned())
                            })?,
                    );
                }
                "--name" => {
                    application_name = Some(
                        raw_args
                            .next()
                            .ok_or_else(|| {
                                CliError::Commands("--name requires a value".to_owned())
                            })?
                            .into_string()
                            .map_err(|_| {
                                CliError::Commands(
                                    "application name must be valid UTF-8".to_owned(),
                                )
                            })?,
                    );
                }
                "--starter" => {
                    let value = raw_args.next().ok_or_else(|| {
                        CliError::Commands("--starter requires a preset".to_owned())
                    })?;
                    mobile_starter = Some(
                        commands::MobileStarter::parse(&value.to_string_lossy())
                            .map_err(CliError::Commands)?,
                    );
                }
                "--platform" => {
                    let value = raw_args.next().ok_or_else(|| {
                        CliError::Commands("--platform requires android, ios, or all".to_owned())
                    })?;
                    mobile_platforms = commands::MobilePlatform::parse(&value.to_string_lossy())
                        .map_err(CliError::Commands)?;
                }
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
                application_id,
                application_name,
                mobile_starter,
                mobile_platforms,
            },
        )
        .map_err(CliError::Commands);
    }

    if script_arg == "composer" {
        let arguments = raw_args.collect::<Vec<_>>();
        return composer::run(&executable, &arguments).map_err(CliError::Commands);
    }

    if script_arg == "console" {
        let context = current_project().ok_or_else(|| {
            CliError::Commands("`pam console` must run inside a PAM project".to_owned())
        })?;
        if context.kind != project::ProjectKind::Laravel {
            return Err(CliError::Commands(format!(
                "`pam console` is currently available for Laravel projects; {} projects can register a namespaced command such as app:console in pam.json",
                context.kind.label()
            )));
        }
        let script = resolve_script(context.root.join("artisan").as_os_str())?;
        let mut arguments = vec![OsString::from("tinker")];
        arguments.extend(raw_args);
        // SAFETY: this console owns a single-threaded PHP lifecycle and no workers exist yet.
        unsafe {
            env::set_var("PAM_CLI_MODE", "1");
            env::set_var("APP_RUNNING_IN_CONSOLE", "true");
        }
        return run_script(&executable, &script, arguments);
    }

    if script_arg == "info" {
        let json = match raw_args.next() {
            None => false,
            Some(value) if value == "--json" => true,
            Some(value) => {
                return Err(CliError::Commands(format!(
                    "unknown info option: {}",
                    value.to_string_lossy()
                )));
            }
        };
        if raw_args.next().is_some() {
            return Err(CliError::Commands("info accepts only --json".to_owned()));
        }
        let context = current_project().ok_or_else(|| {
            CliError::Commands("`pam info` must run inside a PAM project".to_owned())
        })?;
        return project::info(&context, json).map_err(CliError::Commands);
    }

    if script_arg == "commands" {
        let json = match raw_args.next() {
            None => false,
            Some(value) if value == "--json" => true,
            Some(value) => {
                return Err(CliError::Commands(format!(
                    "unknown commands option: {}",
                    value.to_string_lossy()
                )));
            }
        };
        if raw_args.next().is_some() {
            return Err(CliError::Commands(
                "commands accepts only --json".to_owned(),
            ));
        }
        let context = current_project().ok_or_else(|| {
            CliError::Commands("`pam commands` must run inside a PAM project".to_owned())
        })?;
        let commands = project::registered_commands(&context).map_err(CliError::Commands)?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema": 1,
                    "commands": commands.iter().map(|command| serde_json::json!({
                        "name": command.name,
                        "description": command.description,
                    })).collect::<Vec<_>>(),
                }))
                .map_err(|error| CliError::Commands(error.to_string()))?
            );
        } else if commands.is_empty() {
            println!("No application or package commands are registered.");
        } else {
            for command in commands {
                println!("{:<28} {}", command.name, command.description);
            }
        }
        return Ok(0);
    }

    if script_arg == "packages" {
        let json = match raw_args.next() {
            None => false,
            Some(value) if value == "--json" => true,
            Some(value) => {
                return Err(CliError::Commands(format!(
                    "unknown packages option: {}",
                    value.to_string_lossy()
                )));
            }
        };
        if raw_args.next().is_some() {
            return Err(CliError::Commands(
                "packages accepts only --json".to_owned(),
            ));
        }
        let context = current_project();
        return ecosystem::list(context.as_ref().map(|context| context.root.as_path()), json)
            .map_err(CliError::Commands);
    }

    if script_arg == "format" {
        let context = current_project().ok_or_else(|| {
            CliError::Commands("`pam format` must run inside a PAM project".to_owned())
        })?;
        let mut check = false;
        let mut paths = Vec::new();
        for argument in raw_args {
            if argument == "--check" {
                check = true;
            } else {
                paths.push(argument);
            }
        }
        return quality::format(&executable, &context.root, check, paths)
            .map_err(CliError::Commands);
    }

    if script_arg == "lint" {
        if raw_args.next().is_some() {
            return Err(CliError::Commands(
                "lint does not accept arguments".to_owned(),
            ));
        }
        let context = current_project().ok_or_else(|| {
            CliError::Commands("`pam lint` must run inside a PAM project".to_owned())
        })?;
        return quality::lint(&executable, &context.root).map_err(CliError::Commands);
    }

    if script_arg == "outdated" {
        let mut direct = true;
        for argument in raw_args {
            match argument.to_string_lossy().as_ref() {
                "--all" => direct = false,
                option => {
                    return Err(CliError::Commands(format!(
                        "unknown outdated option: {option}"
                    )));
                }
            }
        }
        let context = current_project().ok_or_else(|| {
            CliError::Commands("`pam outdated` must run inside a PAM project".to_owned())
        })?;
        return quality::outdated(&executable, &context.root, direct).map_err(CliError::Commands);
    }

    if script_arg == "release" {
        let check_only = match raw_args.next() {
            None => false,
            Some(value) if value == "--check" => true,
            Some(value) => {
                return Err(CliError::Commands(format!(
                    "unknown release option: {}",
                    value.to_string_lossy()
                )));
            }
        };
        if raw_args.next().is_some() {
            return Err(CliError::Commands(
                "release accepts only --check".to_owned(),
            ));
        }
        let context = current_project().ok_or_else(|| {
            CliError::Commands("`pam release` must run inside a PAM project".to_owned())
        })?;
        return ship::release(&executable, &context.root, context.kind, check_only)
            .map_err(CliError::Commands);
    }

    if script_arg == "release:verify" {
        let context = current_project().ok_or_else(|| {
            CliError::Commands("`pam release:verify` must run inside a PAM project".to_owned())
        })?;
        if context.kind != project::ProjectKind::Product {
            return Err(CliError::Commands(
                "`pam release:verify` requires a PAM Product workspace".to_owned(),
            ));
        }
        let manifest = match (raw_args.next(), raw_args.next()) {
            (None, None) => context.root.join("dist/product-release.json"),
            (Some(path), None) => {
                let path = PathBuf::from(path);
                if path.is_absolute() {
                    path
                } else {
                    context.root.join(path)
                }
            }
            _ => {
                return Err(CliError::Commands(
                    "release:verify accepts at most one manifest path".to_owned(),
                ));
            }
        };
        return ship::verify_product_release(&context.root, &manifest).map_err(CliError::Commands);
    }

    if script_arg == "distribution:verify" {
        let mut manifest = None;
        let mut json = false;
        for argument in raw_args {
            if argument == "--json" {
                json = true;
            } else if argument.to_string_lossy().starts_with('-') {
                return Err(CliError::Commands(format!(
                    "unknown distribution:verify option: {}",
                    argument.to_string_lossy()
                )));
            } else if manifest.replace(PathBuf::from(argument)).is_some() {
                return Err(CliError::Commands(
                    "distribution:verify accepts exactly one manifest path".to_owned(),
                ));
            }
        }
        let manifest = manifest.ok_or_else(|| {
            CliError::Commands("distribution:verify requires a manifest path".to_owned())
        })?;
        let result = distribution::verify(&manifest).map_err(CliError::Commands)?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&result)
                    .map_err(|error| CliError::Commands(error.to_string()))?
            );
        } else {
            println!(
                "Distribution evidence valid (surface {}, platform {}, package {}): {}",
                result["surfaceCode"],
                result["platformCode"],
                result["packageCode"],
                manifest.display()
            );
        }
        return Ok(0);
    }

    if script_arg == "distribution:sign" {
        let draft = raw_args.next().ok_or_else(|| {
            CliError::Commands("distribution:sign requires a draft manifest".to_owned())
        })?;
        let mut key = None;
        let mut output = None;
        while let Some(option) = raw_args.next() {
            let destination = match option.to_string_lossy().as_ref() {
                "--key" => &mut key,
                "--output" => &mut output,
                unknown => {
                    return Err(CliError::Commands(format!(
                        "unknown distribution:sign option: {unknown}"
                    )));
                }
            };
            if destination.is_some() {
                return Err(CliError::Commands(format!(
                    "{} may be provided only once",
                    option.to_string_lossy()
                )));
            }
            *destination = Some(PathBuf::from(raw_args.next().ok_or_else(|| {
                CliError::Commands(format!("{} requires a path", option.to_string_lossy()))
            })?));
        }
        let key = key.ok_or_else(|| {
            CliError::Commands("distribution:sign requires --key <private-key>".to_owned())
        })?;
        let output = output.ok_or_else(|| {
            CliError::Commands("distribution:sign requires --output <manifest>".to_owned())
        })?;
        distribution::sign(Path::new(&draft), &key, &output).map_err(CliError::Commands)?;
        println!("Signed distribution evidence: {}", output.display());
        return Ok(0);
    }

    if script_arg == "distribution:desktop-report" {
        let output = distribution::desktop_report(raw_args).map_err(CliError::Commands)?;
        println!("Desktop platform verification: {}", output.display());
        return Ok(0);
    }

    if script_arg == "package" {
        let context = current_project().ok_or_else(|| {
            CliError::Commands("`pam package` must run inside a PAM project".to_owned())
        })?;
        if context.kind == project::ProjectKind::Desktop {
            let mut arguments = vec![OsString::from("build"), context.root.into_os_string()];
            arguments.extend(raw_args);
            return desktop::run(&executable, arguments).map_err(CliError::Commands);
        }
        if context.kind == project::ProjectKind::Product {
            return ship::package_product(&context.root, raw_args).map_err(CliError::Commands);
        }
        if context.kind != project::ProjectKind::Native {
            return ship::package_server(&context.root, context.kind, raw_args)
                .map_err(CliError::Commands);
        }
        if project::native_platforms(&context).map_err(CliError::Commands)? == [2] {
            let mut arguments = vec![OsString::from("ios:package"), context.root.into_os_string()];
            arguments.extend(raw_args);
            return mobile::run(arguments).map_err(CliError::Commands);
        }
        let mut arguments = vec![OsString::from("package"), context.root.into_os_string()];
        arguments.extend(raw_args);
        return mobile::run(arguments).map_err(CliError::Commands);
    }

    if matches!(
        script_arg.to_string_lossy().as_ref(),
        "run" | "logs" | "devices" | "devtools" | "sign"
    ) {
        let context = current_project().ok_or_else(|| {
            CliError::Commands(format!(
                "`pam {}` must run inside a PAM project",
                script_arg.to_string_lossy()
            ))
        })?;
        if script_arg == "run" && context.kind == project::ProjectKind::Desktop {
            let mut arguments = vec![OsString::from("run"), context.root.into_os_string()];
            arguments.extend(raw_args);
            return desktop::run(&executable, arguments).map_err(CliError::Commands);
        }
        if context.kind != project::ProjectKind::Native {
            return Err(CliError::Commands(format!(
                "`pam {}` is not available for {} projects",
                script_arg.to_string_lossy(),
                context.kind.label()
            )));
        }
        let only_ios = project::native_platforms(&context).map_err(CliError::Commands)? == [2];
        let delegated = if only_ios {
            match script_arg.to_string_lossy().as_ref() {
                "run" => OsString::from("ios:run"),
                "logs" => OsString::from("ios:logs"),
                "devices" => OsString::from("ios:devices"),
                "devtools" => OsString::from("ios:devtools"),
                "sign" => OsString::from("ios:sign"),
                _ => script_arg,
            }
        } else {
            script_arg
        };
        let mut arguments = vec![delegated, context.root.into_os_string()];
        arguments.extend(raw_args);
        return mobile::run(arguments).map_err(CliError::Commands);
    }

    if script_arg == "add" || script_arg == "remove" {
        let capability = raw_args.next().ok_or_else(|| {
            CliError::Commands(format!(
                "pam {} requires a capability name",
                script_arg.to_string_lossy()
            ))
        })?;
        if raw_args.next().is_some() {
            return Err(CliError::Commands(format!(
                "pam {} accepts one capability at a time",
                script_arg.to_string_lossy()
            )));
        }
        let capability = capability
            .into_string()
            .map_err(|_| CliError::Commands("capability name must be valid UTF-8".to_owned()))?;
        let context = current_project().ok_or_else(|| {
            CliError::Commands(format!(
                "`pam {}` must run inside a PAM project",
                script_arg.to_string_lossy()
            ))
        })?;
        return if script_arg == "add" {
            ecosystem::add(&executable, &context, &capability)
        } else {
            ecosystem::remove(&executable, &context.root, &capability)
        }
        .map_err(CliError::Commands);
    }

    if script_arg.to_string_lossy().starts_with("make:") {
        let context = current_project().ok_or_else(|| {
            CliError::Commands("generator commands must run inside a PAM project".to_owned())
        })?;
        if context.kind == project::ProjectKind::Native {
            let mut arguments = vec![script_arg];
            arguments.extend(raw_args);
            if arguments.len() == 2 {
                arguments.push(context.root.into_os_string());
            }
            return mobile::run(arguments).map_err(CliError::Commands);
        }
        if context.kind == project::ProjectKind::Laravel {
            let script = resolve_script(context.root.join("artisan").as_os_str())?;
            let mut arguments = vec![script_arg];
            arguments.extend(raw_args);
            // SAFETY: generators run in Artisan's single-threaded console lifecycle.
            unsafe {
                env::set_var("PAM_CLI_MODE", "1");
                env::set_var("APP_RUNNING_IN_CONSOLE", "true");
            }
            return run_script(&executable, &script, arguments);
        }
        let command_name = script_arg.to_string_lossy();
        if let Some(command) = project::registered_commands(&context)
            .map_err(CliError::Commands)?
            .into_iter()
            .find(|command| command.name == command_name)
        {
            return run_script(&executable, &command.script, raw_args.collect());
        }
        return Err(CliError::Commands(format!(
            "{} does not provide {}; register a namespaced make:* command in pam.json to extend it",
            context.kind.label(),
            command_name,
        )));
    }

    if script_arg == "build" {
        if let Some(context) = current_project()
            && context.kind == project::ProjectKind::Native
        {
            if project::native_platforms(&context).map_err(CliError::Commands)? == [2] {
                let mut arguments =
                    vec![OsString::from("ios:build"), context.root.into_os_string()];
                arguments.extend(raw_args);
                return mobile::run(arguments).map_err(CliError::Commands);
            }
            let mut arguments = vec![
                OsString::from("build"),
                context.root.into_os_string(),
                OsString::from("--release"),
            ];
            arguments.extend(raw_args);
            return mobile::run(arguments).map_err(CliError::Commands);
        }
        if let Some(context) = current_project()
            && context.kind == project::ProjectKind::Desktop
        {
            let mut arguments = vec![OsString::from("build"), context.root.into_os_string()];
            arguments.extend(raw_args);
            return desktop::run(&executable, arguments).map_err(CliError::Commands);
        }
        let mut target = OsString::from(".");
        let mut output = OsString::from("dist");
        let mut entry = OsString::from("index.php");
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
        return commands::build(&target, &output, Path::new(&entry)).map_err(CliError::Commands);
    }

    if script_arg == "benchmark" {
        if let Some(context) = current_project()
            && context.kind == project::ProjectKind::Native
        {
            let mut arguments = vec![OsString::from("benchmark"), context.root.into_os_string()];
            arguments.extend(raw_args);
            return mobile::run(arguments).map_err(CliError::Commands);
        }
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

    if let Some(context) = current_project() {
        let command_name = script_arg.to_string_lossy();
        if let Some(command) = project::registered_commands(&context)
            .map_err(CliError::Commands)?
            .into_iter()
            .find(|command| command.name == command_name)
        {
            return run_script(&executable, &command.script, raw_args.collect());
        }
    }
    let script = resolve_script(&script_arg)?;
    let script_args = raw_args.collect::<Vec<_>>();
    run_script(&executable, &script, script_args)
}

fn current_project() -> Option<project::ProjectContext> {
    let directory = env::current_dir().ok()?;
    project::discover(&directory)
}

fn enable_server_opcache() {
    let mut entries = env::var("PAM_INI_ENTRIES").unwrap_or_default();
    for (name, value) in [
        ("opcache.enable_cli", "1"),
        ("opcache.validate_timestamps", "0"),
        ("opcache.jit", "tracing"),
        ("opcache.jit_buffer_size", "128M"),
    ] {
        let prefix = format!("{name}=");
        if !entries
            .lines()
            .any(|entry| entry.trim_start().starts_with(&prefix))
        {
            entries.push_str(name);
            entries.push('=');
            entries.push_str(value);
            entries.push('\n');
        }
    }
    // SAFETY: server command dispatch occurs before PHP or Tokio initializes.
    unsafe { env::set_var("PAM_INI_ENTRIES", entries) };
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

fn finish_dev_with_artifact_budget(
    context: Option<&project::ProjectContext>,
    budget: Option<u64>,
    outcome: Result<u8, CliError>,
) -> Result<u8, CliError> {
    let cleanup = match (context, budget) {
        (Some(context), Some(budget)) => project::enforce_dev_artifact_budget(context, budget),
        _ => Ok(None),
    };
    match (outcome, cleanup) {
        (Err(error), Err(cleanup_error)) => {
            eprintln!("PAM could not complete the post-dev artifact check: {cleanup_error}");
            Err(error)
        }
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(error)) => Err(CliError::Commands(error)),
        (Ok(code), Ok(_)) => Ok(code),
    }
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

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
enum CliErrorCode {
    ScriptUnavailable = 1,
    NotAFile = 2,
    Development = 3,
    Cluster = 4,
    Command = 5,
    Doctor = 6,
    Runtime = 7,
    Server = 8,
}

impl CliError {
    fn code(&self) -> CliErrorCode {
        match self {
            Self::ScriptUnavailable { .. } => CliErrorCode::ScriptUnavailable,
            Self::NotAFile(_) => CliErrorCode::NotAFile,
            Self::Dev(_) => CliErrorCode::Development,
            Self::Cluster(_) => CliErrorCode::Cluster,
            Self::Commands(_) => CliErrorCode::Command,
            Self::Doctor(_) => CliErrorCode::Doctor,
            Self::Runtime(_) => CliErrorCode::Runtime,
            Self::Server(_) => CliErrorCode::Server,
        }
    }

    fn exit_code(&self) -> u8 {
        match self {
            Self::ScriptUnavailable { .. } | Self::NotAFile(_) => EX_NOINPUT,
            Self::Dev(_)
            | Self::Cluster(_)
            | Self::Commands(_)
            | Self::Doctor(_)
            | Self::Runtime(_)
            | Self::Server(_) => EX_SOFTWARE,
        }
    }

    fn remediation(&self) -> &'static str {
        match self {
            Self::ScriptUnavailable { .. } | Self::NotAFile(_) => {
                "check that the path exists and points to the intended file"
            }
            Self::Dev(_) => "fix the reported development error, then restart `pam dev`",
            Self::Cluster(_) => "run `pam doctor`, inspect worker logs, then retry the command",
            Self::Commands(_) => "run `pam --help` or `pam help <command>` for valid usage",
            Self::Doctor(_) => "run `pam doctor --fix`, then repeat `pam doctor`",
            Self::Runtime(_) => "run `pam doctor` and inspect the PHP runtime diagnostics",
            Self::Server(_) => "inspect the server configuration and retry after `pam doctor`",
        }
    }

    fn verification_command(&self) -> &'static str {
        match self {
            Self::ScriptUnavailable { .. } | Self::NotAFile(_) => "pam doctor",
            Self::Dev(_) | Self::Cluster(_) | Self::Doctor(_) | Self::Runtime(_) => {
                "pam doctor --json"
            }
            Self::Commands(_) => "pam --help",
            Self::Server(_) => "pam doctor --json",
        }
    }

    fn json(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": 1,
            "errorCode": self.code() as u8,
            "message": self.to_string(),
            "remediation": self.remediation(),
            "verificationCommand": self.verification_command(),
            "exitCode": self.exit_code(),
        }))
        .expect("CLI error envelope is serializable")
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_dev_budget_removes_outputs_created_during_the_session() {
        let root =
            std::env::temp_dir().join(format!("pam-post-dev-budget-{}", std::process::id(),));
        let artifact = root.join("target/debug/deps/session.bin");
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        let file = fs::File::create(&artifact).unwrap();
        file.set_len(project::DEFAULT_DEV_ARTIFACT_BUDGET_BYTES + 1)
            .unwrap();
        let context = project::ProjectContext {
            root: root.clone(),
            kind: project::ProjectKind::Raw,
        };

        assert_eq!(
            finish_dev_with_artifact_budget(
                Some(&context),
                Some(project::DEFAULT_DEV_ARTIFACT_BUDGET_BYTES),
                Ok(0),
            )
            .unwrap(),
            0,
        );
        assert!(!root.join("target").exists());
        assert!(root.join("Cargo.toml").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
