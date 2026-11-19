//! `pam deploy:remote HOST`: ships the project to a server with the system `ssh` and
//! `rsync`, installs production dependencies, runs `bin/migrate` and activates the
//! release through the readiness-gated `pam deploy` of a per-application `pamd`.
//!
//! Layout on the server (`ROOT` is `deploy.path`, relative to the remote home):
//!
//! ```text
//! ROOT/releases/<UTC timestamp>/   one directory per release (vendor installed there)
//! ROOT/shared/.env                 linked into every release and passed as --env-file
//! ROOT/shared/storage/             linked as storage/ (sessions, SQLite, caches)
//! ROOT/state/                      PAM_MANAGER_STATE_DIR of the application's pamd
//! ROOT/current -> releases/<id>    the active release
//! ```
//!
//! `--init` also installs `~/.config/systemd/user/pam-<name>.service`, which runs that
//! `pamd`; it restores the saved application after a reboot.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_KEEP: usize = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteDeploy {
    pub host: String,
    /// Absolute, or relative to the remote user's home.
    pub path: String,
    pub name: String,
    pub workers: Option<u16>,
    pub health_check_url: Option<String>,
    pub keep: usize,
    /// The `pam` executable on the server.
    pub pam: String,
    pub init: bool,
    pub dry_run: bool,
}

pub fn run(arguments: Vec<OsString>) -> Result<u8, String> {
    let current = std::env::current_dir().map_err(|error| error.to_string())?;
    let root = crate::project::discover(&current)
        .map(|context| context.root)
        .unwrap_or(current);
    let manifest = fs::read_to_string(root.join("pam.json"))
        .ok()
        .and_then(|source| serde_json::from_str::<serde_json::Value>(&source).ok());
    let fallback_name = manifest
        .as_ref()
        .and_then(|manifest| manifest["name"].as_str())
        .map(str::to_owned)
        .or_else(|| {
            root.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "pam-app".to_owned());
    let deploy = parse(
        arguments,
        manifest.as_ref().map(|manifest| &manifest["deploy"]),
        &fallback_name,
    )?;
    if !root.join("composer.json").is_file() {
        return Err(format!(
            "{} has no composer.json; deploy:remote ships Composer applications",
            root.display()
        ));
    }
    let release = release_id(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock is before the Unix epoch".to_owned())?
            .as_secs(),
    );

    let mut steps = Vec::new();
    if deploy.init {
        let (remote_root, pam) = if deploy.dry_run {
            // Resolved on the server by the real run.
            let absolute = |value: &str, placeholder: &str| {
                if value.starts_with('/') {
                    value.to_owned()
                } else {
                    format!("{placeholder}/{value}")
                }
            };
            (
                absolute(&deploy.path, "/home/REMOTE_USER"),
                absolute(&deploy.pam, "/usr/local/bin"),
            )
        } else {
            probe(&deploy)?
        };
        let unit = systemd_unit(&deploy.name, &remote_root, &pam)?;
        let environment = production_environment(&root, deploy.dry_run)?;
        steps.push(ssh(
            &deploy.host,
            &init_script(&deploy, &unit, &environment),
        ));
    }
    steps.push(ssh(&deploy.host, &prepare_script(&deploy, &release)));
    steps.push(rsync(&deploy, &root, &release));
    steps.push(ssh(&deploy.host, &release_script(&deploy, &release)));

    for step in &steps {
        if deploy.dry_run {
            println!("{}", display(step));
            continue;
        }
        let status = Command::new(&step[0])
            .args(&step[1..])
            .current_dir(&root)
            .status()
            .map_err(|error| format!("cannot run {}: {error}", step[0]))?;
        if !status.success() {
            return Err(format!(
                "{} failed with {status}; the active release was not changed",
                step[0]
            ));
        }
    }
    if !deploy.dry_run {
        println!(
            "Deployed {} release {release} to {}:{}",
            deploy.name, deploy.host, deploy.path
        );
    }
    Ok(0)
}

/// Options win over the `deploy` object of `pam.json`.
pub fn parse(
    arguments: Vec<OsString>,
    config: Option<&serde_json::Value>,
    fallback_name: &str,
) -> Result<RemoteDeploy, String> {
    let config = config.filter(|value| value.is_object());
    let text = |key: &str| {
        config
            .and_then(|config| config[key].as_str())
            .map(str::to_owned)
    };
    let mut positional_host = None;
    let mut path = text("path");
    let mut name = text("name");
    let mut workers = config
        .and_then(|config| config["workers"].as_u64())
        .map(|workers| u16::try_from(workers).map_err(|_| "deploy.workers is too large".to_owned()))
        .transpose()?;
    let mut health_check_url = text("healthCheckUrl");
    let mut keep = config
        .and_then(|config| config["keep"].as_u64())
        .map_or(DEFAULT_KEEP, |keep| keep as usize);
    let mut pam = text("pam").unwrap_or_else(|| "pam".to_owned());
    let mut init = false;
    let mut dry_run = false;

    let mut arguments = arguments.into_iter();
    let value = |arguments: &mut std::vec::IntoIter<OsString>, option: &str| {
        arguments
            .next()
            .and_then(|value| value.into_string().ok())
            .ok_or_else(|| format!("{option} requires a value"))
    };
    while let Some(argument) = arguments.next() {
        let argument = argument
            .into_string()
            .map_err(|_| "deploy:remote arguments must be UTF-8".to_owned())?;
        match argument.as_str() {
            "--path" => path = Some(value(&mut arguments, "--path")?),
            "--name" => name = Some(value(&mut arguments, "--name")?),
            "--workers" => {
                workers = Some(
                    value(&mut arguments, "--workers")?
                        .parse()
                        .map_err(|_| "--workers requires a number from 1 to 65535".to_owned())?,
                )
            }
            "--health-check-url" => {
                health_check_url = Some(value(&mut arguments, "--health-check-url")?)
            }
            "--keep" => {
                keep = value(&mut arguments, "--keep")?
                    .parse()
                    .map_err(|_| "--keep requires a number".to_owned())?
            }
            "--pam" => pam = value(&mut arguments, "--pam")?,
            "--init" => init = true,
            "--dry-run" => dry_run = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown deploy:remote option: {option}"));
            }
            _ if positional_host.is_none() => positional_host = Some(argument),
            _ => return Err("deploy:remote accepts one host".to_owned()),
        }
    }
    let host = positional_host.or_else(|| text("host")).ok_or_else(|| {
        "deploy:remote requires a host (user@server) or deploy.host in pam.json".to_owned()
    })?;
    let name = name.unwrap_or_else(|| fallback_name.to_owned());
    let path = path.unwrap_or_else(|| format!("pam-apps/{name}"));
    validate_host(&host)?;
    validate_name(&name)?;
    validate_path(&path, "deploy path")?;
    validate_path(&pam, "remote pam executable")?;
    if workers == Some(0) {
        return Err("--workers must be positive".to_owned());
    }
    if !(1..=100).contains(&keep) {
        return Err("--keep must be between 1 and 100".to_owned());
    }
    if let Some(url) = &health_check_url
        && (!url.starts_with("http://") || url.chars().any(char::is_whitespace))
    {
        return Err("--health-check-url must be an http:// URL of the application".to_owned());
    }
    Ok(RemoteDeploy {
        host,
        path,
        name,
        workers,
        health_check_url,
        keep,
        pam,
        init,
        dry_run,
    })
}

/// `YYYYMMDDHHMMSS` in UTC; releases sort by name.
pub fn release_id(timestamp: u64) -> String {
    crate::api_generator::migration_date(timestamp).replace('.', "")
}

/// Creates the release and shared directories.
pub fn prepare_script(deploy: &RemoteDeploy, release: &str) -> String {
    format!(
        "set -eu\nmkdir -p -- {path}/releases/{release} {path}/shared/storage {path}/state\n",
        path = shell_quote(&deploy.path),
    )
}

pub fn rsync(deploy: &RemoteDeploy, source: &Path, release: &str) -> Vec<String> {
    let mut source = source.to_string_lossy().into_owned();
    if !source.ends_with('/') {
        source.push('/');
    }
    let mut command = vec!["rsync".to_owned(), "-az".to_owned(), "--delete".to_owned()];
    for excluded in [
        "/vendor/",
        "/.git/",
        "node_modules/",
        "/.pam/",
        "/.env",
        "/storage/",
    ] {
        command.push(format!("--exclude={excluded}"));
    }
    // Unchanged files are hard links to the active release instead of new copies.
    command.push("--link-dest=../../current/".to_owned());
    command.push(source);
    // validate_path() keeps the path free of characters a remote shell would interpret.
    command.push(format!(
        "{}:{}/releases/{release}/",
        deploy.host, deploy.path
    ));
    command
}

/// Installs dependencies, migrates, then activates through `pam up` (first release)
/// or the readiness-gated `pam deploy`, which keeps the previous release on failure.
pub fn release_script(deploy: &RemoteDeploy, release: &str) -> String {
    let pam = shell_quote(&deploy.pam);
    let name = shell_quote(&deploy.name);
    let mut up = format!("\"$PAM\" up --name {name}");
    if let Some(workers) = deploy.workers {
        up.push_str(&format!(" --workers {workers}"));
    }
    if let Some(url) = &deploy.health_check_url {
        up.push_str(&format!(" --health-check-url {}", shell_quote(url)));
    }
    format!(
        r#"set -eu
PAM={pam}
ROOT=$(cd -- {path} && pwd -P)
RELEASE="$ROOT/releases/{release}"
cd -- "$RELEASE"
ln -sfn "$ROOT/shared/storage" storage
ENV_FILE=""
if [ -f "$ROOT/shared/.env" ]; then
    ln -sfn "$ROOT/shared/.env" .env
    ENV_FILE="$ROOT/shared/.env"
fi
export PAM_ENV=production
"$PAM" composer install --no-dev --optimize-autoloader --no-interaction --no-progress
if [ -f bin/migrate ]; then
    "$PAM" exec bin/migrate
fi
export PAM_MANAGER_STATE_DIR="$ROOT/state"
export PAM_MANAGER_RUNTIME_DIR="$ROOT/state/run"
if "$PAM" describe {name} >/dev/null 2>&1; then
    "$PAM" deploy {name} "$RELEASE"
elif [ -n "$ENV_FILE" ]; then
    {up} --env-file "$ENV_FILE" index.php
else
    {up} index.php
fi
"$PAM" save >/dev/null
ln -sfn "releases/{release}" "$ROOT/current.next"
mv -T "$ROOT/current.next" "$ROOT/current"
cd -- "$ROOT/releases"
ls -1 | sort -r | tail -n +{first_pruned} | while read -r old; do
    [ "$old" = "{release}" ] || rm -rf -- "$old"
done
"#,
        path = shell_quote(&deploy.path),
        first_pruned = deploy.keep + 1,
    )
}

/// Creates the layout, a production `.env` when the server has none, and installs and
/// starts the systemd user unit that runs the application's `pamd`.
pub fn init_script(deploy: &RemoteDeploy, unit: &str, environment: &str) -> String {
    let service = format!("pam-{}.service", deploy.name);
    format!(
        r#"set -eu
mkdir -p -- {path}/releases {path}/shared/storage {path}/state
if [ ! -f {path}/shared/.env ]; then
    umask 077
    cat > {path}/shared/.env <<'PAM_ENV_FILE'
{environment}PAM_ENV_FILE
    echo "Created {path}/shared/.env; review it before the first request."
fi
mkdir -p "$HOME/.config/systemd/user"
cat > "$HOME/.config/systemd/user/{service}" <<'PAM_UNIT'
{unit}PAM_UNIT
systemctl --user daemon-reload
systemctl --user enable --now {service}
loginctl enable-linger "$(id -un)" 2>/dev/null || echo "Run 'sudo loginctl enable-linger $(id -un)' so {service} starts at boot without a login."
"#,
        path = shell_quote(&deploy.path),
    )
}

/// The unit of the application's process manager; the application itself is the saved
/// `pam up` record, restored when `pamd` starts.
pub fn systemd_unit(name: &str, root: &str, pam: &str) -> Result<String, String> {
    validate_name(name)?;
    for (value, label) in [(root, "remote deploy path"), (pam, "remote pam executable")] {
        if !value.starts_with('/') {
            return Err(format!("{label} must resolve to an absolute path"));
        }
        validate_path(value, label)?;
    }
    Ok(format!(
        "[Unit]\nDescription=PAM application {name}\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nWorkingDirectory={root}\nEnvironment=PAM_ENV=production\nEnvironment=PAM_MANAGER_STATE_DIR={root}/state\nEnvironment=PAM_MANAGER_RUNTIME_DIR={root}/state/run\nExecStart={pam} __pamd\nExecStop={pam} daemon stop\nRestart=on-failure\nRestartSec=1\nNoNewPrivileges=true\nPrivateTmp=true\n\n[Install]\nWantedBy=default.target\n"
    ))
}

/// `.env.example` of the project with a fresh `APP_KEY`, secure session cookies and
/// `PAM_ENV=production`. A dry run prints a placeholder instead of the key.
fn production_environment(root: &Path, dry_run: bool) -> Result<String, String> {
    let example = fs::read_to_string(root.join(".env.example")).unwrap_or_default();
    production_environment_from(&example, || {
        if dry_run {
            Ok("<generated>".to_owned())
        } else {
            app_key()
        }
    })
}

pub fn production_environment_from(
    example: &str,
    key: impl Fn() -> Result<String, String>,
) -> Result<String, String> {
    let mut environment = String::new();
    let mut has_key = false;
    for line in example.lines() {
        if line.starts_with("APP_KEY=") {
            has_key = true;
            environment.push_str(&format!("APP_KEY={}\n", key()?));
        } else if line.starts_with("SESSION_SECURE_COOKIE=") {
            environment.push_str("SESSION_SECURE_COOKIE=true\n");
        } else if !line.starts_with("PAM_ENV=") {
            environment.push_str(line);
            environment.push('\n');
        }
    }
    if !has_key && example.contains("APP_") {
        environment.push_str(&format!("APP_KEY={}\n", key()?));
    }
    environment.push_str("PAM_ENV=production\n");
    if environment.lines().any(|line| line == "PAM_ENV_FILE") {
        return Err(".env.example cannot contain the line PAM_ENV_FILE".to_owned());
    }
    Ok(environment)
}

fn app_key() -> Result<String, String> {
    use std::io::Read;
    let mut bytes = [0_u8; 32];
    fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| format!("cannot generate APP_KEY: {error}"))?;
    Ok(format!(
        "base64:{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
    ))
}

/// Resolves the absolute deploy path and the `pam` executable on the server.
fn probe(deploy: &RemoteDeploy) -> Result<(String, String), String> {
    let script = format!(
        "set -eu\nmkdir -p -- {path}\ncd -- {path}\npwd -P\ncommand -v {pam}\n",
        path = shell_quote(&deploy.path),
        pam = shell_quote(&deploy.pam),
    );
    let command = ssh(&deploy.host, &script);
    let output = Command::new(&command[0])
        .args(&command[1..])
        .output()
        .map_err(|error| format!("cannot run ssh: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot prepare {}:{} or find {} there: {}",
            deploy.host,
            deploy.path,
            deploy.pam,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines().filter(|line| !line.is_empty());
    match (lines.next(), lines.next()) {
        (Some(root), Some(pam)) => Ok((root.to_owned(), pam.to_owned())),
        _ => Err(format!(
            "{} is not installed on {}; install PAM there first",
            deploy.pam, deploy.host
        )),
    }
}

/// Runs the script in a login shell, so the PATH of the remote profile applies.
pub fn ssh(host: &str, script: &str) -> Vec<String> {
    vec![
        "ssh".to_owned(),
        "-o".to_owned(),
        "BatchMode=yes".to_owned(),
        host.to_owned(),
        "--".to_owned(),
        format!("bash -lc {}", shell_quote(script)),
    ]
}

/// POSIX single quoting; `~/` stays outside the quotes so the remote shell expands it.
pub fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-:=@+,".contains(&byte))
    {
        return value.to_owned();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn display(command: &[String]) -> String {
    command
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_host(host: &str) -> Result<(), String> {
    let valid = !host.is_empty()
        && host.len() <= 255
        && !host.starts_with('-')
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@._-[]:".contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(format!("invalid deploy host {host:?}"))
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "invalid application name {name:?}; use letters, digits, '.', '_' and '-'"
        ))
    }
}

/// Paths end up in shell scripts and systemd units: a conservative character set.
fn validate_path(path: &str, label: &str) -> Result<(), String> {
    let valid = !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('-')
        && !path.split('/').any(|segment| segment == "..")
        && path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/._-+@".contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(format!(
            "invalid {label} {path:?}; use letters, digits, '/', '.', '_', '-', '+' and '@'"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn deploy() -> RemoteDeploy {
        parse(
            arguments(&["deploy@app.example.com", "--workers", "4"]),
            None,
            "shop",
        )
        .unwrap()
    }

    #[test]
    fn reads_pam_json_and_lets_options_win() {
        let config = serde_json::json!({
            "host": "ops@old.example.com",
            "path": "/srv/shop",
            "workers": 2,
            "keep": 3,
            "healthCheckUrl": "http://127.0.0.1:3000/login"
        });
        let parsed = parse(arguments(&["--workers", "8"]), Some(&config), "shop").unwrap();
        assert_eq!(parsed.host, "ops@old.example.com");
        assert_eq!(parsed.path, "/srv/shop");
        assert_eq!(parsed.workers, Some(8));
        assert_eq!(parsed.keep, 3);
        assert_eq!(parsed.name, "shop");

        let defaults = deploy();
        assert_eq!(defaults.host, "deploy@app.example.com");
        assert_eq!(defaults.path, "pam-apps/shop");
        assert_eq!(defaults.pam, "pam");
        assert!(!defaults.init);
    }

    #[test]
    fn rejects_unsafe_values() {
        assert!(parse(arguments(&[]), None, "shop").is_err());
        assert!(parse(arguments(&["-oProxyCommand=x"]), None, "shop").is_err());
        assert!(parse(arguments(&["host", "--path", "a b"]), None, "shop").is_err());
        assert!(parse(arguments(&["host", "--path", "../etc"]), None, "shop").is_err());
        assert!(parse(arguments(&["host", "--name", "a;b"]), None, "shop").is_err());
        assert!(parse(arguments(&["host", "--keep", "0"]), None, "shop").is_err());
        assert!(parse(arguments(&["host", "--bogus"]), None, "shop").is_err());
        assert!(parse(arguments(&["host", "other"]), None, "shop").is_err());
    }

    #[test]
    fn rsync_skips_vendor_git_node_modules_and_server_state() {
        let command = rsync(&deploy(), Path::new("/work/shop"), "20261119120000");
        assert_eq!(command[0], "rsync");
        for excluded in ["/vendor/", "/.git/", "node_modules/", "/.env", "/storage/"] {
            assert!(command.contains(&format!("--exclude={excluded}")));
        }
        assert_eq!(command[command.len() - 2], "/work/shop/");
        assert_eq!(
            command[command.len() - 1],
            "deploy@app.example.com:pam-apps/shop/releases/20261119120000/"
        );
    }

    #[test]
    fn release_script_installs_migrates_and_activates_through_pam_deploy() {
        let script = release_script(&deploy(), "20261119120000");
        let install = script
            .find("composer install --no-dev --optimize-autoloader")
            .unwrap();
        let migrate = script.find("\"$PAM\" exec bin/migrate").unwrap();
        let activate = script.find("\"$PAM\" deploy shop \"$RELEASE\"").unwrap();
        assert!(install < migrate && migrate < activate);
        assert!(
            script
                .contains("\"$PAM\" up --name shop --workers 4 --env-file \"$ENV_FILE\" index.php")
        );
        assert!(script.contains("export PAM_MANAGER_STATE_DIR=\"$ROOT/state\""));
        assert!(script.contains("tail -n +6"));
        assert!(script.starts_with("set -eu\n"));
    }

    #[test]
    fn ssh_runs_one_quoted_login_shell_script() {
        let command = ssh("deploy@app", "echo 'a b'\n");
        assert_eq!(
            &command[..5],
            ["ssh", "-o", "BatchMode=yes", "deploy@app", "--"]
        );
        assert_eq!(command[5], "bash -lc 'echo '\\''a b'\\''\n'");
        assert_eq!(shell_quote("pam-apps/shop"), "pam-apps/shop");
        assert_eq!(shell_quote("a b"), "'a b'");
    }

    #[test]
    fn systemd_unit_runs_the_application_pamd() {
        let unit =
            systemd_unit("shop", "/home/deploy/pam-apps/shop", "/usr/local/bin/pam").unwrap();
        assert!(unit.contains("ExecStart=/usr/local/bin/pam __pamd\n"));
        assert!(
            unit.contains("Environment=PAM_MANAGER_STATE_DIR=/home/deploy/pam-apps/shop/state\n")
        );
        assert!(unit.contains(
            "Environment=PAM_MANAGER_RUNTIME_DIR=/home/deploy/pam-apps/shop/state/run\n"
        ));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(systemd_unit("shop", "relative", "/usr/bin/pam").is_err());
        assert!(systemd_unit("shop", "/srv/a b", "/usr/bin/pam").is_err());

        let script = init_script(&deploy(), &unit, "APP_KEY=x\n");
        assert!(script.contains("pam-shop.service"));
        assert!(script.contains("systemctl --user enable --now pam-shop.service"));
        assert!(script.contains("<<'PAM_UNIT'\n[Unit]"));
    }

    #[test]
    fn production_environment_has_a_key_and_secure_cookies() {
        let environment = production_environment_from(
            "APP_KEY=\nSESSION_SECURE_COOKIE=false\nPAM_ENV=local\nDB_CONNECTION=sqlite\n",
            || Ok("base64:key".to_owned()),
        )
        .unwrap();
        assert_eq!(
            environment,
            "APP_KEY=base64:key\nSESSION_SECURE_COOKIE=true\nDB_CONNECTION=sqlite\nPAM_ENV=production\n"
        );
        assert!(production_environment_from("PAM_ENV_FILE\n", || Ok(String::new())).is_err());
    }

    #[test]
    fn release_ids_sort_by_time() {
        assert_eq!(release_id(1_763_510_400), "20251119000000");
        assert!(release_id(1_763_510_401) > release_id(1_763_510_400));
    }
}
