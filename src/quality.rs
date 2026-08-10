use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::composer;
use crate::php::PhpRuntime;

pub fn format(
    executable: &OsStr,
    project: &Path,
    check: bool,
    paths: Vec<OsString>,
) -> Result<u8, String> {
    let Some((formatter, arguments)) = formatter_command(project, check, paths) else {
        return Err(
            "no supported formatter is installed; PAM recognizes pam-native-format, Laravel Pint, PHP-CS-Fixer, and PHPCBF"
                .to_owned(),
        );
    };
    run_php_tool(executable, project, &formatter, &arguments)
}

pub fn lint(executable: &OsStr, project: &Path) -> Result<u8, String> {
    let mut gates = 0_u8;
    if let Some((formatter, arguments)) = formatter_command(project, true, Vec::new()) {
        gates += 1;
        let status = run_php_tool(executable, project, &formatter, &arguments)?;
        if status != 0 {
            return Ok(status);
        }
    }
    if project.join("composer.json").is_file() {
        gates += 1;
        let status = in_project(project, || {
            composer::run(
                executable,
                &[
                    OsString::from("validate"),
                    OsString::from("--strict"),
                    OsString::from("--no-interaction"),
                ],
            )
        })?;
        if status != 0 {
            return Ok(status);
        }
    }
    let phpstan = project.join("vendor/bin/phpstan");
    if phpstan.is_file() {
        return run_php_tool(
            executable,
            project,
            &phpstan,
            &[
                OsString::from("analyse"),
                OsString::from("--no-progress"),
                OsString::from("--memory-limit=1G"),
            ],
        );
    }
    if gates == 0 {
        println!(
            "No optional formatter or static analyzer is installed; project contracts passed."
        );
    } else {
        println!(
            "Quality contracts passed. PHPStan is not installed; static analysis was skipped."
        );
    }
    Ok(0)
}

pub fn outdated(executable: &OsStr, project: &Path, direct: bool) -> Result<u8, String> {
    in_project(project, || {
        let mut arguments = vec![OsString::from("outdated")];
        if direct {
            arguments.push(OsString::from("--direct"));
        }
        composer::run(executable, &arguments)
    })
}

fn formatter_command(
    project: &Path,
    check: bool,
    paths: Vec<OsString>,
) -> Option<(PathBuf, Vec<OsString>)> {
    let binary = |name: &str| project.join("vendor/bin").join(name);
    let native = binary("pam-native-format");
    if native.is_file() {
        let mut arguments = Vec::new();
        if check {
            arguments.push(OsString::from("--check"));
        }
        arguments.extend(if paths.is_empty() {
            vec![OsString::from("src")]
        } else {
            paths
        });
        return Some((native, arguments));
    }
    let pint = binary("pint");
    if pint.is_file() {
        let mut arguments = Vec::new();
        if check {
            arguments.push(OsString::from("--test"));
        }
        arguments.extend(paths);
        return Some((pint, arguments));
    }
    let fixer = binary("php-cs-fixer");
    if fixer.is_file() {
        let mut arguments = vec![OsString::from("fix")];
        if check {
            arguments.extend([OsString::from("--dry-run"), OsString::from("--diff")]);
        }
        arguments.extend(if paths.is_empty() {
            vec![OsString::from(".")]
        } else {
            paths
        });
        return Some((fixer, arguments));
    }
    let phpcbf = binary(if check { "phpcs" } else { "phpcbf" });
    if phpcbf.is_file() {
        return Some((
            phpcbf,
            if paths.is_empty() {
                vec![OsString::from(".")]
            } else {
                paths
            },
        ));
    }
    None
}

fn run_php_tool(
    executable: &OsStr,
    project: &Path,
    tool: &Path,
    arguments: &[OsString],
) -> Result<u8, String> {
    in_project(project, || {
        // Composer bin proxies and PHAR tools own their bootstrap. Preloading the
        // application's autoloader can collide with scoped dependencies (PHPStan
        // is a common example), so tooling runs in an isolated Embed lifecycle.
        let mut runtime = PhpRuntime::initialize_tool(executable, tool, arguments)?;
        runtime.execute_file(tool)
    })
}

fn in_project<T>(
    project: &Path,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let original = std::env::current_dir().map_err(|error| error.to_string())?;
    std::env::set_current_dir(project)
        .map_err(|error| format!("cannot enter {}: {error}", project.display()))?;
    let result = operation();
    std::env::set_current_dir(&original)
        .map_err(|error| format!("cannot restore {}: {error}", original.display()))?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temporary(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pam-quality-{name}-{}", std::process::id()))
    }

    #[test]
    fn selects_contextual_formatters_and_check_flags() {
        let root = temporary("formatters");
        fs::create_dir_all(root.join("vendor/bin")).unwrap();
        fs::write(root.join("vendor/bin/pint"), "<?php\n").unwrap();
        let (tool, arguments) = formatter_command(&root, true, Vec::new()).unwrap();
        assert_eq!(tool, root.join("vendor/bin/pint"));
        assert_eq!(arguments, vec![OsString::from("--test")]);

        fs::write(root.join("vendor/bin/pam-native-format"), "<?php\n").unwrap();
        let (tool, arguments) = formatter_command(&root, false, Vec::new()).unwrap();
        assert_eq!(tool, root.join("vendor/bin/pam-native-format"));
        assert_eq!(arguments, vec![OsString::from("src")]);
        fs::remove_dir_all(root).unwrap();
    }
}
