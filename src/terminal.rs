use std::env;
use std::ffi::OsStr;
use std::io::{self, IsTerminal};

#[derive(Clone, Copy)]
pub enum Output {
    Stdout,
    Stderr,
}

pub struct Terminal {
    interactive: bool,
    color: bool,
}

impl Terminal {
    pub fn stdout() -> Self {
        Self::new(Output::Stdout)
    }

    pub fn stderr() -> Self {
        Self::new(Output::Stderr)
    }

    fn new(output: Output) -> Self {
        let is_terminal = match output {
            Output::Stdout => io::stdout().is_terminal(),
            Output::Stderr => io::stderr().is_terminal(),
        };
        let color = is_terminal
            && env::var_os("NO_COLOR").is_none()
            && env::var("TERM").map_or(true, |term| term != "dumb")
            && env::var("PAM_COLOR").map_or(true, |value| value != "never");
        Self {
            interactive: is_terminal,
            color,
        }
    }

    pub fn interactive(&self) -> bool {
        self.interactive
    }

    fn paint(&self, value: impl std::fmt::Display, code: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{value}\x1b[0m")
        } else {
            value.to_string()
        }
    }

    pub fn brand(&self, value: impl std::fmt::Display) -> String {
        self.paint(value, "1;96")
    }

    pub fn heading(&self, value: impl std::fmt::Display) -> String {
        self.paint(value, "1;97")
    }

    pub fn accent(&self, value: impl std::fmt::Display) -> String {
        self.paint(value, "96")
    }

    pub fn muted(&self, value: impl std::fmt::Display) -> String {
        self.paint(value, "2;37")
    }

    pub fn success(&self, value: impl std::fmt::Display) -> String {
        self.paint(value, "1;92")
    }

    pub fn warning(&self, value: impl std::fmt::Display) -> String {
        self.paint(value, "1;93")
    }

    pub fn danger(&self, value: impl std::fmt::Display) -> String {
        self.paint(value, "1;91")
    }

    pub fn command(&self, value: impl std::fmt::Display) -> String {
        self.paint(value, "1;96")
    }

    pub fn status(&self, state: &str, message: impl std::fmt::Display) -> String {
        let marker = match state {
            "ok" => self.success("●"),
            "warn" => self.warning("▲"),
            "fail" => self.danger("×"),
            _ => self.accent("◆"),
        };
        let label = match state {
            "ok" => self.success("[ok]"),
            "warn" => self.warning("[warn]"),
            "fail" => self.danger("[fail]"),
            _ => self.accent("[info]"),
        };
        format!("{marker} {label} {message}")
    }

    pub fn rule(&self) -> String {
        self.muted("────────────────────────────────────────────────────────────")
    }

    pub fn clear_screen(&self) {
        if self.interactive {
            print!("\x1b[2J\x1b[H");
        }
    }
}

pub fn launcher(executable: &OsStr) -> Result<u8, String> {
    use std::io::Write;
    use std::process::Command;

    let ui = Terminal::stdout();
    println!("{}", ui.brand("PAM — PHP, Always in Memory"));
    println!("{}", ui.rule());
    println!();
    println!("{}", ui.heading("What do you want to do?"));
    println!("  {}  Create a project", ui.accent("1"));
    println!("  {}  Open this project", ui.accent("2"));
    println!("  {}  Check my environment", ui.accent("3"));
    println!("  {}  Show command reference", ui.accent("4"));
    println!("  {}  Update PAM", ui.accent("5"));
    print!("\n{} ", ui.command("Choose [1] ›"));
    std::io::stdout()
        .flush()
        .map_err(|error| format!("cannot display PAM launcher: {error}"))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("cannot read PAM launcher choice: {error}"))?;
    let executable_path = std::env::current_exe().unwrap_or_else(|_| executable.into());
    match answer.trim() {
        "" | "1" => {
            print!("{} ", ui.command("Project directory ›"));
            std::io::stdout()
                .flush()
                .map_err(|error| format!("cannot display project prompt: {error}"))?;
            let mut directory = String::new();
            std::io::stdin()
                .read_line(&mut directory)
                .map_err(|error| format!("cannot read project directory: {error}"))?;
            let directory = directory.trim();
            if directory.is_empty() {
                return Err("a project directory is required".to_owned());
            }
            child_status(Command::new(executable_path).arg("init").arg(directory))
        }
        "2" => {
            print!("{} ", ui.command("Project directory [.] ›"));
            std::io::stdout()
                .flush()
                .map_err(|error| format!("cannot display project prompt: {error}"))?;
            let mut directory = String::new();
            std::io::stdin()
                .read_line(&mut directory)
                .map_err(|error| format!("cannot read project directory: {error}"))?;
            let directory = directory.trim();
            let directory = if directory.is_empty() { "." } else { directory };
            let directory = std::fs::canonicalize(directory)
                .map_err(|error| format!("cannot open project directory {directory}: {error}"))?;
            if !directory.is_dir() {
                return Err(format!(
                    "project path is not a directory: {}",
                    directory.display()
                ));
            }
            child_status(
                Command::new(executable_path)
                    .arg("info")
                    .current_dir(directory),
            )
        }
        "3" => child_status(Command::new(executable_path).arg("doctor")),
        "4" => {
            print_help(executable);
            Ok(0)
        }
        "5" => child_status(Command::new(executable_path).arg("self-update")),
        value => Err(format!("unknown launcher choice {value:?}")),
    }
}

fn child_status(command: &mut std::process::Command) -> Result<u8, String> {
    let status = command
        .status()
        .map_err(|error| format!("cannot start PAM command: {error}"))?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

pub fn print_help(executable: &OsStr) {
    let ui = Terminal::stderr();
    let executable_name = executable.to_string_lossy();
    let program = executable_name.rsplit('/').next().unwrap_or("pam");
    eprintln!(
        "{}",
        ui.brand("╭────────────────────────────────────────────────────────────╮")
    );
    eprintln!(
        "{}",
        ui.brand(format!(
            "│  PAM  /  PHP ALWAYS IN MEMORY                       v{:<6}│",
            env!("CARGO_PKG_VERSION")
        ))
    );
    eprintln!(
        "{}",
        ui.brand("│  Persistent PHP runtime, powered by Rust + Embed SAPI     │")
    );
    eprintln!(
        "{}",
        ui.brand("╰────────────────────────────────────────────────────────────╯")
    );
    eprintln!();
    eprintln!("{}", ui.heading("USAGE"));
    eprintln!(
        "  {} {}",
        ui.command(program),
        ui.accent("<command> [options]")
    );
    eprintln!(
        "  {} {}",
        ui.command(program),
        ui.accent("--json-errors <command> [options]")
    );
    eprintln!(
        "  {} {}",
        ui.command(program),
        ui.accent("<script.php> [arguments...]")
    );
    eprintln!(
        "  {} {}",
        ui.command(program),
        ui.accent("-r <code> [arguments...]")
    );
    eprintln!();
    command_group(
        &ui,
        "RUNTIME",
        &[
            ("dev [script.php]", "Watch files and reload the application"),
            (
                "start [script.php]",
                "Run a supervised multi-worker cluster",
            ),
            (
                "artisan [args...]",
                "Run the Laravel console inside Embed SAPI",
            ),
            ("octane:start [options]", "Start Laravel Octane on PAM"),
            ("octane:status", "Inspect the PAM Octane master"),
            ("octane:reload", "Reload PAM Octane without downtime"),
            ("octane:stop", "Gracefully stop PAM Octane"),
            ("exec <script.php>", "Execute a PHP script explicitly"),
            ("composer [args...]", "Run the embedded Composer toolchain"),
            ("test [path]", "Run Pest or PHPUnit inside Pam"),
        ],
    );
    command_group(
        &ui,
        "OBSERVE",
        &[
            (
                "doctor [path]",
                "Validate PHP, Composer, and runtime compatibility",
            ),
            ("inspect [index.php]", "Print runtime capabilities as JSON"),
            ("routes [index.php]", "Print registered routes as JSON"),
            (
                "diagnostics [index.php]",
                "Print the complete runtime snapshot",
            ),
            (
                "heap | fibers | connections",
                "Inspect one diagnostics subsystem",
            ),
            ("profile | trace", "Capture profiling or trace diagnostics"),
            ("top [admin-url]", "Stream live runtime metrics"),
            ("benchmark <url>", "Measure throughput and latency"),
        ],
    );
    command_group(
        &ui,
        "PROJECT",
        &[
            ("new | init [directory]", "Create a guided PAM project"),
            ("info", "Describe the current project and active toolchain"),
            ("packages", "Explore official ecosystem capabilities"),
            ("add | remove <capability>", "Manage an official capability"),
            ("make:*", "Generate project-native source files"),
            (
                "format | lint",
                "Format and statically validate project code",
            ),
            ("outdated", "Inspect direct dependency updates"),
            ("commands", "List application and package commands"),
            (
                "editor:install [editor]",
                "Install or print PAM language-editor integration",
            ),
        ],
    );
    command_group(
        &ui,
        "SHIP",
        &[
            (
                "build [directory]",
                "Build a self-contained production bundle",
            ),
            ("package", "Create a signed platform distributable"),
            ("sign", "Validate release-signing configuration"),
            ("release [--check]", "Run every local release gate"),
            (
                "desktop <command>",
                "Develop and diagnose a Pam Desktop app",
            ),
            (
                "mobile <command>",
                "Build, run, profile, and extend a native app",
            ),
        ],
    );
    eprintln!("{}", ui.heading("DISCOVER"));
    eprintln!(
        "  {} {}",
        ui.command(format!("{:<25}", format!("{program} help <command>"))),
        ui.muted("Show focused help and examples")
    );
    eprintln!(
        "  {} {}",
        ui.command(format!("{:<25}", format!("{program} --version"))),
        ui.muted("Print the installed version")
    );
    eprintln!(
        "  {} {}",
        ui.command(format!("{:<25}", format!("{program} self-update"))),
        ui.muted("Install the latest verified release")
    );
    eprintln!();
    eprintln!(
        "{}",
        ui.muted("Color: automatic in terminals · NO_COLOR=1 or PAM_COLOR=never disables it")
    );
}

fn command_group(ui: &Terminal, title: &str, commands: &[(&str, &str)]) {
    eprintln!("{}", ui.heading(title));
    for (command, description) in commands {
        eprintln!(
            "  {} {}",
            ui.command(format!("{command:<31}")),
            ui.muted(description)
        );
    }
    eprintln!();
}

pub fn print_command_help(executable: &OsStr, command: &str) -> bool {
    let ui = Terminal::stderr();
    let executable_name = executable.to_string_lossy();
    let program = executable_name.rsplit('/').next().unwrap_or("pam");
    let (summary, usage, options, examples): (&str, &str, &[(&str, &str)], &[&str]) = match command
    {
        "dev" => (
            "Watch PHP, Composer, and environment files; restart on change.",
            "dev [script.php] [arguments...]",
            &[],
            &["dev", "dev public/index.php"],
        ),
        "start" => (
            "Run a resilient multi-worker cluster with zero-downtime reloads.",
            "start [script.php] [options] [-- script arguments...]",
            &[
                ("--workers N", "Worker process count"),
                ("--max-requests N", "Recycle a worker after N requests"),
                (
                    "--admin-address IP:PORT",
                    "Expose health and metrics control plane",
                ),
                ("--graceful-timeout MS", "Worker shutdown deadline"),
                ("--startup-timeout MS", "Worker readiness deadline"),
                ("--restart-backoff MS", "Initial crash restart delay"),
                ("--watchdog-grace MS", "Hard request-deadline grace period"),
            ],
            &[
                "start index.php --workers 4",
                "start index.php --workers 8 --admin-address 127.0.0.1:3010",
            ],
        ),
        "octane:start" => (
            "Start Laravel Octane on PAM's Rust and Tokio runtime.",
            "octane:start [--workers N | --pool SPEC...] [--host ADDRESS] [--port PORT]",
            &[
                ("--workers N", "Number of isolated Laravel workers"),
                ("--max-requests N", "Recycle workers after N requests"),
                ("--admin-address IP:PORT", "Control-plane address"),
                (
                    "--ingress-address IP:PORT",
                    "Public address for worker pools",
                ),
                ("--pool NAME=N@PREFIXES", "Isolated route worker pool"),
                ("--host ADDRESS", "Address to bind"),
                ("--port PORT", "HTTP port"),
            ],
            &[
                "octane:start --workers=8",
                "octane:start --workers=8 --host=0.0.0.0 --port=8080",
                "octane:start --ingress-address=0.0.0.0:8000 --pool=api=8@/api --pool=web=4@*",
            ],
        ),
        "octane:status" => (
            "Inspect the supervised PAM Octane process.",
            "octane:status",
            &[],
            &["octane:status"],
        ),
        "octane:reload" => (
            "Start a new PAM Octane generation and drain the old workers.",
            "octane:reload",
            &[],
            &["octane:reload"],
        ),
        "octane:stop" => (
            "Gracefully drain and stop PAM Octane.",
            "octane:stop",
            &[],
            &["octane:stop"],
        ),
        "init" => (
            "Scaffold a production-ready Pam project.",
            "init [directory] [options]",
            &[
                (
                    "--template PRESET",
                    "raw, api, laravel, desktop, mobile, or mobile-ui",
                ),
                ("--socket", "Add Pam Socket support"),
                ("--name NAME", "Human-readable mobile application name"),
                (
                    "--application-id ID",
                    "Mobile bundle/application identifier",
                ),
                (
                    "--starter PRESET",
                    "blank, tabs, auth, ecommerce, chat, or showcase",
                ),
                ("--platform TARGET", "android, ios, or all"),
                ("--no-install", "Create files without installing packages"),
                ("--no-interaction", "Use API when no preset is supplied"),
            ],
            &[
                "init my-api --template api",
                "init my-app --template laravel --socket",
                "init native-app --template mobile --no-install",
                "init ui-app --template mobile-ui --no-install",
            ],
        ),
        "build" => (
            "Create a self-contained production bundle with integrity manifest.",
            "build [directory] [options]",
            &[
                (
                    "--entry FILE",
                    "Application entry point; default: index.php",
                ),
                ("--output DIR", "New bundle directory; default: dist"),
            ],
            &[
                "build .",
                "build . --entry public/index.php --output release",
            ],
        ),
        "test" => (
            "Run the project's test suite inside the Pam Embed SAPI.",
            "test [path] [options] [runner arguments...]",
            &[
                ("--pest", "Force the Pest runner"),
                ("--phpunit", "Force the PHPUnit runner"),
            ],
            &["test", "test . --pest --filter RuntimeTest"],
        ),
        "doctor" => (
            "Audit and optionally repair the active project toolchain.",
            "doctor [path] [options]",
            &[
                ("--fix", "Apply safe repairs after dependency preflight"),
                ("--ci", "Disable interactive/color output for automation"),
                ("--json", "Emit a stable structured diagnostic envelope"),
            ],
            &[
                "doctor",
                "doctor --fix",
                "doctor ./my-project --ci",
                "doctor --json",
            ],
        ),
        "clean" => (
            "Inspect and remove regenerable development artifacts inside one project.",
            "clean [path] [options]",
            &[
                ("--dry-run", "Measure and report without deleting anything"),
                (
                    "--all",
                    "Also remove complete generated hosts and Cargo target",
                ),
                ("--json", "Emit a stable machine-readable cleanup report"),
            ],
            &["clean --dry-run", "clean . --json", "clean . --all"],
        ),
        "top" => (
            "Display live metrics from a Pam cluster control plane.",
            "top [admin-url] [options]",
            &[
                ("--iterations N", "Number of samples; default: 10"),
                ("--interval-ms N", "Delay between samples; default: 1000"),
            ],
            &["top", "top http://127.0.0.1:3010 --iterations 60"],
        ),
        "benchmark" => (
            "Measure HTTP throughput, success rate, and latency percentiles.",
            "benchmark <url> [options]",
            &[
                ("--requests N", "Total request count; default: 100"),
                ("--concurrency N", "Parallel request count; default: 10"),
            ],
            &["benchmark http://127.0.0.1:3000/health --requests 1000 --concurrency 32"],
        ),
        "package" => (
            "Create a versioned platform distributable and SHA-256 checksum.",
            "package [options]",
            &[
                (
                    "--entry FILE",
                    "Server entry point; inferred by project type",
                ),
                ("--output DIR", "Artifact directory; default: dist"),
            ],
            &[
                "package",
                "package --entry public/index.php --output artifacts",
            ],
        ),
        "console" => (
            "Open the contextual interactive application console.",
            "console [arguments...]",
            &[],
            &[
                "console",
                "console --execute=\"App\\Models\\User::count()\"",
            ],
        ),
        "editor:install" => (
            "Install or print PAM Native language support for your editor.",
            "editor:install [vscode|neovim|helix] [options]",
            &[("--force", "Replace an existing VS Code extension")],
            &["editor:install vscode", "editor:install neovim"],
        ),
        "self-update" => (
            "Install a release through PAM's HTTPS and SHA-256 verified channel.",
            "self-update [vMAJOR.MINOR.PATCH] [options]",
            &[(
                "--check",
                "Report whether the requested/latest release differs",
            )],
            &["self-update --check", "self-update v1.0.0"],
        ),
        "mobile" => (
            "Build native Android and iOS applications powered by PHP.",
            "mobile <command> [project] [options]",
            &[
                ("doctor", "Validate the Android and PAM Native toolchain"),
                (
                    "audit",
                    "Audit native permissions and release dependency authority",
                ),
                ("prepare", "Stage the project and generate its Android host"),
                ("codegen", "Regenerate Kotlin native-module bindings"),
                (
                    "ios:doctor | ios:prepare",
                    "Validate and generate the iOS host",
                ),
                ("ios:build | ios:run", "Build or launch on an iOS Simulator"),
                (
                    "ios:devices | ios:logs",
                    "Inspect iOS simulators and application logs",
                ),
                (
                    "screenshot | ios:screenshot",
                    "Capture a validated PNG for visual tests",
                ),
                (
                    "ios:sign | ios:package",
                    "Validate signing and export a signed IPA",
                ),
                ("build | run | dev", "Build, launch, or hot-reload the app"),
                (
                    "sign | package",
                    "Validate signing and create signed APK/AAB release artifacts",
                ),
                (
                    "benchmark | profile",
                    "Measure performance or create a baseline profile",
                ),
                ("devtools", "Toggle the live performance overlay"),
                ("diagnostics", "Capture a redacted live Android snapshot"),
                ("logs | devices", "Inspect app logs and connected targets"),
                ("plugin:list | plugin:doctor", "Inspect native plugins"),
                (
                    "runtime:list | runtime:info",
                    "Inspect selectable embedded PHP runtimes",
                ),
                (
                    "runtime:use | runtime:update",
                    "Select and lock PHP 8.4 or 8.5",
                ),
                (
                    "runtime:install",
                    "Install verified Android runtimes and native engines",
                ),
                ("make:*", "Generate screens, components, or native views"),
            ],
            &[
                "mobile doctor",
                "mobile audit . --deny-high --json",
                "mobile dev .",
                "mobile devtools .",
                "mobile diagnostics .",
                "mobile screenshot . --output artifacts/home.png",
                "mobile runtime:use 8.5 .",
                "mobile runtime:install .",
                "mobile make:screen Dashboard .",
                "mobile ios:doctor .",
                "mobile ios:run .",
            ],
        ),
        _ => {
            let Some(spec) = crate::catalog::COMMANDS
                .iter()
                .find(|spec| spec.name == command)
            else {
                return false;
            };
            eprintln!(
                "{}  {}",
                ui.brand(format!("PAM / {}", command.to_uppercase())),
                ui.muted(spec.summary)
            );
            eprintln!("{}", ui.rule());
            eprintln!();
            eprintln!("{}", ui.heading("USAGE"));
            eprintln!("  {} {} [options]", ui.command(program), command);
            eprintln!();
            eprintln!("{}", ui.heading("DISCOVER"));
            eprintln!(
                "  {}",
                ui.muted("Run the command without options for contextual guidance.")
            );
            return true;
        }
    };

    eprintln!(
        "{}  {}",
        ui.brand(format!("PAM / {}", command.to_uppercase())),
        ui.muted(summary)
    );
    eprintln!("{}", ui.rule());
    eprintln!();
    eprintln!("{}", ui.heading("USAGE"));
    eprintln!("  {} {}", ui.command(program), usage);
    if !options.is_empty() {
        eprintln!();
        eprintln!("{}", ui.heading("OPTIONS"));
        for (option, description) in options {
            eprintln!(
                "  {} {}",
                ui.accent(format!("{option:<31}")),
                ui.muted(description)
            );
        }
    }
    eprintln!();
    eprintln!("{}", ui.heading("EXAMPLES"));
    for example in examples {
        eprintln!("  {}", ui.command(format!("$ {program} {example}")));
    }
    true
}
