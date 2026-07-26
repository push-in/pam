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
                "capacity | deploy | remote",
                "Plan capacity and operate local or remote Laravel releases",
            ),
            (
                "rollback | logs | workers | queues | scheduler | scale",
                "Use concise PAM Cloud operation aliases",
            ),
            (
                "nightwatch | compatibility | autoscale | mcp",
                "Certify, observe, scale, and expose controlled Laravel AI tooling",
            ),
            ("exec <script.php>", "Execute a PHP script explicitly"),
            (
                "sandbox <manifest>",
                "Execute PHP under Landlock/seccomp capabilities",
            ),
            ("composer [args...]", "Run the embedded Composer toolchain"),
            ("test [path]", "Run Pest or PHPUnit inside Pam"),
            (
                "snapshot create|verify|run",
                "Build and execute integrity-checked source snapshots",
            ),
            (
                "supply-chain [path]",
                "Audit Composer provenance, policy and capabilities",
            ),
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
                "contracts [index.php]",
                "Generate schemas, clients, mobile bindings and docs",
            ),
            (
                "diagnostics [index.php]",
                "Print the complete runtime snapshot",
            ),
            (
                "heap | fibers | connections",
                "Inspect one diagnostics subsystem",
            ),
            ("profile | trace", "Capture profiling or trace diagnostics"),
            (
                "record [index.php]",
                "Record redacted HTTP interactions for deterministic replay",
            ),
            (
                "replay <recording>",
                "Replay and verify recorded HTTP interactions",
            ),
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
                "verify [bundle]",
                "Verify every file in a production bundle",
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
                (
                    "--admin-token-env NAME",
                    "Enable authenticated reload/drain mutations using this secret",
                ),
                ("--graceful-timeout MS", "Worker shutdown deadline"),
                ("--startup-timeout MS", "Worker readiness deadline"),
                ("--restart-backoff MS", "Initial crash restart delay"),
                ("--watchdog-grace MS", "Hard request-deadline grace period"),
            ],
            &[
                "start index.php --workers 4",
                "start index.php --workers 8 --admin-address 127.0.0.1:3010 --admin-token-env PAM_ADMIN_TOKEN",
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
                (
                    "--signing-key FILE",
                    "Sign manifest with an Ed25519 private key",
                ),
            ],
            &[
                "build .",
                "build . --entry public/index.php --output release",
            ],
        ),
        "verify" => (
            "Verify the integrity and completeness of a production bundle.",
            "verify [bundle] [options]",
            &[
                (
                    "--public-key FILE",
                    "Trusted Ed25519 public key for signature verification",
                ),
                ("--require-signature", "Reject unsigned production bundles"),
            ],
            &[
                "verify dist",
                "verify ./release --public-key release.pub --require-signature",
            ],
        ),
        "record" => (
            "Run an application with the bounded, redacting HTTP flight recorder.",
            "record [index.php] [options] [-- script-arguments...]",
            &[
                (
                    "--output FILE",
                    "JSONL output; default: .pam/recordings/latest.jsonl",
                ),
                (
                    "--max-body-bytes N",
                    "Maximum captured bytes per body; default: 65536",
                ),
                (
                    "--max-bytes N",
                    "Maximum total recording size; default: 67108864",
                ),
            ],
            &[
                "record index.php",
                "record public/index.php --output .pam/incidents/checkout.jsonl",
            ],
        ),
        "sandbox" => (
            "Run untrusted PHP with fail-closed kernel-enforced capabilities.",
            "sandbox <pam.capabilities.json> -- <script.php> [arguments...]",
            &[],
            &["sandbox pam.capabilities.json -- plugin.php"],
        ),
        "contracts" => (
            "Generate every external contract from attributed PHP DTOs and enums.",
            "contracts [index.php] [--output DIR]",
            &[(
                "--output DIR",
                "Generated artifact directory; default: generated/contracts",
            )],
            &[
                "contracts index.php",
                "contracts bootstrap/contracts.php --output generated/api",
            ],
        ),
        "snapshot" => (
            "Create, verify, or run a deterministic integrity-checked PHP source snapshot.",
            "snapshot <create|verify|run> [arguments] [options]",
            &[
                ("--entry FILE", "Create entry point; default: index.php"),
                (
                    "--output FILE",
                    "Create manifest; default: PROJECT/.pam/bootstrap.snapshot.json",
                ),
                ("--signing-key FILE", "Sign a created snapshot with Ed25519"),
                (
                    "--project DIR",
                    "Project root for verify or run; default: .",
                ),
                ("--public-key FILE", "Trusted Ed25519 key for verify or run"),
                ("--require-signature", "Reject unsigned snapshots"),
                (
                    "--",
                    "Pass remaining arguments to the entry point when running",
                ),
            ],
            &[
                "snapshot create . --entry public/index.php",
                "snapshot verify .pam/bootstrap.snapshot.json --project .",
                "snapshot run .pam/bootstrap.snapshot.json --project .",
            ],
        ),
        "supply-chain" => (
            "Audit Composer scripts, plugins, maintainers, licenses, provenance, advisories and capabilities.",
            "supply-chain [directory] [options]",
            &[
                ("--policy FILE", "PAM supply-chain policy JSON"),
                (
                    "--capabilities FILE",
                    "Package capability manifest to audit",
                ),
                ("--output FILE", "Write the deterministic JSON report"),
                ("--offline", "Skip advisories and mark them as unchecked"),
            ],
            &[
                "supply-chain . --policy pam.supply-chain.json",
                "supply-chain package --capabilities package/pam.capabilities.json --offline",
            ],
        ),
        "replay" => (
            "Replay a recording against a live runtime and detect divergence.",
            "replay <recording.jsonl> [options]",
            &[
                (
                    "--url URL",
                    "Runtime base URL; default: http://127.0.0.1:3000",
                ),
                (
                    "--secret-env NAME=ENV_VAR",
                    "Inject a redacted input from an environment variable",
                ),
            ],
            &[
                "replay .pam/recordings/latest.jsonl",
                "PAM_TEST_TOKEN=value replay incident.jsonl --secret-env token=PAM_TEST_TOKEN",
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
        "up" => (
            "Start every process in the Laravel PAM manifest.",
            "up [name]",
            &[],
            &["up", "up web"],
        ),
        "status" => (
            "Inspect processes in the Laravel PAM manifest.",
            "status [name]",
            &[],
            &["status", "status queue"],
        ),
        "restart" => (
            "Restart processes in the Laravel PAM manifest.",
            "restart [name]",
            &[],
            &["restart", "restart web"],
        ),
        "stop" => (
            "Stop processes in the Laravel PAM manifest.",
            "stop [name]",
            &[],
            &["stop", "stop queue"],
        ),
        "check-production" => (
            "Validate a Laravel application before production deployment.",
            "check-production [--json]",
            &[("--json", "Print a machine-readable report")],
            &["check-production", "check-production --json"],
        ),
        "compatibility" => (
            "Certify the Laravel runtime or inspect a package contract.",
            "compatibility [package] [options]",
            &[
                ("--refresh", "Run a fresh compatibility probe"),
                ("--json", "Print a machine-readable report"),
            ],
            &[
                "compatibility",
                "compatibility laravel/nightwatch --refresh --json",
            ],
        ),
        "health" => (
            "Run the Laravel application health checks.",
            "health",
            &[],
            &["health"],
        ),
        "leaks" => (
            "Inspect persistent-worker leak diagnostics.",
            "leaks [--json]",
            &[("--json", "Print a machine-readable report")],
            &["leaks", "leaks --json"],
        ),
        "capacity" => (
            "Estimate safe worker capacity for the available memory.",
            "capacity [options]",
            &[
                ("--memory-mb N", "Total memory budget; default: 512"),
                ("--worker-mb N", "Expected memory per worker; default: 96"),
                (
                    "--reserve-percent N",
                    "Reserved memory percentage; default: 20",
                ),
            ],
            &["capacity --memory-mb 2048 --worker-mb 128"],
        ),
        "deploy" => (
            "Deploy locally, to PAM Cloud, or through Laravel Forge.",
            "deploy [destination] [options]",
            &[
                ("--rollback", "Restore the previous release"),
                ("--local", "Treat destination as a local release directory"),
                ("--release ID", "Deploy a specific remote release"),
            ],
            &[
                "deploy production",
                "deploy ./release --local",
                "deploy production --rollback",
            ],
        ),
        "remote" => (
            "Operate PAM Cloud or Laravel Forge targets.",
            "remote <action> [target] [options]",
            &[
                ("--process NAME", "Process to scale"),
                ("--instances N", "Desired process count, from 1 to 128"),
                ("--release ID", "Release identifier for deploy"),
                ("--lines N", "Log line count; default: 200"),
                ("--json", "Print a machine-readable response"),
            ],
            &[
                "remote status production",
                "remote logs production --lines 500",
                "remote scale production --process queue --instances 4",
            ],
        ),
        "rollback" => (
            "Roll back a remote deployment target.",
            "rollback [target] [--json]",
            &[("--json", "Print a machine-readable response")],
            &["rollback production"],
        ),
        "logs" => (
            "Read logs from a remote deployment target.",
            "logs [target] [options]",
            &[
                ("--lines N", "Log line count; default: 200"),
                ("--json", "Print a machine-readable response"),
            ],
            &["logs production --lines 500"],
        ),
        "workers" => (
            "Inspect workers on a remote deployment target.",
            "workers [target] [--json]",
            &[("--json", "Print a machine-readable response")],
            &["workers production"],
        ),
        "queues" => (
            "Inspect queues on a remote deployment target.",
            "queues [target] [--json]",
            &[("--json", "Print a machine-readable response")],
            &["queues production"],
        ),
        "scheduler" => (
            "Inspect the scheduler on a remote deployment target.",
            "scheduler [target] [--json]",
            &[("--json", "Print a machine-readable response")],
            &["scheduler production"],
        ),
        "scale" => (
            "Scale a remote PAM process.",
            "scale [target] --process NAME --instances N",
            &[
                ("--process NAME", "Process to scale"),
                ("--instances N", "Desired process count, from 1 to 128"),
            ],
            &["scale production --process queue --instances 4"],
        ),
        "nightwatch" => (
            "Validate and configure Laravel Nightwatch for PAM workers.",
            "nightwatch [options]",
            &[
                (
                    "--install-process",
                    "Add the Nightwatch agent to the process manifest",
                ),
                ("--json", "Print a machine-readable report"),
            ],
            &["nightwatch", "nightwatch --install-process --json"],
        ),
        "autoscale" => (
            "Reconcile local worker capacity against live metrics.",
            "autoscale [process] [options]",
            &[
                ("--cpu N", "Current average CPU percentage"),
                ("--p95 N", "Current p95 latency in milliseconds"),
                ("--metrics-url URL", "Live JSON metrics endpoint"),
                ("--watch", "Reconcile continuously"),
                ("--interval N", "Watch interval in seconds; default: 15"),
            ],
            &[
                "autoscale queue --cpu 75 --p95 120",
                "autoscale queue --metrics-url http://127.0.0.1:3010/metrics --watch",
            ],
        ),
        "mcp" => (
            "Serve diagnostics and controlled operations over MCP stdio.",
            "mcp",
            &[],
            &["mcp"],
        ),
        "forge-script" => (
            "Generate a Laravel Forge deployment script.",
            "forge-script [--output FILE]",
            &[("--output FILE", "Write the script to a file")],
            &["forge-script", "forge-script --output deploy.sh"],
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
