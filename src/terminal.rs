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
        let interactive = match output {
            Output::Stdout => io::stdout().is_terminal(),
            Output::Stderr => io::stderr().is_terminal(),
        };
        let color = interactive
            && env::var_os("NO_COLOR").is_none()
            && env::var("TERM").map_or(true, |term| term != "dumb")
            && env::var("PAM_COLOR").map_or(true, |value| value != "never");
        Self { interactive, color }
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
            (
                "up | status | restart | stop",
                "Manage a Laravel PAM process manifest",
            ),
            (
                "check-production | health | leaks",
                "Run Laravel production diagnostics",
            ),
            (
                "capacity | deploy",
                "Plan capacity or activate an atomic Laravel release",
            ),
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
        "SHIP",
        &[
            (
                "init [directory]",
                "Create a raw, API, Laravel, desktop, or mobile app",
            ),
            (
                "build [directory]",
                "Build a self-contained production bundle",
            ),
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
        "init" => (
            "Scaffold a production-ready Pam project.",
            "init [directory] [options]",
            &[
                (
                    "--template PRESET",
                    "raw, api, laravel, desktop, mobile, or mobile-ui",
                ),
                ("--socket", "Add Pam Socket support"),
                ("--no-install", "Create files without installing packages"),
                ("--no-interaction", "Use API when no preset is supplied"),
            ],
            &[
                "init my-api --template api",
                "init my-app --template laravel --socket",
                "init native-app --template mobile --no-install",
                "init polished-app --template mobile-ui --no-install",
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
            "Audit the embedded runtime and project compatibility.",
            "doctor [path]",
            &[],
            &["doctor", "doctor ./my-project"],
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
        "mobile" => (
            "Build native Android applications powered by PHP.",
            "mobile <command> [project] [options]",
            &[
                ("doctor", "Validate Android and Pam Native toolchains"),
                ("prepare", "Stage the project and generate its Android host"),
                ("codegen", "Regenerate Kotlin native-module bindings"),
                ("build | run | dev", "Build, launch, or hot-reload the app"),
                (
                    "benchmark | profile",
                    "Measure performance or create a baseline profile",
                ),
                ("devtools", "Toggle the live performance overlay"),
                ("plugin:list | plugin:doctor", "Inspect native plugins"),
                ("make:*", "Generate screens, components, or native views"),
            ],
            &[
                "mobile doctor",
                "mobile dev .",
                "mobile devtools .",
                "mobile make:screen Dashboard .",
            ],
        ),
        _ => return false,
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
