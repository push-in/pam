use std::collections::{BTreeSet, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::package_coordinates;

const MANIFEST_NAME: &str = "pam-native.json";
const DEFAULT_PORT: u16 = 39_100;
const MAX_PROJECT_FILES: usize = 10_000;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PROJECT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DEV_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
const HOT_RELOAD_DEBOUNCE: Duration = Duration::from_millis(400);
const PLUGIN_PROTOCOL_VERSION: u32 = 1;
const PLUGIN_LOCK_VERSION: u32 = 1;
const PLUGIN_MANIFEST_MAX_BYTES: u64 = 1024 * 1024;
const RUNTIME_LOCK_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildMode {
    Debug = 1,
    Release = 2,
}

impl BuildMode {
    fn gradle_task(self) -> &'static str {
        match self {
            Self::Debug => "assembleDebug",
            Self::Release => "assembleRelease",
        }
    }

    fn directory(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum AndroidAbi {
    Arm64 = 1,
    X86_64 = 2,
}

impl AndroidAbi {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "arm64-v8a" | "arm64" | "aarch64" => Ok(Self::Arm64),
            "x86_64" | "x64" => Ok(Self::X86_64),
            _ => Err(format!(
                "unsupported Android ABI {value:?}; expected arm64-v8a or x86_64"
            )),
        }
    }

    fn android(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64-v8a",
            Self::X86_64 => "x86_64",
        }
    }

    fn rust_target(self) -> &'static str {
        match self {
            Self::Arm64 => "aarch64-linux-android",
            Self::X86_64 => "x86_64-linux-android",
        }
    }

    fn clang(self) -> &'static str {
        match self {
            Self::Arm64 => "aarch64-linux-android26-clang",
            Self::X86_64 => "x86_64-linux-android26-clang",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeManifest {
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
    version: u32,
    application_id: String,
    name: String,
    entry: PathBuf,
    #[serde(default)]
    runtime: RuntimeRequest,
    #[serde(default = "default_version_code")]
    version_code: u32,
    #[serde(default = "default_version_name")]
    version_name: String,
    #[serde(default)]
    android: AndroidOptions,
    #[serde(default)]
    modules: Vec<NativeModule>,
    #[serde(default)]
    views: Vec<NativeView>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeRequest {
    #[serde(default = "default_php_series")]
    php: String,
    #[serde(default = "default_runtime_channel")]
    channel: String,
}

impl Default for RuntimeRequest {
    fn default() -> Self {
        Self {
            php: default_php_series(),
            channel: default_runtime_channel(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeCatalog {
    schema_version: u32,
    default: String,
    channels: std::collections::BTreeMap<String, String>,
    releases: std::collections::BTreeMap<String, RuntimeRelease>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeRelease {
    php_version: String,
    runtime_revision: u32,
    source_url: String,
    source_sha256: String,
    android_api: u32,
    ndk_version: String,
    extensions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeLock<'a> {
    schema_version: u32,
    runtime_id: &'a str,
    php_version: &'a str,
    runtime_revision: u32,
    channel: &'a str,
    source_sha256: &'a str,
    android_api: u32,
    ndk_version: &'a str,
    extensions: &'a [String],
}

struct ResolvedRuntime {
    id: String,
    release: RuntimeRelease,
    root: PathBuf,
}

fn default_php_series() -> String {
    "8.5".to_owned()
}

fn default_runtime_channel() -> String {
    "stable".to_owned()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AndroidOptions {
    #[serde(default = "default_min_sdk")]
    min_sdk: u32,
    #[serde(default = "default_target_sdk")]
    target_sdk: u32,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    deep_links: Vec<AndroidDeepLink>,
}

impl Default for AndroidOptions {
    fn default() -> Self {
        Self {
            min_sdk: default_min_sdk(),
            target_sdk: default_target_sdk(),
            permissions: Vec::new(),
            deep_links: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AndroidDeepLink {
    scheme: String,
    #[serde(default)]
    host: Option<String>,
    #[serde(default)]
    path_prefix: Option<String>,
    #[serde(default)]
    auto_verify: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeModule {
    name: String,
    class: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeView {
    name: String,
    class: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ComposerInstalled {
    Object { packages: Vec<ComposerPackage> },
    List(Vec<ComposerPackage>),
}

impl ComposerInstalled {
    fn packages(self) -> Vec<ComposerPackage> {
        match self {
            Self::Object { packages } | Self::List(packages) => packages,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ComposerPackage {
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    install_path: Option<PathBuf>,
    #[serde(default)]
    extra: ComposerExtra,
}

#[derive(Debug, Default, Deserialize)]
struct ComposerExtra {
    #[serde(rename = "pam-native")]
    pam_native: Option<ComposerPamNative>,
}

#[derive(Debug, Deserialize)]
struct ComposerPamNative {
    #[serde(default)]
    plugin: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginManifest {
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
    version: u32,
    protocol: u32,
    pam_native: PluginCompatibility,
    #[serde(default)]
    php: PluginPhp,
    #[serde(default)]
    android: PluginAndroid,
    #[serde(default)]
    modules: Vec<NativeModule>,
    #[serde(default)]
    views: Vec<NativeView>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginCompatibility {
    minimum: String,
    maximum_exclusive: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginPhp {
    #[serde(default)]
    provider: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginAndroid {
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default = "default_min_sdk")]
    min_sdk: u32,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default)]
    repositories: Vec<String>,
    #[serde(default)]
    dependencies: Vec<String>,
    #[serde(default)]
    local_aars: Vec<PathBuf>,
    #[serde(default)]
    source_dirs: Vec<PathBuf>,
    #[serde(default)]
    resource_dirs: Vec<PathBuf>,
    #[serde(default)]
    asset_dirs: Vec<PathBuf>,
    #[serde(default)]
    jni_lib_dirs: Vec<PathBuf>,
    #[serde(default)]
    manifest: Option<PathBuf>,
    #[serde(default)]
    consumer_rules: Option<PathBuf>,
}

#[derive(Debug)]
struct NativePlugin {
    package: String,
    package_version: String,
    root: PathBuf,
    descriptor: PathBuf,
    descriptor_digest: String,
    manifest: PluginManifest,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginLock<'a> {
    version: u32,
    protocol: u32,
    pam_native: &'a str,
    plugins: Vec<PluginLockEntry<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginLockEntry<'a> {
    package: &'a str,
    package_version: &'a str,
    descriptor_sha256: &'a str,
    php_provider: Option<&'a str>,
    modules: Vec<&'a str>,
    views: Vec<&'a str>,
    android_dependencies: Vec<&'a str>,
}

fn default_min_sdk() -> u32 {
    26
}

fn default_target_sdk() -> u32 {
    36
}

fn default_version_code() -> u32 {
    1
}

fn default_version_name() -> String {
    "0.1.0".to_owned()
}

struct Project {
    root: PathBuf,
    manifest: NativeManifest,
    plugins: Vec<NativePlugin>,
}

struct MobileOptions {
    project: PathBuf,
    mode: BuildMode,
    abis: Vec<AndroidAbi>,
}

pub fn run(arguments: Vec<OsString>) -> Result<u8, String> {
    let mut arguments = arguments.into_iter();
    let Some(command) = arguments.next() else {
        print_usage();
        return Ok(0);
    };
    match command.to_string_lossy().as_ref() {
        "--help" | "-h" | "help" => {
            print_usage();
            Ok(0)
        }
        "doctor" => doctor(parse_project_only(arguments)?),
        "prepare" => {
            let options = parse_options(arguments, false)?;
            let project = load_project(&options.project)?;
            let native_home = native_home()?;
            prepare(&project, &native_home, &options.abis)?;
            println!(
                "Prepared {} for Android ({})",
                project.manifest.name,
                display_abis(&options.abis)
            );
            Ok(0)
        }
        "codegen" => {
            let project = load_project(&parse_project_only(arguments)?)?;
            let native_home = native_home()?;
            let runtime = resolve_runtime(&project, &pam_home()?)?;
            let workspace = sync_android_host(&project, &native_home)?;
            configure_android(
                &project,
                &native_home,
                &runtime,
                &workspace,
                &default_abis(),
            )?;
            generate_modules(&project, &workspace)?;
            generate_views(&project, &workspace)?;
            println!("Generated Android bindings for {}", project.manifest.name);
            Ok(0)
        }
        "build" => {
            let options = parse_options(arguments, true)?;
            build(options).map(|_| 0)
        }
        "run" => {
            let mut options = parse_options(arguments, true)?;
            if options.abis == default_abis() {
                options.abis = vec![connected_abi()?];
            }
            let apk = build(options)?;
            install_and_launch(&apk.project, &apk.path, apk.mode)?;
            Ok(0)
        }
        "dev" => {
            let mut options = parse_options(arguments, false)?;
            options.mode = BuildMode::Debug;
            if options.abis == default_abis() {
                options.abis = vec![connected_abi()?];
            }
            dev(options)
        }
        "benchmark" => benchmark(parse_project_only(arguments)?),
        "profile" => baseline_profile(parse_project_only(arguments)?),
        "devtools" => toggle_devtools(parse_project_only(arguments)?),
        "plugin:list" => list_plugins(parse_project_only(arguments)?),
        "plugin:doctor" => doctor_plugins(parse_project_only(arguments)?),
        "runtime:list" => list_runtimes(parse_project_only(arguments)?),
        "runtime:info" => runtime_info(parse_project_only(arguments)?),
        "runtime:use" => runtime_use(parse_runtime_use(arguments)?),
        "runtime:update" => runtime_update(parse_project_only(arguments)?),
        "make:screen" => generate_screen(parse_generator(arguments)?),
        "make:component" => generate_component(parse_generator(arguments)?),
        "make:flow" => generate_flow(parse_generator(arguments)?),
        "make:native-view" => generate_native_view(parse_generator(arguments)?),
        unknown => Err(format!(
            "unknown mobile command {unknown:?}; run `pam mobile --help`"
        )),
    }
}

struct RuntimeUseOptions {
    php: String,
    project: PathBuf,
}

fn parse_runtime_use(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<RuntimeUseOptions, String> {
    let php = arguments
        .next()
        .ok_or_else(|| "`pam mobile runtime:use` requires 8.4 or 8.5".to_owned())?
        .into_string()
        .map_err(|_| "PHP runtime version must be valid UTF-8".to_owned())?;
    let project = arguments.next().unwrap_or_else(|| OsString::from("."));
    if let Some(extra) = arguments.next() {
        return Err(format!(
            "unexpected runtime argument {}",
            extra.to_string_lossy()
        ));
    }
    Ok(RuntimeUseOptions {
        php,
        project: PathBuf::from(project),
    })
}

struct GeneratorOptions {
    name: String,
    project: PathBuf,
}

fn parse_generator(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<GeneratorOptions, String> {
    let name = arguments
        .next()
        .ok_or_else(|| "generator commands require a PascalCase name".to_owned())?
        .into_string()
        .map_err(|_| "generator names must be valid UTF-8".to_owned())?;
    if !valid_pascal_name(&name) {
        return Err(
            "generator names must start with an uppercase ASCII letter and contain only letters or digits"
                .to_owned(),
        );
    }
    let project = arguments.next().unwrap_or_else(|| OsString::from("."));
    if let Some(extra) = arguments.next() {
        return Err(format!(
            "unexpected generator argument {}",
            extra.to_string_lossy()
        ));
    }
    Ok(GeneratorOptions {
        name,
        project: PathBuf::from(project),
    })
}

fn valid_pascal_name(value: &str) -> bool {
    value.len() <= 80
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

fn kebab_case(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(value.len() + 8);
    for (index, character) in characters.iter().copied().enumerate() {
        if character.is_ascii_uppercase() {
            let previous = index.checked_sub(1).and_then(|value| characters.get(value));
            let next = characters.get(index + 1);
            if index > 0
                && (previous
                    .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
                    || (previous.is_some_and(|value| value.is_ascii_uppercase())
                        && next.is_some_and(|value| value.is_ascii_lowercase())))
            {
                output.push('-');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn parse_project_only(mut arguments: impl Iterator<Item = OsString>) -> Result<PathBuf, String> {
    let project = arguments.next().unwrap_or_else(|| OsString::from("."));
    if let Some(extra) = arguments.next() {
        return Err(format!(
            "unexpected mobile argument {}",
            extra.to_string_lossy()
        ));
    }
    Ok(PathBuf::from(project))
}

fn parse_options(
    mut arguments: impl Iterator<Item = OsString>,
    allow_release: bool,
) -> Result<MobileOptions, String> {
    let mut project = PathBuf::from(".");
    let mut positional = false;
    let mut mode = BuildMode::Debug;
    let mut abis = Vec::new();
    while let Some(argument) = arguments.next() {
        match argument.to_string_lossy().as_ref() {
            "--release" if allow_release => mode = BuildMode::Release,
            "--release" => return Err("`pam mobile dev` only supports debug builds".to_owned()),
            "--abi" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--abi requires arm64-v8a or x86_64".to_owned())?;
                let abi = AndroidAbi::parse(&value.to_string_lossy())?;
                if !abis.contains(&abi) {
                    abis.push(abi);
                }
            }
            option if option.starts_with('-') => {
                return Err(format!("unknown mobile option {option}"));
            }
            _ if !positional => {
                project = PathBuf::from(argument);
                positional = true;
            }
            _ => return Err("mobile commands accept at most one project directory".to_owned()),
        }
    }
    if abis.is_empty() {
        abis = default_abis();
    }
    Ok(MobileOptions {
        project,
        mode,
        abis,
    })
}

fn default_abis() -> Vec<AndroidAbi> {
    vec![AndroidAbi::Arm64, AndroidAbi::X86_64]
}

fn load_project(path: &Path) -> Result<Project, String> {
    let root = fs::canonicalize(path)
        .map_err(|error| format!("cannot resolve mobile project {}: {error}", path.display()))?;
    if !root.is_dir() {
        return Err(format!(
            "mobile project {} is not a directory",
            root.display()
        ));
    }
    let manifest_path = root.join(MANIFEST_NAME);
    let contents = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest: NativeManifest = serde_json::from_str(&contents)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    validate_manifest(&root, &manifest)?;
    let plugins = discover_plugins(&root, &manifest)?;
    Ok(Project {
        root,
        manifest,
        plugins,
    })
}

fn validate_manifest(root: &Path, manifest: &NativeManifest) -> Result<(), String> {
    if manifest.version != 1 {
        return Err(format!(
            "unsupported Pam Native manifest version {}; expected 1",
            manifest.version
        ));
    }
    if !matches!(manifest.runtime.php.as_str(), "8.4" | "8.5") {
        return Err("runtime.php must be 8.4 or 8.5".to_owned());
    }
    if manifest.runtime.channel != "stable" {
        return Err("runtime.channel currently supports only \"stable\"".to_owned());
    }
    if !valid_application_id(&manifest.application_id) {
        return Err("applicationId must be a dot-separated Java package name".to_owned());
    }
    if manifest.name.trim().is_empty() || manifest.name.chars().count() > 80 {
        return Err("application name must contain between 1 and 80 characters".to_owned());
    }
    if manifest.version_code == 0 {
        return Err("versionCode must be a positive integer".to_owned());
    }
    if manifest.version_name.is_empty()
        || manifest.version_name.len() > 64
        || manifest.version_name.contains(['\n', '\r', '\0'])
    {
        return Err("versionName must be a safe string no longer than 64 bytes".to_owned());
    }
    if manifest.android.min_sdk < 26 || manifest.android.min_sdk > manifest.android.target_sdk {
        return Err("Android minSdk must be at least 26 and no greater than targetSdk".to_owned());
    }
    if manifest.android.target_sdk > 36 {
        return Err("this Pam Native release supports targetSdk up to 36".to_owned());
    }
    validate_relative_path(&manifest.entry)?;
    if !root.join(&manifest.entry).is_file() {
        return Err(format!(
            "mobile entry {} does not exist",
            root.join(&manifest.entry).display()
        ));
    }
    if !root.join("vendor/autoload.php").is_file() {
        return Err("vendor/autoload.php is missing; run `pam composer install` first".to_owned());
    }
    let mut module_names = HashSet::new();
    for module in &manifest.modules {
        if !valid_module_name(&module.name) {
            return Err(format!(
                "native module name {:?} must use lowercase letters, digits, dots, _ or -",
                module.name
            ));
        }
        if !valid_class_name(&module.class) {
            return Err(format!("invalid Kotlin class name {:?}", module.class));
        }
        if !module_names.insert(&module.name) {
            return Err(format!("duplicate native module name {:?}", module.name));
        }
    }
    let mut view_names = HashSet::new();
    for view in &manifest.views {
        if !valid_module_name(&view.name) {
            return Err(format!(
                "native view name {:?} must use lowercase letters, digits, dots, _ or -",
                view.name
            ));
        }
        if !valid_class_name(&view.class) {
            return Err(format!(
                "invalid native view factory class {:?}",
                view.class
            ));
        }
        if !view_names.insert(&view.name) {
            return Err(format!("duplicate native view name {:?}", view.name));
        }
    }
    for permission in &manifest.android.permissions {
        if !permission.starts_with("android.permission.")
            || !permission
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || value == '_' || value == '.')
        {
            return Err(format!("invalid Android permission {permission:?}"));
        }
    }
    for link in &manifest.android.deep_links {
        if !valid_uri_scheme(&link.scheme) {
            return Err(format!(
                "invalid Android deep-link scheme {:?}",
                link.scheme
            ));
        }
        if let Some(host) = &link.host
            && !valid_deep_link_host(host)
        {
            return Err(format!("invalid Android deep-link host {host:?}"));
        }
        if let Some(path) = &link.path_prefix
            && (!path.starts_with('/')
                || path.len() > 512
                || path.contains(['\n', '\r', '\0', '"', '<', '>', '&']))
        {
            return Err(format!(
                "Android deep-link pathPrefix {path:?} must be an absolute safe path"
            ));
        }
        if link.auto_verify && (link.scheme != "https" || link.host.is_none()) {
            return Err(
                "Android autoVerify deep links require scheme \"https\" and a host".to_owned(),
            );
        }
    }
    Ok(())
}

fn valid_uri_scheme(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|value| value.is_ascii_alphabetic())
        && characters.all(|value| value.is_ascii_alphanumeric() || matches!(value, '+' | '-' | '.'))
        && value.len() <= 64
}

fn valid_deep_link_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
        && !value.starts_with('.')
        && !value.starts_with('-')
        && !value.ends_with('.')
        && !value.ends_with('-')
}

fn discover_plugins(root: &Path, app: &NativeManifest) -> Result<Vec<NativePlugin>, String> {
    let composer_directory = root.join("vendor/composer");
    let installed_path = composer_directory.join("installed.json");
    if !installed_path.is_file() {
        return Ok(Vec::new());
    }
    let metadata = installed_path
        .metadata()
        .map_err(|error| format!("cannot inspect {}: {error}", installed_path.display()))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "Composer metadata {} exceeds the 8 MiB safety limit",
            installed_path.display()
        ));
    }
    let contents = fs::read_to_string(&installed_path)
        .map_err(|error| format!("cannot read {}: {error}", installed_path.display()))?;
    let installed: ComposerInstalled = serde_json::from_str(&contents)
        .map_err(|error| format!("invalid {}: {error}", installed_path.display()))?;
    let composer_directory = fs::canonicalize(&composer_directory).map_err(|error| {
        format!(
            "cannot resolve Composer directory {}: {error}",
            composer_directory.display()
        )
    })?;
    let vendor_directory = fs::canonicalize(root.join("vendor"))
        .map_err(|error| format!("cannot resolve Composer vendor directory: {error}"))?;
    let packages = installed.packages();
    let current_version_text = packages
        .iter()
        .find(|package| package.name == package_coordinates::NATIVE)
        .map(|package| package.version.clone())
        .ok_or_else(|| {
            format!(
                "Composer package {} is required for a Pam Native mobile project",
                package_coordinates::NATIVE,
            )
        })?;
    let current_version = parse_installed_sdk_version(&current_version_text)?;
    let mut plugins = Vec::new();

    for package in packages {
        let Some(extra) = package.extra.pam_native else {
            continue;
        };
        let Some(plugin_descriptor) = extra.plugin else {
            continue;
        };
        let install_path = package.install_path.ok_or_else(|| {
            format!(
                "Pam Native plugin package {} has no Composer install-path",
                package.name
            )
        })?;
        if !valid_composer_package(&package.name) {
            return Err(format!(
                "Pam Native plugin package name {:?} is invalid",
                package.name
            ));
        }
        validate_composer_install_path(&install_path)?;
        validate_relative_path(&plugin_descriptor)?;
        let package_root =
            fs::canonicalize(composer_directory.join(&install_path)).map_err(|error| {
                format!(
                    "cannot resolve Composer package {} at {}: {error}",
                    package.name,
                    composer_directory.join(&install_path).display()
                )
            })?;
        if !package_root.is_dir() {
            return Err(format!(
                "Composer package {} install path is not a directory",
                package.name
            ));
        }
        if !package_root.starts_with(&vendor_directory) {
            return Err(format!(
                "Composer package {} install path escapes the project vendor directory; \
                 path repositories used by mobile apps must set options.symlink to false",
                package.name,
            ));
        }
        let descriptor =
            fs::canonicalize(package_root.join(&plugin_descriptor)).map_err(|error| {
                format!(
                    "cannot resolve Pam Native plugin descriptor for {}: {error}",
                    package.name
                )
            })?;
        if !descriptor.starts_with(&package_root) {
            return Err(format!(
                "Pam Native plugin descriptor for {} escapes its Composer package",
                package.name
            ));
        }
        let descriptor_metadata = descriptor
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", descriptor.display()))?;
        if !descriptor_metadata.is_file() || descriptor_metadata.len() > PLUGIN_MANIFEST_MAX_BYTES {
            return Err(format!(
                "Pam Native plugin descriptor for {} must be a file no larger than 1 MiB",
                package.name
            ));
        }
        let descriptor_bytes = fs::read(&descriptor)
            .map_err(|error| format!("cannot read {}: {error}", descriptor.display()))?;
        let plugin_manifest: PluginManifest = serde_json::from_slice(&descriptor_bytes)
            .map_err(|error| format!("invalid {}: {error}", descriptor.display()))?;
        validate_plugin_manifest(
            &package.name,
            &package_root,
            &plugin_manifest,
            app,
            current_version,
            &current_version_text,
        )?;
        plugins.push(NativePlugin {
            package: package.name,
            package_version: package.version,
            root: package_root,
            descriptor,
            descriptor_digest: format!("{:x}", Sha256::digest(&descriptor_bytes)),
            manifest: plugin_manifest,
        });
    }

    plugins.sort_by(|left, right| left.package.cmp(&right.package));
    validate_plugin_bindings(app, &plugins)?;
    Ok(plugins)
}

fn validate_plugin_manifest(
    package: &str,
    root: &Path,
    manifest: &PluginManifest,
    app: &NativeManifest,
    current_version: (u32, u32, u32),
    current_version_text: &str,
) -> Result<(), String> {
    if manifest.version != 1 {
        return Err(format!(
            "plugin {package} uses unsupported manifest version {}; expected 1",
            manifest.version
        ));
    }
    if manifest.protocol != PLUGIN_PROTOCOL_VERSION {
        return Err(format!(
            "plugin {package} requires protocol {}, but this SDK implements {}",
            manifest.protocol, PLUGIN_PROTOCOL_VERSION
        ));
    }
    let minimum = parse_release_version(&manifest.pam_native.minimum)
        .map_err(|error| format!("plugin {package} has invalid pamNative.minimum: {error}"))?;
    let maximum =
        parse_release_version(&manifest.pam_native.maximum_exclusive).map_err(|error| {
            format!("plugin {package} has invalid pamNative.maximumExclusive: {error}")
        })?;
    if minimum >= maximum {
        return Err(format!(
            "plugin {package} compatibility minimum must be lower than maximumExclusive"
        ));
    }
    if current_version < minimum || current_version >= maximum {
        return Err(format!(
            "plugin {package} supports Pam Native {} through {}, exclusive; installed SDK is {}",
            manifest.pam_native.minimum,
            manifest.pam_native.maximum_exclusive,
            current_version_text,
        ));
    }
    if let Some(provider) = &manifest.php.provider
        && !valid_php_class_name(provider)
    {
        return Err(format!(
            "plugin {package} has invalid PHP provider {provider:?}"
        ));
    }
    if manifest.android.min_sdk < 26 || manifest.android.min_sdk > app.android.min_sdk {
        return Err(format!(
            "plugin {package} requires Android minSdk {}, but the app uses {}; plugin minSdk must be between 26 and the app minSdk",
            manifest.android.min_sdk, app.android.min_sdk
        ));
    }
    if let Some(namespace) = &manifest.android.namespace
        && !valid_application_id(namespace)
    {
        return Err(format!(
            "plugin {package} has invalid Android namespace {namespace:?}"
        ));
    }
    for permission in &manifest.android.permissions {
        if !valid_android_permission(permission) {
            return Err(format!(
                "plugin {package} has invalid Android permission {permission:?}"
            ));
        }
    }
    for repository in &manifest.android.repositories {
        if !repository.starts_with("https://")
            || repository.contains(['\n', '\r', '\0', '"'])
            || repository.len() > 2048
        {
            return Err(format!(
                "plugin {package} repository URLs must use HTTPS and contain no control characters"
            ));
        }
    }
    for dependency in &manifest.android.dependencies {
        if !valid_maven_coordinate(dependency) {
            return Err(format!(
                "plugin {package} has invalid Maven dependency {dependency:?}"
            ));
        }
    }
    for path in plugin_paths(&manifest.android) {
        validate_plugin_path(package, root, path)?;
    }
    for binding in &manifest.modules {
        validate_binding(package, "module", &binding.name, &binding.class)?;
    }
    for binding in &manifest.views {
        validate_binding(package, "view", &binding.name, &binding.class)?;
    }
    Ok(())
}

fn validate_plugin_bindings(app: &NativeManifest, plugins: &[NativePlugin]) -> Result<(), String> {
    let mut modules = app
        .modules
        .iter()
        .map(|module| module.name.clone())
        .collect::<HashSet<_>>();
    let mut views = app
        .views
        .iter()
        .map(|view| view.name.clone())
        .collect::<HashSet<_>>();
    for plugin in plugins {
        for module in &plugin.manifest.modules {
            if !modules.insert(module.name.clone()) {
                return Err(format!(
                    "duplicate native module name {:?} introduced by plugin {}",
                    module.name, plugin.package
                ));
            }
        }
        for view in &plugin.manifest.views {
            if !views.insert(view.name.clone()) {
                return Err(format!(
                    "duplicate native view name {:?} introduced by plugin {}",
                    view.name, plugin.package
                ));
            }
        }
    }
    Ok(())
}

fn plugin_paths(android: &PluginAndroid) -> Vec<&PathBuf> {
    let mut paths = Vec::new();
    paths.extend(&android.local_aars);
    paths.extend(&android.source_dirs);
    paths.extend(&android.resource_dirs);
    paths.extend(&android.asset_dirs);
    paths.extend(&android.jni_lib_dirs);
    paths.extend(android.manifest.iter());
    paths.extend(android.consumer_rules.iter());
    paths
}

fn validate_plugin_path(package: &str, root: &Path, path: &Path) -> Result<(), String> {
    validate_relative_path(path)
        .map_err(|error| format!("plugin {package} path {}: {error}", path.display()))?;
    let resolved = fs::canonicalize(root.join(path)).map_err(|error| {
        format!(
            "plugin {package} path {} cannot be resolved: {error}",
            path.display()
        )
    })?;
    if !resolved.starts_with(root) {
        return Err(format!(
            "plugin {package} path {} escapes its Composer package",
            path.display()
        ));
    }
    Ok(())
}

fn validate_binding(package: &str, kind: &str, name: &str, class: &str) -> Result<(), String> {
    if !valid_module_name(name) {
        return Err(format!(
            "plugin {package} {kind} name {name:?} must use lowercase letters, digits, dots, _ or -"
        ));
    }
    if !valid_class_name(class) {
        return Err(format!(
            "plugin {package} has invalid Kotlin {kind} class {class:?}"
        ));
    }
    Ok(())
}

fn parse_release_version(value: &str) -> Result<(u32, u32, u32), String> {
    let release = value
        .split_once(['-', '+'])
        .map_or(value, |(release, _)| release);
    let numbers = release
        .split('.')
        .map(|part| {
            part.parse::<u32>()
                .map_err(|_| format!("{value:?} is not a semantic release version"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if numbers.len() != 3 {
        return Err(format!("{value:?} must contain major.minor.patch"));
    }
    Ok((numbers[0], numbers[1], numbers[2]))
}

fn parse_installed_sdk_version(value: &str) -> Result<(u32, u32, u32), String> {
    if let Some(release) = value.strip_suffix("-dev") {
        let parts = release.split('.').collect::<Vec<_>>();
        if parts.len() == 3 && matches!(parts[2], "x" | "*") {
            let major = parts[0]
                .parse::<u32>()
                .map_err(|_| format!("{value:?} is not a supported Composer SDK version"))?;
            let minor = parts[1]
                .parse::<u32>()
                .map_err(|_| format!("{value:?} is not a supported Composer SDK version"))?;
            return Ok((major, minor, 0));
        }
    }

    parse_release_version(value)
        .map_err(|_| format!("{value:?} is not a supported Composer SDK version"))
}

fn valid_composer_package(value: &str) -> bool {
    let mut parts = value.split('/');
    parts.next().is_some_and(valid_composer_part)
        && parts.next().is_some_and(valid_composer_part)
        && parts.next().is_none()
}

fn valid_composer_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

fn valid_php_class_name(value: &str) -> bool {
    !value.starts_with('\\')
        && value.split('\\').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

fn valid_android_permission(value: &str) -> bool {
    value
        .strip_prefix("android.permission.")
        .is_some_and(|permission| {
            !permission.is_empty()
                && value.len() <= 160
                && permission.bytes().all(|byte| {
                    byte.is_ascii_uppercase()
                        || byte.is_ascii_digit()
                        || byte == b'.'
                        || byte == b'_'
                })
        })
}

fn valid_maven_coordinate(value: &str) -> bool {
    let parts = value.split(':').collect::<Vec<_>>();
    (3..=4).contains(&parts.len())
        && value.len() <= 300
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-+[](),".contains(&byte))
        })
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("unsafe project-relative path {}", path.display()));
    }
    Ok(())
}

fn validate_composer_install_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::RootDir | Component::Prefix(_)))
    {
        return Err(format!("unsafe Composer install path {}", path.display()));
    }
    Ok(())
}

fn valid_application_id(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() >= 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphabetic())
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
}

fn valid_module_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '.' | '_' | '-')
        })
}

fn valid_class_name(value: &str) -> bool {
    value.split('.').count() >= 2
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
}

fn native_home() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(configured) = std::env::var_os("PAM_NATIVE_HOME") {
        candidates.push(PathBuf::from(configured));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pam-native"));
    if let Ok(executable) = std::env::current_exe()
        && let Some(binary) = executable.parent()
    {
        candidates.push(binary.join("../share/pam/native"));
        candidates.push(binary.join("../lib/pam/native"));
    }
    candidates
        .into_iter()
        .find_map(|candidate| {
            let resolved = fs::canonicalize(candidate).ok()?;
            resolved
                .join("android/settings.gradle.kts")
                .is_file()
                .then_some(resolved)
        })
        .ok_or_else(|| {
            "Pam Native SDK was not found; set PAM_NATIVE_HOME to its verified installation"
                .to_owned()
        })
}

fn pam_home() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(configured) = std::env::var_os("PAM_HOME") {
        candidates.push(PathBuf::from(configured));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    if let Ok(executable) = std::env::current_exe()
        && let Some(binary) = executable.parent()
    {
        candidates.push(binary.join("../share/pam"));
        candidates.push(binary.join("../lib/pam"));
    }
    candidates
        .into_iter()
        .find_map(|candidate| {
            let resolved = fs::canonicalize(candidate).ok()?;
            resolved
                .join("runtime/catalog.json")
                .is_file()
                .then_some(resolved)
        })
        .ok_or_else(|| "PAM runtime catalog was not found; set PAM_HOME".to_owned())
}

fn load_runtime_catalog(pam_home: &Path) -> Result<RuntimeCatalog, String> {
    let path = pam_home.join("runtime/catalog.json");
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read runtime catalog {}: {error}", path.display()))?;
    let catalog: RuntimeCatalog = serde_json::from_str(&contents)
        .map_err(|error| format!("invalid runtime catalog {}: {error}", path.display()))?;
    if catalog.schema_version != 1 {
        return Err(format!(
            "unsupported runtime catalog schema {}; expected 1",
            catalog.schema_version
        ));
    }
    if !catalog.channels.contains_key(&catalog.default) {
        return Err("runtime catalog default does not name a channel".to_owned());
    }
    Ok(catalog)
}

fn resolve_runtime(project: &Project, pam_home: &Path) -> Result<ResolvedRuntime, String> {
    let catalog = load_runtime_catalog(pam_home)?;
    let id = catalog
        .channels
        .get(&project.manifest.runtime.php)
        .ok_or_else(|| {
            format!(
                "PHP {} has no {} runtime in this Pam Native SDK",
                project.manifest.runtime.php, project.manifest.runtime.channel
            )
        })?
        .clone();
    let release = catalog
        .releases
        .get(&id)
        .ok_or_else(|| format!("runtime catalog points to missing release {id}"))?
        .clone();
    Ok(ResolvedRuntime {
        root: pam_home.join("runtime/android").join(&id),
        id,
        release,
    })
}

fn write_runtime_lock(project: &Project, runtime: &ResolvedRuntime) -> Result<(), String> {
    let lock = RuntimeLock {
        schema_version: RUNTIME_LOCK_VERSION,
        runtime_id: &runtime.id,
        php_version: &runtime.release.php_version,
        runtime_revision: runtime.release.runtime_revision,
        channel: &project.manifest.runtime.channel,
        source_sha256: &runtime.release.source_sha256,
        android_api: runtime.release.android_api,
        ndk_version: &runtime.release.ndk_version,
        extensions: &runtime.release.extensions,
    };
    let bytes = serde_json::to_vec_pretty(&lock)
        .map_err(|error| format!("cannot encode runtime lock: {error}"))?;
    write_atomic(
        &project.root.join(".pam-native/runtime.lock.json"),
        &[bytes, b"\n".to_vec()].concat(),
    )
}

fn list_runtimes(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let pam_home = pam_home()?;
    let catalog = load_runtime_catalog(&pam_home)?;
    println!("Pam Native PHP runtimes\n");
    for (series, id) in &catalog.channels {
        let release = catalog
            .releases
            .get(id)
            .ok_or_else(|| format!("runtime catalog points to missing release {id}"))?;
        let selected = series == &project.manifest.runtime.php;
        let installed = default_abis()
            .into_iter()
            .all(|abi| runtime_ready_at(&pam_home.join("runtime/android").join(id), abi));
        println!(
            "{} PHP {} · {} · {}",
            if selected { "*" } else { " " },
            release.php_version,
            id,
            if installed { "installed" } else { "not built" }
        );
    }
    Ok(0)
}

fn runtime_info(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let runtime = resolve_runtime(&project, &pam_home()?)?;
    println!("Pam Native runtime\n");
    println!("PHP:          {}", runtime.release.php_version);
    println!("Runtime:      {}", runtime.id);
    println!("Revision:     {}", runtime.release.runtime_revision);
    println!("Android API:  {}", runtime.release.android_api);
    println!("NDK:          {}", runtime.release.ndk_version);
    println!("Extensions:   {}", runtime.release.extensions.join(", "));
    println!("Location:     {}", runtime.root.display());
    Ok(0)
}

fn runtime_use(options: RuntimeUseOptions) -> Result<u8, String> {
    if !matches!(options.php.as_str(), "8.4" | "8.5") {
        return Err("PHP runtime must be 8.4 or 8.5".to_owned());
    }
    let root = fs::canonicalize(&options.project).map_err(|error| {
        format!(
            "cannot resolve mobile project {}: {error}",
            options.project.display()
        )
    })?;
    let path = root.join(MANIFEST_NAME);
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|error| format!("invalid {}: {error}", path.display()))?;
    manifest["runtime"] = serde_json::json!({
        "php": options.php,
        "channel": "stable"
    });
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("cannot serialize {}: {error}", path.display()))?;
    write_atomic(&path, &[bytes, b"\n".to_vec()].concat())?;
    let project = load_project(&root)?;
    let pam_home = pam_home()?;
    let runtime = resolve_runtime(&project, &pam_home)?;
    write_runtime_lock(&project, &runtime)?;
    println!(
        "Selected PHP {} ({}) for {}.",
        runtime.release.php_version, runtime.id, project.manifest.name
    );
    if !default_abis()
        .into_iter()
        .all(|abi| runtime_ready_at(&runtime.root, abi))
    {
        println!(
            "Build it with: {}/runtime-builder/android/build.sh --php {} all",
            pam_home.display(),
            project.manifest.runtime.php
        );
    }
    Ok(0)
}

fn runtime_update(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let runtime = resolve_runtime(&project, &pam_home()?)?;
    write_runtime_lock(&project, &runtime)?;
    println!(
        "Locked PHP {} to {}.",
        project.manifest.runtime.php, runtime.id
    );
    Ok(0)
}

fn doctor(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let native_home = native_home()?;
    let runtime = resolve_runtime(&project, &pam_home()?)?;
    let mut healthy = true;
    println!("Pam Native Android doctor\n");
    check("Native SDK", true, native_home.display().to_string());
    check(
        "Project",
        true,
        format!(
            "{} ({})",
            project.manifest.name, project.manifest.application_id
        ),
    );
    healthy &= command_exists("java");
    check(
        "Java 17+",
        command_exists("java"),
        tool_version("java", &["-version"]),
    );
    let missing_engines = default_abis()
        .into_iter()
        .filter(|abi| !engine_ready(&native_home, *abi))
        .collect::<Vec<_>>();
    if missing_engines.is_empty() {
        for abi in default_abis() {
            check(
                &format!("Native engine ({})", abi.android()),
                true,
                engine_library(&native_home, abi).display().to_string(),
            );
        }
    } else {
        healthy &= command_exists("cargo");
        check(
            "Rust",
            command_exists("cargo"),
            tool_version("rustc", &["--version"]),
        );
    }
    let sdk = android_sdk();
    healthy &= sdk.is_ok();
    check(
        "Android SDK",
        sdk.is_ok(),
        sdk.as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|error| error.clone()),
    );
    let ndk = sdk
        .as_ref()
        .map(|path| path.join("ndk/27.1.12297006"))
        .is_ok_and(|path| path.is_dir());
    healthy &= ndk;
    check("Android NDK 27.1", ndk, "27.1.12297006".to_owned());
    for abi in default_abis() {
        let ready = runtime_ready_at(&runtime.root, abi);
        healthy &= ready;
        check(
            &format!(
                "PHP {} runtime ({})",
                runtime.release.php_version,
                abi.android()
            ),
            ready,
            runtime.root.join(abi.android()).display().to_string(),
        );
    }
    if !missing_engines.is_empty() {
        let installed = installed_rust_targets().unwrap_or_default();
        for abi in missing_engines {
            let available = installed.contains(abi.rust_target());
            healthy &= available;
            check(
                &format!("Rust target ({})", abi.rust_target()),
                available,
                if available {
                    "installed".to_owned()
                } else {
                    format!("run: rustup target add {}", abi.rust_target())
                },
            );
        }
    }
    if healthy {
        println!("\nPam Native is ready to build Android applications.");
        Ok(0)
    } else {
        Err("Pam Native doctor found blocking Android requirements".to_owned())
    }
}

fn list_plugins(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    if project.plugins.is_empty() {
        println!("No Pam Native Composer plugins are installed.");
        return Ok(0);
    }
    println!(
        "Pam Native plugins · protocol {} · SDK {}\n",
        PLUGIN_PROTOCOL_VERSION,
        env!("CARGO_PKG_VERSION")
    );
    for plugin in &project.plugins {
        println!(
            "{} {} · {} module(s) · {} view(s){}",
            plugin.package,
            if plugin.package_version.is_empty() {
                "unknown"
            } else {
                &plugin.package_version
            },
            plugin.manifest.modules.len(),
            plugin.manifest.views.len(),
            plugin
                .manifest
                .php
                .provider
                .as_deref()
                .map(|provider| format!(" · provider {provider}"))
                .unwrap_or_default(),
        );
        println!("  {}", plugin.descriptor.display());
    }
    Ok(0)
}

fn doctor_plugins(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    println!("Pam Native plugin doctor\n");
    let composer_metadata = project.root.join("vendor/composer/installed.json");
    check(
        "Composer metadata",
        composer_metadata.is_file(),
        if composer_metadata.is_file() {
            composer_metadata.display().to_string()
        } else {
            "not generated; no Composer plugins can be discovered".to_owned()
        },
    );
    check(
        "Plugin protocol",
        true,
        format!("version {PLUGIN_PROTOCOL_VERSION}"),
    );
    check("Installed plugins", true, project.plugins.len().to_string());
    for plugin in &project.plugins {
        check(
            &plugin.package,
            true,
            format!(
                "{} · descriptor {}",
                if plugin.package_version.is_empty() {
                    "unknown"
                } else {
                    &plugin.package_version
                },
                &plugin.descriptor_digest[..16],
            ),
        );
    }
    println!("\nAll discovered plugins are compatible and safe to autolink.");
    Ok(0)
}

fn check(label: &str, okay: bool, detail: String) {
    println!(
        "[{}] {:<28} {}",
        if okay { "ok" } else { "missing" },
        label,
        detail.lines().next().unwrap_or_default()
    );
}

fn tool_version(command: &str, arguments: &[&str]) -> String {
    Command::new(command)
        .args(arguments)
        .output()
        .map(|output| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().is_empty() {
                stderr.trim().to_owned()
            } else {
                stdout.trim().to_owned()
            }
        })
        .unwrap_or_else(|error| error.to_string())
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn prepare(project: &Project, native_home: &Path, abis: &[AndroidAbi]) -> Result<PathBuf, String> {
    let runtime = resolve_runtime(project, &pam_home()?)?;
    write_runtime_lock(project, &runtime)?;
    let workspace = sync_android_host(project, native_home)?;
    configure_android(project, native_home, &runtime, &workspace, abis)?;
    generate_modules(project, &workspace)?;
    generate_views(project, &workspace)?;
    stage_project(project, &workspace)?;
    Ok(workspace)
}

fn sync_android_host(project: &Project, native_home: &Path) -> Result<PathBuf, String> {
    let source = native_home.join("android");
    let destination = project.root.join(".pam-native/android");
    fs::create_dir_all(&destination)
        .map_err(|error| format!("cannot create {}: {error}", destination.display()))?;
    prune_tree(
        &source,
        &destination,
        &[".gradle", "build", ".cxx", "local.properties"],
    )?;
    copy_tree(
        &source,
        &destination,
        &[".gradle", "build", ".cxx", "local.properties"],
    )?;
    Ok(destination)
}

fn prune_tree(source: &Path, destination: &Path, ignored: &[&str]) -> Result<(), String> {
    for entry in fs::read_dir(destination)
        .map_err(|error| format!("cannot read {}: {error}", destination.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        if ignored.iter().any(|ignored| OsStr::new(ignored) == name) {
            continue;
        }
        let target = entry.path();
        let expected = source.join(&name);
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", target.display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "refusing symlink in generated Android workspace: {}",
                target.display()
            ));
        }
        if !expected.exists() {
            if file_type.is_dir() {
                fs::remove_dir_all(&target)
                    .map_err(|error| format!("cannot prune {}: {error}", target.display()))?;
            } else {
                fs::remove_file(&target)
                    .map_err(|error| format!("cannot prune {}: {error}", target.display()))?;
            }
            continue;
        }
        let expected_type = expected
            .metadata()
            .map_err(|error| format!("cannot inspect {}: {error}", expected.display()))?;
        if file_type.is_dir() && expected_type.is_dir() {
            prune_tree(&expected, &target, ignored)?;
        } else if file_type.is_dir() != expected_type.is_dir() {
            if file_type.is_dir() {
                fs::remove_dir_all(&target)
                    .map_err(|error| format!("cannot replace {}: {error}", target.display()))?;
            } else {
                fs::remove_file(&target)
                    .map_err(|error| format!("cannot replace {}: {error}", target.display()))?;
            }
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path, ignored: &[&str]) -> Result<(), String> {
    for entry in fs::read_dir(source)
        .map_err(|error| format!("cannot read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        if ignored.iter().any(|ignored| OsStr::new(ignored) == name) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "refusing symlink in Pam Native SDK template: {}",
                entry.path().display()
            ));
        }
        let target = destination.join(&name);
        if file_type.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
            copy_tree(&entry.path(), &target, ignored)?;
        } else if file_type.is_file() {
            target
                .parent()
                .map(fs::create_dir_all)
                .transpose()
                .map_err(|error| {
                    format!("cannot create parent for {}: {error}", target.display())
                })?;
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("cannot copy {}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn configure_android(
    project: &Project,
    native_home: &Path,
    runtime: &ResolvedRuntime,
    workspace: &Path,
    abis: &[AndroidAbi],
) -> Result<(), String> {
    let sdk = android_sdk()?;
    write_atomic(
        &workspace.join("local.properties"),
        format!("sdk.dir={}\n", property_value(&sdk.to_string_lossy())).as_bytes(),
    )?;
    let properties = format!(
        "nativeHome={}\nruntimeHome={}\nprojectRoot={}\napplicationId={}\napplicationName={}\nminSdk={}\ntargetSdk={}\nversionCode={}\nversionName={}\nabis={}\n",
        property_value(&native_home.to_string_lossy()),
        property_value(&runtime.root.to_string_lossy()),
        property_value(&project.root.to_string_lossy()),
        project.manifest.application_id,
        property_value(&project.manifest.name),
        project.manifest.android.min_sdk,
        project.manifest.android.target_sdk,
        project.manifest.version_code,
        property_value(&project.manifest.version_name),
        display_abis(abis),
    );
    write_atomic(
        &workspace.join("pam-native.properties"),
        properties.as_bytes(),
    )?;
    generate_plugin_projects(project, workspace)?;
    write_plugin_lock(project)?;
    let permissions = project
        .plugins
        .iter()
        .flat_map(|plugin| plugin.manifest.android.permissions.iter())
        .chain(project.manifest.android.permissions.iter())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    add_permissions(
        &workspace.join("app/src/main/AndroidManifest.xml"),
        &permissions,
    )?;
    add_deep_links(
        &workspace.join("app/src/main/AndroidManifest.xml"),
        &project.manifest.android.deep_links,
    )
}

fn property_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('=', "\\=")
        .replace(':', "\\:")
}

fn add_permissions(manifest: &Path, permissions: &[String]) -> Result<(), String> {
    if permissions.is_empty() {
        return Ok(());
    }
    let mut contents = fs::read_to_string(manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    let marker = "    <application";
    let position = contents
        .find(marker)
        .ok_or_else(|| "Android manifest has no application element".to_owned())?;
    let mut declarations = String::new();
    for permission in permissions {
        if !contents.contains(&format!("android:name=\"{permission}\"")) {
            declarations.push_str(&format!(
                "    <uses-permission android:name=\"{permission}\" />\n"
            ));
        }
    }
    contents.insert_str(position, &declarations);
    write_atomic(manifest, contents.as_bytes())
}

fn add_deep_links(manifest: &Path, links: &[AndroidDeepLink]) -> Result<(), String> {
    let mut contents = fs::read_to_string(manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    const START_MARKER: &str = "            <!-- pam:deep-links -->\n";
    const END_MARKER: &str = "            <!-- /pam:deep-links -->\n";
    if let Some(start) = contents.find(START_MARKER) {
        let end = contents[start + START_MARKER.len()..]
            .find(END_MARKER)
            .map(|position| start + START_MARKER.len() + position + END_MARKER.len())
            .ok_or_else(|| "Android manifest has an incomplete PAM deep-link block".to_owned())?;
        contents.replace_range(start..end, "");
    }
    if links.is_empty() {
        return write_atomic(manifest, contents.as_bytes());
    }
    let activity_start = contents
        .find("android:name=\".PamActivity\"")
        .ok_or_else(|| "Android manifest has no PamActivity element".to_owned())?;
    let activity_end = contents[activity_start..]
        .find("</activity>")
        .map(|position| activity_start + position)
        .ok_or_else(|| "Android manifest has no closing PamActivity element".to_owned())?;
    let mut declarations = START_MARKER.to_owned();
    for link in links {
        let auto_verify = if link.auto_verify {
            " android:autoVerify=\"true\""
        } else {
            ""
        };
        declarations.push_str(&format!(
            "            <intent-filter{auto_verify}>\n\
             \x20   <action android:name=\"android.intent.action.VIEW\" />\n\
             \x20   <category android:name=\"android.intent.category.DEFAULT\" />\n\
             \x20   <category android:name=\"android.intent.category.BROWSABLE\" />\n\
             \x20   <data android:scheme=\"{}\"",
            link.scheme,
        ));
        if let Some(host) = &link.host {
            declarations.push_str(&format!(" android:host=\"{host}\""));
        }
        if let Some(path) = &link.path_prefix {
            declarations.push_str(&format!(" android:pathPrefix=\"{path}\""));
        }
        declarations.push_str(
            " />\n\
             \x20   </intent-filter>\n",
        );
    }
    declarations.push_str(END_MARKER);
    contents.insert_str(activity_end, &declarations);
    write_atomic(manifest, contents.as_bytes())
}

fn generate_plugin_projects(project: &Project, workspace: &Path) -> Result<(), String> {
    let plugins_directory = workspace.join("pam-plugins");
    if plugins_directory.exists() {
        fs::remove_dir_all(&plugins_directory).map_err(|error| {
            format!(
                "cannot clean generated plugin projects {}: {error}",
                plugins_directory.display()
            )
        })?;
    }
    fs::create_dir_all(&plugins_directory).map_err(|error| {
        format!(
            "cannot create generated plugin projects {}: {error}",
            plugins_directory.display()
        )
    })?;

    let android_plugins = project
        .plugins
        .iter()
        .filter(|plugin| has_android_payload(&plugin.manifest))
        .collect::<Vec<_>>();
    let repositories = android_plugins
        .iter()
        .flat_map(|plugin| plugin.manifest.android.repositories.iter())
        .collect::<BTreeSet<_>>();
    let mut properties = format!(
        "plugin.count={}\nrepository.count={}\n",
        android_plugins.len(),
        repositories.len()
    );
    for (index, repository) in repositories.iter().enumerate() {
        properties.push_str(&format!(
            "repository.{index}={}\n",
            property_value(repository)
        ));
    }

    for (index, plugin) in android_plugins.iter().enumerate() {
        let module_name = format!(":pam-plugin-{index}");
        let module_directory = plugins_directory.join(format!("plugin-{index}"));
        fs::create_dir_all(module_directory.join("src/main")).map_err(|error| {
            format!(
                "cannot create generated project for {}: {error}",
                plugin.package
            )
        })?;
        let generated_manifest = module_directory.join("src/main/AndroidManifest.xml");
        write_atomic(
            &generated_manifest,
            b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<manifest />\n",
        )?;
        let namespace = plugin
            .manifest
            .android
            .namespace
            .clone()
            .unwrap_or_else(|| generated_namespace(index, &plugin.package));
        let build_script = plugin_build_script(plugin, &namespace, &generated_manifest)?;
        write_atomic(
            &module_directory.join("build.gradle.kts"),
            build_script.as_bytes(),
        )?;
        properties.push_str(&format!(
            "plugin.{index}.module={}\nplugin.{index}.dir={}\nplugin.{index}.package={}\n",
            property_value(&module_name),
            property_value(&module_directory.to_string_lossy()),
            property_value(&plugin.package),
        ));
    }

    write_atomic(
        &workspace.join("pam-plugins.properties"),
        properties.as_bytes(),
    )
}

fn has_android_payload(manifest: &PluginManifest) -> bool {
    !manifest.modules.is_empty()
        || !manifest.views.is_empty()
        || !manifest.android.permissions.is_empty()
        || !manifest.android.repositories.is_empty()
        || !manifest.android.dependencies.is_empty()
        || !manifest.android.local_aars.is_empty()
        || !manifest.android.source_dirs.is_empty()
        || !manifest.android.resource_dirs.is_empty()
        || !manifest.android.asset_dirs.is_empty()
        || !manifest.android.jni_lib_dirs.is_empty()
        || manifest.android.manifest.is_some()
        || manifest.android.consumer_rules.is_some()
}

fn generated_namespace(index: usize, package: &str) -> String {
    let package = package
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                char::from(byte)
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("dev.pam.generated.plugin{index}.{package}")
}

fn plugin_build_script(
    plugin: &NativePlugin,
    namespace: &str,
    generated_manifest: &Path,
) -> Result<String, String> {
    let android = &plugin.manifest.android;
    let manifest = match &android.manifest {
        Some(path) => canonical_plugin_path(plugin, path)?,
        None => generated_manifest.to_path_buf(),
    };
    let mut source = format!(
        "plugins {{\n    id(\"com.android.library\")\n}}\n\n\
         android {{\n    namespace = {}\n    compileSdk = 36\n\n\
         \x20   defaultConfig {{\n        minSdk = {}\n",
        kotlin_string(namespace),
        android.min_sdk,
    );
    if let Some(rules) = &android.consumer_rules {
        source.push_str(&format!(
            "        consumerProguardFiles({})\n",
            kotlin_string(&canonical_plugin_path(plugin, rules)?.to_string_lossy())
        ));
    }
    source.push_str(
        "    }\n\n    compileOptions {\n        sourceCompatibility = JavaVersion.VERSION_17\n\
         \x20       targetCompatibility = JavaVersion.VERSION_17\n    }\n\n\
         \x20   sourceSets {\n        getByName(\"main\") {\n",
    );
    source.push_str(&format!(
        "            manifest.srcFile({})\n",
        kotlin_string(&manifest.to_string_lossy())
    ));
    append_source_directories(&mut source, "java", plugin, &android.source_dirs, false)?;
    append_source_directories(&mut source, "res", plugin, &android.resource_dirs, false)?;
    append_source_directories(&mut source, "assets", plugin, &android.asset_dirs, false)?;
    append_source_directories(&mut source, "jniLibs", plugin, &android.jni_lib_dirs, false)?;
    if !android.source_dirs.is_empty() {
        let values = android
            .source_dirs
            .iter()
            .map(|path| {
                canonical_plugin_path(plugin, path)
                    .map(|path| kotlin_string(&path.to_string_lossy()))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        source.push_str(&format!(
            "            kotlin.directories.addAll(listOf({values}))\n"
        ));
    }
    source.push_str(
        "        }\n    }\n\n    lint {\n        abortOnError = true\n\
         \x20       warningsAsErrors = true\n        disable += setOf(\n\
         \x20           \"AndroidGradlePluginVersion\",\n            \"GradleDependency\",\n\
         \x20       )\n    }\n}\n\n\
         dependencies {\n    api(project(\":plugin-api\"))\n",
    );
    for dependency in &android.dependencies {
        source.push_str(&format!(
            "    implementation({})\n",
            kotlin_string(dependency)
        ));
    }
    for aar in &android.local_aars {
        let resolved = canonical_plugin_path(plugin, aar)?;
        if resolved.extension() != Some(OsStr::new("aar")) || !resolved.is_file() {
            return Err(format!(
                "plugin {} local AAR {} must point to an .aar file",
                plugin.package,
                aar.display()
            ));
        }
        source.push_str(&format!(
            "    implementation(files({}))\n",
            kotlin_string(&resolved.to_string_lossy())
        ));
    }
    source.push_str("}\n");
    Ok(source)
}

fn append_source_directories(
    output: &mut String,
    kind: &str,
    plugin: &NativePlugin,
    paths: &[PathBuf],
    allow_files: bool,
) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    let values = paths
        .iter()
        .map(|path| {
            let resolved = canonical_plugin_path(plugin, path)?;
            if !allow_files && !resolved.is_dir() {
                return Err(format!(
                    "plugin {} {} path {} must be a directory",
                    plugin.package,
                    kind,
                    path.display()
                ));
            }
            Ok(kotlin_string(&resolved.to_string_lossy()))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(", ");
    output.push_str(&format!(
        "            {kind}.directories.addAll(listOf({values}))\n"
    ));
    Ok(())
}

fn canonical_plugin_path(plugin: &NativePlugin, path: &Path) -> Result<PathBuf, String> {
    let resolved = fs::canonicalize(plugin.root.join(path)).map_err(|error| {
        format!(
            "plugin {} path {} cannot be resolved: {error}",
            plugin.package,
            path.display()
        )
    })?;
    if !resolved.starts_with(&plugin.root) {
        return Err(format!(
            "plugin {} path {} escapes its Composer package",
            plugin.package,
            path.display()
        ));
    }
    Ok(resolved)
}

fn kotlin_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

fn write_plugin_lock(project: &Project) -> Result<(), String> {
    let entries = project
        .plugins
        .iter()
        .map(|plugin| PluginLockEntry {
            package: &plugin.package,
            package_version: &plugin.package_version,
            descriptor_sha256: &plugin.descriptor_digest,
            php_provider: plugin.manifest.php.provider.as_deref(),
            modules: plugin
                .manifest
                .modules
                .iter()
                .map(|module| module.name.as_str())
                .collect(),
            views: plugin
                .manifest
                .views
                .iter()
                .map(|view| view.name.as_str())
                .collect(),
            android_dependencies: plugin
                .manifest
                .android
                .dependencies
                .iter()
                .map(String::as_str)
                .collect(),
        })
        .collect();
    let lock = PluginLock {
        version: PLUGIN_LOCK_VERSION,
        protocol: PLUGIN_PROTOCOL_VERSION,
        pam_native: env!("CARGO_PKG_VERSION"),
        plugins: entries,
    };
    let bytes = serde_json::to_vec_pretty(&lock)
        .map_err(|error| format!("cannot encode plugin lock: {error}"))?;
    let target = project.root.join(".pam-native/plugins.lock.json");
    let mut bytes = bytes;
    bytes.push(b'\n');
    write_atomic(&target, &bytes)
}

fn generate_modules(project: &Project, workspace: &Path) -> Result<(), String> {
    let target =
        workspace.join("app/src/main/java/dev/pam/nativeapp/modules/GeneratedPamModules.kt");
    let mut source = String::from(
        "package dev.pam.nativeapp.modules\n\nimport android.content.Context\n\n\
         /** Generated by `pam mobile codegen`. */\nobject GeneratedPamModules {\n\
         \x20   fun create(context: Context): Map<String, NativeModule> = buildMap {\n",
    );
    for module in project.manifest.modules.iter().chain(
        project
            .plugins
            .iter()
            .flat_map(|plugin| plugin.manifest.modules.iter()),
    ) {
        source.push_str(&format!(
            "        put({:?}, {}(context))\n",
            module.name, module.class
        ));
    }
    if project.manifest.modules.is_empty()
        && project
            .plugins
            .iter()
            .all(|plugin| plugin.manifest.modules.is_empty())
    {
        source.push_str("        context.applicationContext\n");
    }
    source.push_str("    }\n}\n");
    write_atomic(&target, source.as_bytes())
}

fn generate_views(project: &Project, workspace: &Path) -> Result<(), String> {
    let target = workspace.join("app/src/main/java/dev/pam/nativeapp/views/GeneratedPamViews.kt");
    let mut source = String::from(
        "package dev.pam.nativeapp.views\n\nimport android.content.Context\n\n\
         /** Generated by `pam mobile codegen`. */\nobject GeneratedPamViews {\n\
         \x20   fun create(context: Context): Map<String, NativeViewFactory> = buildMap {\n",
    );
    for view in project.manifest.views.iter().chain(
        project
            .plugins
            .iter()
            .flat_map(|plugin| plugin.manifest.views.iter()),
    ) {
        source.push_str(&format!(
            "        put({:?}, {}(context))\n",
            view.name, view.class
        ));
    }
    if project.manifest.views.is_empty()
        && project
            .plugins
            .iter()
            .all(|plugin| plugin.manifest.views.is_empty())
    {
        source.push_str("        context.applicationContext\n");
    }
    source.push_str("    }\n}\n");
    write_atomic(&target, source.as_bytes())
}

fn stage_project(project: &Project, workspace: &Path) -> Result<(), String> {
    let destination = workspace.join("app/src/main/assets/pam");
    if destination.exists() {
        fs::remove_dir_all(&destination)
            .map_err(|error| format!("cannot clean {}: {error}", destination.display()))?;
    }
    fs::create_dir_all(&destination)
        .map_err(|error| format!("cannot create {}: {error}", destination.display()))?;
    let mut budget = CopyBudget::default();
    copy_project_files(&project.root, &project.root, &destination, &mut budget)?;
    if project.manifest.entry != Path::new("index.php") {
        let entry = project.manifest.entry.to_string_lossy().replace('\\', "/");
        if entry.contains('\'') {
            return Err("mobile entry cannot contain a single quote".to_owned());
        }
        write_atomic(
            &destination.join("index.php"),
            format!("<?php\n\ndeclare(strict_types=1);\n\nrequire __DIR__.'/{entry}';\n")
                .as_bytes(),
        )?;
    }
    let version = directory_digest(&destination)?;
    write_atomic(
        &destination.join("manifest.sha256"),
        format!("{version}\n").as_bytes(),
    )
}

#[derive(Default)]
struct CopyBudget {
    files: usize,
    bytes: u64,
}

fn copy_project_files(
    root: &Path,
    current: &Path,
    destination: &Path,
    budget: &mut CopyBudget,
) -> Result<(), String> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| format!("cannot read {}: {error}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_path_buf();
        if ignored_project_path(&relative) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "mobile application bundles cannot contain symlinks: {}",
                relative.display()
            ));
        }
        let target = destination.join(&relative);
        if file_type.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
            copy_project_files(root, &entry.path(), destination, budget)?;
        } else if file_type.is_file() {
            let bytes = entry.metadata().map_err(|error| error.to_string())?.len();
            budget.files += 1;
            budget.bytes = budget.bytes.saturating_add(bytes);
            if budget.files > MAX_PROJECT_FILES
                || bytes > MAX_FILE_BYTES
                || budget.bytes > MAX_PROJECT_BYTES
            {
                return Err(
                    "mobile application exceeds the safe bundle file or size limits".to_owned(),
                );
            }
            target
                .parent()
                .map(fs::create_dir_all)
                .transpose()
                .map_err(|error| {
                    format!("cannot create parent for {}: {error}", target.display())
                })?;
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("cannot copy {}: {error}", relative.display()))?;
        }
    }
    Ok(())
}

fn ignored_project_path(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::Normal(value)
                if value.to_str().is_some_and(|name| {
                    name.starts_with('.')
                        || matches!(
                            name,
                            ".build"
                            | "build"
                            | "dist"
                            | "docs"
                            | "examples"
                            | "node_modules"
                            | "target"
                            | "tests"
                            | "tools"
                        )
                })
        )
    })
}

fn directory_digest(root: &Path) -> Result<String, String> {
    let mut files = files_in(root)?;
    files.retain(|file| file.file_name() != Some(OsStr::new("manifest.sha256")));
    let mut digest = Sha256::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        digest.update(relative.as_bytes());
        digest.update([0]);
        let mut input = fs::File::open(&file)
            .map_err(|error| format!("cannot read {}: {error}", file.display()))?;
        let mut buffer = [0_u8; 8192];
        loop {
            let read = input.read(&mut buffer).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn files_in(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn visit(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in fs::read_dir(root)
            .map_err(|error| format!("cannot read {}: {error}", root.display()))?
        {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                visit(&entry.path(), files)?;
            } else {
                files.push(entry.path());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, &mut files)?;
    files.sort_by_key(|file| {
        file.strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/")
    });
    Ok(files)
}

struct BuiltApk {
    project: Project,
    path: PathBuf,
    mode: BuildMode,
}

fn build(options: MobileOptions) -> Result<BuiltApk, String> {
    let project = load_project(&options.project)?;
    let native_home = native_home()?;
    let runtime = resolve_runtime(&project, &pam_home()?)?;
    let workspace = prepare(&project, &native_home, &options.abis)?;
    for abi in &options.abis {
        if !runtime_ready_at(&runtime.root, *abi) {
            return Err(format!(
                "verified PHP {} Android runtime is missing for {}; build it with `pam mobile runtime:update` (expected {})",
                runtime.release.php_version,
                abi.android(),
                runtime.root.join(abi.android()).display()
            ));
        }
        build_engine(&native_home, *abi)?;
    }
    let gradlew = workspace.join("gradlew");
    let status = Command::new(&gradlew)
        .arg(format!(":app:{}", options.mode.gradle_task()))
        .arg("--stacktrace")
        .env(
            "GRADLE_USER_HOME",
            project.root.join(".pam-native/gradle-home"),
        )
        .current_dir(&workspace)
        .status()
        .map_err(|error| format!("cannot start Gradle: {error}"))?;
    if !status.success() {
        return Err(format!("Gradle exited with status {status}"));
    }
    let output_directory = workspace
        .join("app/build/outputs/apk")
        .join(options.mode.directory());
    let signed_apk = output_directory.join(format!("app-{}.apk", options.mode.directory()));
    let unsigned_apk =
        output_directory.join(format!("app-{}-unsigned.apk", options.mode.directory()));
    let apk = if signed_apk.is_file() {
        signed_apk
    } else if unsigned_apk.is_file() {
        unsigned_apk
    } else {
        return Err(format!(
            "Gradle did not produce an APK in {}",
            output_directory.display()
        ));
    };
    println!("Built {}", apk.display());
    Ok(BuiltApk {
        project,
        path: apk,
        mode: options.mode,
    })
}

fn benchmark(project_path: PathBuf) -> Result<u8, String> {
    run_android_performance_suite(
        project_path,
        "dev.pam.nativeapp.benchmark.PamNativeBenchmark#coldStartup",
        "Benchmark",
    )
}

fn baseline_profile(project_path: PathBuf) -> Result<u8, String> {
    run_android_performance_suite(
        project_path,
        "dev.pam.nativeapp.benchmark.BaselineProfileGenerator",
        "Baseline Profile",
    )
}

fn toggle_devtools(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let application_id = format!("{}.debug", project.manifest.application_id);
    let running = Command::new("adb")
        .args(["shell", "pidof", &application_id])
        .output()
        .map_err(|error| format!("cannot query Android device: {error}"))?;
    if !running.status.success() || running.stdout.is_empty() {
        return Err(format!(
            "{application_id} is not running; start it with `pam mobile dev {}` first",
            project.root.display()
        ));
    }
    command_status(
        "adb",
        &[
            "shell",
            "am",
            "broadcast",
            "-a",
            "dev.pam.nativeapp.action.TOGGLE_DEVTOOLS",
            "-p",
            &application_id,
        ],
    )?;
    println!("Toggled Pam Native DevTools in {application_id}");
    Ok(0)
}

fn run_android_performance_suite(
    project_path: PathBuf,
    test_class: &str,
    label: &str,
) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let native_home = native_home()?;
    let runtime = resolve_runtime(&project, &pam_home()?)?;
    let abi = connected_abi()?;
    let workspace = prepare(&project, &native_home, &[abi])?;
    if !runtime_ready_at(&runtime.root, abi) {
        return Err(format!(
            "verified PHP {} Android runtime is missing for {}; expected {}",
            runtime.release.php_version,
            abi.android(),
            runtime.root.join(abi.android()).display()
        ));
    }
    build_engine(&native_home, abi)?;
    let class_argument = format!("-Pandroid.testInstrumentationRunnerArguments.class={test_class}");
    let status = Command::new(workspace.join("gradlew"))
        .arg(":macrobenchmark:connectedBenchmarkAndroidTest")
        .arg(class_argument)
        .args(["--stacktrace", "--no-configuration-cache"])
        .env(
            "GRADLE_USER_HOME",
            project.root.join(".pam-native/gradle-home"),
        )
        .current_dir(&workspace)
        .status()
        .map_err(|error| format!("cannot start the Android benchmark: {error}"))?;
    if !status.success() {
        return Err(format!("{label} collection exited with status {status}"));
    }
    println!(
        "{label} complete. Android Studio and CI results are in {}",
        workspace
            .join("macrobenchmark/build/outputs/connected_android_test_additional_output")
            .display()
    );
    Ok(0)
}

fn generate_screen(options: GeneratorOptions) -> Result<u8, String> {
    let project = load_project(&options.project)?;
    let component_path = project
        .root
        .join("src/Screens")
        .join(format!("{}.pam.php", options.name));
    ensure_available(&[&component_path])?;
    let component = format!(
        r#"<?php

declare(strict_types=1);

namespace App\Screens;

use Pam\Native\Attributes\State;
use Pam\Native\Component;

final class {name} extends Component
{{
    #[State]
    public int $count = 0;

    public function increment(): void
    {{
        $this->count++;
    }}
}}
?>

<template>
    <Screen>
        <SafeAreaView class="flex-1 surface">
            <Column class="flex-1 p-6 gap-4">
                <Text class="text-primary" height="48" fontSize="28" fontWeight="700">{title}</Text>
                <Text class="text-muted" height="44">Native Android UI controlled by persistent PHP.</Text>
                <Button class="accent" height="52" @press="increment" accessibilityLabel="{title} counter">
                    Count: {{{{ $count }}}}
                </Button>
            </Column>
        </SafeAreaView>
    </Screen>
</template>
"#,
        name = options.name,
        title = options.name,
    );
    write_new_file(&component_path, component.as_bytes())?;
    println!("Created screen {}", component_path.display());
    println!(
        "After App::components(...): App::make(App\\Screens\\{}::class)",
        options.name
    );
    Ok(0)
}

fn generate_component(options: GeneratorOptions) -> Result<u8, String> {
    let project = load_project(&options.project)?;
    let component_path = project
        .root
        .join("src/Components")
        .join(format!("{}.pam.php", options.name));
    ensure_available(&[&component_path])?;
    let component = format!(
        r#"<?php

declare(strict_types=1);

namespace App\Components;

use Pam\Native\Component;

final class {name} extends Component
{{
    public function __construct(
        public string $title,
        public ?string $subtitle = null,
        public bool $elevated = false,
    ) {{
    }}
}}
?>

<template>
    <Column :class="['card', 'gap-2', 'elevation-2' => $elevated]">
        <Row class="items-center justify-between">
            <Column>
                <Text class="text-primary" height="32" fontSize="18" fontWeight="700">
                    {{{{ $title }}}}
                </Text>
                <Text v-if="$subtitle" class="text-muted" height="28">
                    {{{{ $subtitle }}}}
                </Text>
            </Column>
            <Slot name="action" />
        </Row>
        <Slot>
            <Text class="text-muted" height="32">{name} content</Text>
        </Slot>
    </Column>
</template>
"#,
        name = options.name,
    );
    write_new_file(&component_path, component.as_bytes())?;
    println!("Created component {}", component_path.display());
    println!(
        "Use it as <{name} title=\"...\"> after App::components(__DIR__.'/src').",
        name = options.name,
    );
    Ok(0)
}

fn generate_flow(options: GeneratorOptions) -> Result<u8, String> {
    let project = load_project(&options.project)?;
    let template_name = format!("{}-flow", kebab_case(&options.name));
    let component_path = project
        .root
        .join("src/Flows")
        .join(format!("{}.php", options.name));
    let template_path = project
        .root
        .join("resources/native")
        .join(format!("{template_name}.pam"));
    let test_path = project
        .root
        .join("tests")
        .join(format!("{}FlowTest.php", options.name));
    ensure_available(&[&component_path, &template_path, &test_path])?;

    let component = format!(
        r#"<?php

declare(strict_types=1);

namespace App\Flows;

use Pam\Native\Attributes\State;
use Pam\Native\Component;
use Pam\Native\Renderable;
use Pam\Native\View;

enum {name}Step: int
{{
    case Details = 1;
    case Review = 2;
    case Complete = 3;
}}

final class {name} extends Component
{{
    #[State]
    public int $step = {name}Step::Details->value;

    public function render(): Renderable
    {{
        return View::make('{template_name}');
    }}

    public function next(): void
    {{
        $this->step = min({name}Step::Complete->value, $this->step + 1);
    }}

    public function back(): void
    {{
        $this->step = max({name}Step::Details->value, $this->step - 1);
    }}
}}
"#,
        name = options.name,
    );
    let template = format!(
        r#"<AppScreen title="{name}" subtitle="A generated, typed PAM flow.">
    <VStack class="gap-4">
        <HStack class="items-center justify-between">
            <Badge variant="secondary">
                <BadgeText>Step {{{{ $step }}}} of 3</BadgeText>
            </Badge>
            <Text class="ui-text-muted">State survives native re-renders</Text>
        </HStack>

        <Progress :value="$step * 33.333">
            <ProgressFilledTrack />
        </Progress>

        <Card v-if="$step === 1" class="p-5 rounded-2xl">
            <VStack class="gap-2">
                <Heading size="lg">Details</Heading>
                <Text class="ui-text-muted">Collect typed input with FormField and NativeForm here.</Text>
            </VStack>
        </Card>
        <Card v-else-if="$step === 2" class="p-5 rounded-2xl">
            <VStack class="gap-2">
                <Heading size="lg">Review</Heading>
                <Text class="ui-text-muted">Review data before the final native action.</Text>
            </VStack>
        </Card>
        <ContentState v-else status="content">
            <Alert action="success" variant="subtle" class="rounded-2xl">
                <AlertText>{name} completed successfully.</AlertText>
            </Alert>
        </ContentState>
    </VStack>

    <template #bottom>
        <HStack class="p-4 gap-3">
            <Button
                v-if="$step > 1 &amp;&amp; $step < 3"
                variant="outline"
                class="flex-1"
                @press="back"
            >
                <ButtonText>Back</ButtonText>
            </Button>
            <Button v-if="$step < 3" class="flex-1" @press="next">
                <ButtonText>{{{{ $step === 2 ? 'Confirm' : 'Continue' }}}}</ButtonText>
            </Button>
        </HStack>
    </template>
</AppScreen>
"#,
        name = options.name,
    );
    let test = format!(
        r#"<?php

declare(strict_types=1);

use App\Flows\{name};
use App\Flows\{name}Step;

require dirname(__DIR__).'/vendor/autoload.php';

$flow = new {name}();
assert($flow->step === {name}Step::Details->value);
$flow->next();
assert($flow->step === {name}Step::Review->value);
$flow->next();
$flow->next();
assert($flow->step === {name}Step::Complete->value);
$flow->back();
assert($flow->step === {name}Step::Review->value);

fwrite(STDOUT, "{name} flow contract passed.\n");
"#,
        name = options.name,
    );

    let mut created = Vec::new();
    for (path, contents) in [
        (&component_path, component.as_bytes()),
        (&template_path, template.as_bytes()),
        (&test_path, test.as_bytes()),
    ] {
        if let Err(error) = write_new_file(path, contents) {
            for created_path in created {
                let _ = fs::remove_file(created_path);
            }
            return Err(error);
        }
        created.push(path);
    }

    println!("Created flow {}", component_path.display());
    println!("Created template {}", template_path.display());
    println!("Created contract test {}", test_path.display());
    println!(
        "Mount App\\Flows\\{} after App::views(...), then run `php {}`.",
        options.name,
        test_path.display(),
    );
    Ok(0)
}

fn generate_native_view(options: GeneratorOptions) -> Result<u8, String> {
    let project = load_project(&options.project)?;
    let binding_name = kebab_case(&options.name);
    let package = format!("{}.views", project.manifest.application_id);
    let class_name = format!("{}Factory", options.name);
    let qualified_class = format!("{package}.{class_name}");
    let source_path = project
        .root
        .join("android/src/main/kotlin")
        .join(package.replace('.', "/"))
        .join(format!("{class_name}.kt"));
    ensure_available(&[&source_path])?;

    let manifest_path = project.root.join(MANIFEST_NAME);
    let contents = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let mut manifest: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    let views = manifest
        .get_mut("views")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| "pam-native.json must contain a views array".to_owned())?;
    if views.iter().any(|view| {
        view.get("name").and_then(serde_json::Value::as_str) == Some(binding_name.as_str())
            || view.get("class").and_then(serde_json::Value::as_str)
                == Some(qualified_class.as_str())
    }) {
        return Err(format!(
            "native view {binding_name:?} or class {qualified_class:?} is already registered"
        ));
    }
    views.push(serde_json::json!({
        "name": binding_name,
        "class": qualified_class,
    }));
    let next_manifest = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("cannot serialize the native manifest: {error}"))?
        + "\n";
    let source = format!(
        r#"package {package}

import android.content.Context
import android.graphics.Color
import android.view.View
import android.widget.TextView
import dev.pam.nativeapp.protocol.WireValue
import dev.pam.nativeapp.views.NativeViewFactory

class {class_name}(
    @Suppress("UNUSED_PARAMETER") context: Context,
) : NativeViewFactory {{
    override fun create(
        context: Context,
        emit: (ByteArray) -> Unit,
    ): View = TextView(context).apply {{
        setTextColor(Color.WHITE)
        setBackgroundColor(0xFF1E293B.toInt())
        textSize = 16f
        text = "{name}"
    }}

    override fun update(
        view: View,
        properties: Map<String, WireValue>,
    ) {{
        val label = (properties["label"] as? WireValue.Text)?.value ?: "{name}"
        (view as TextView).text = label
        view.isEnabled = (properties["enabled"] as? WireValue.Flag)?.value ?: true
    }}
}}
"#,
        name = options.name,
    );
    write_new_file(&source_path, source.as_bytes())?;
    if let Err(error) = write_atomic(&manifest_path, next_manifest.as_bytes()) {
        let _ = fs::remove_file(&source_path);
        return Err(error);
    }
    println!("Created {}", source_path.display());
    println!(
        "Registered <Native name=\"{}\" :properties=\"$props\" /> in {}",
        kebab_case(&options.name),
        manifest_path.display()
    );
    Ok(0)
}

fn ensure_available(paths: &[&Path]) -> Result<(), String> {
    if let Some(path) = paths.iter().find(|path| path.exists()) {
        return Err(format!(
            "refusing to overwrite existing generated file {}",
            path.display()
        ));
    }
    Ok(())
}

fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    output
        .write_all(contents)
        .and_then(|()| output.sync_all())
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn build_engine(native_home: &Path, abi: AndroidAbi) -> Result<(), String> {
    if engine_ready(native_home, abi) {
        return Ok(());
    }
    let installed = installed_rust_targets()?;
    if !installed.contains(abi.rust_target()) {
        return Err(format!(
            "Rust target {} is missing; run `rustup target add {}`",
            abi.rust_target(),
            abi.rust_target()
        ));
    }
    let sdk = android_sdk()?;
    let prebuilt = sdk
        .join("ndk/27.1.12297006/toolchains/llvm/prebuilt")
        .join(host_tag());
    let linker = prebuilt.join("bin").join(abi.clang());
    if !linker.is_file() {
        return Err(format!(
            "Android NDK linker is missing: {}",
            linker.display()
        ));
    }
    let linker_key = format!(
        "CARGO_TARGET_{}_LINKER",
        abi.rust_target().replace('-', "_").to_ascii_uppercase()
    );
    let status = Command::new("cargo")
        .args([
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            "Cargo.toml",
            "-p",
            "pam-native-engine",
            "--target",
            abi.rust_target(),
        ])
        .env(linker_key, linker)
        .current_dir(native_home)
        .status()
        .map_err(|error| format!("cannot build Pam Native engine: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Pam Native engine build failed for {} with {status}",
            abi.android()
        ))
    }
}

fn installed_rust_targets() -> Result<HashSet<String>, String> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map_err(|error| format!("cannot inspect Rust targets: {error}"))?;
    if !output.status.success() {
        return Err("`rustup target list --installed` failed".to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect())
}

fn runtime_ready_at(runtime_root: &Path, abi: AndroidAbi) -> bool {
    let root = runtime_root.join(abi.android());
    root.join("lib/libphp.a").is_file()
        && root.join("include/php/main/php.h").is_file()
        && root.join("include/php/sapi/embed/php_embed.h").is_file()
}

fn engine_library(native_home: &Path, abi: AndroidAbi) -> PathBuf {
    native_home
        .join("target")
        .join(abi.rust_target())
        .join("release/libpam_native_engine.a")
}

fn engine_ready(native_home: &Path, abi: AndroidAbi) -> bool {
    engine_library(native_home, abi).is_file()
}

fn android_sdk() -> Result<PathBuf, String> {
    for name in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(path) = std::env::var_os(name) {
            let path = PathBuf::from(path);
            if path.is_dir() {
                return fs::canonicalize(&path)
                    .map_err(|error| format!("cannot resolve {}: {error}", path.display()));
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home).join("Android/Sdk");
        if candidate.is_dir() {
            return fs::canonicalize(candidate)
                .map_err(|error| format!("cannot resolve Android SDK: {error}"));
        }
    }
    Err("Android SDK not found; set ANDROID_HOME".to_owned())
}

fn host_tag() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        "linux-x86_64"
    }
}

fn connected_abi() -> Result<AndroidAbi, String> {
    let output = Command::new("adb")
        .args(["shell", "getprop", "ro.product.cpu.abi"])
        .output()
        .map_err(|error| format!("cannot query Android device: {error}"))?;
    if !output.status.success() {
        return Err("no authorized Android device is available through adb".to_owned());
    }
    AndroidAbi::parse(String::from_utf8_lossy(&output.stdout).trim())
}

fn install_and_launch(project: &Project, apk: &Path, mode: BuildMode) -> Result<(), String> {
    command_status("adb", &["install", "-r", apk.to_string_lossy().as_ref()])?;
    let application_id = match mode {
        BuildMode::Debug => format!("{}.debug", project.manifest.application_id),
        BuildMode::Release => project.manifest.application_id.clone(),
    };
    command_status(
        "adb",
        &[
            "shell",
            "am",
            "start",
            "-n",
            &format!("{application_id}/dev.pam.nativeapp.PamActivity"),
        ],
    )?;
    println!("Started {application_id}");
    Ok(())
}

fn command_status(command: &str, arguments: &[&str]) -> Result<(), String> {
    let status = Command::new(command)
        .args(arguments)
        .status()
        .map_err(|error| format!("cannot start {command}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command} exited with status {status}"))
    }
}

fn dev(options: MobileOptions) -> Result<u8, String> {
    let apk = build(options)?;
    command_status(
        "adb",
        &[
            "reverse",
            &format!("tcp:{DEFAULT_PORT}"),
            &format!("tcp:{DEFAULT_PORT}"),
        ],
    )?;
    install_and_launch(&apk.project, &apk.path, BuildMode::Debug)?;
    let native_home = native_home()?;
    let workspace = apk.project.root.join(".pam-native/android");
    println!("Pam Native hot reload listening on 127.0.0.1:{DEFAULT_PORT}. Press Ctrl+C to stop.");
    hot_reload_server(&apk.project, &native_home, &workspace)
}

fn hot_reload_server(
    project: &Project,
    native_home: &Path,
    workspace: &Path,
) -> Result<u8, String> {
    let listener = TcpListener::bind(("127.0.0.1", DEFAULT_PORT))
        .map_err(|error| format!("cannot bind hot reload server: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let mut fingerprint = project_fingerprint(&project.root)?;
    let mut version = String::new();
    let mut bundle = Vec::new();
    let mut pending_change: Option<((u64, u128), Instant)> = None;
    refresh_dev_bundle(project, native_home, workspace, &mut version, &mut bundle)?;
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = respond_hot_reload(&mut stream, &version, &bundle);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("hot reload server failed: {error}")),
        }
        let next = project_fingerprint(&project.root)?;
        if next != fingerprint {
            let stable = pending_change
                .as_ref()
                .is_some_and(|(candidate, detected_at)| {
                    candidate == &next && detected_at.elapsed() >= HOT_RELOAD_DEBOUNCE
                });
            if stable {
                fingerprint = next;
                pending_change = None;
                match refresh_dev_bundle(project, native_home, workspace, &mut version, &mut bundle)
                {
                    Ok(()) => println!("Reload ready · {}", &version[..16]),
                    Err(error) => eprintln!("pam mobile dev: {error}"),
                }
            } else if pending_change
                .as_ref()
                .is_none_or(|(candidate, _)| candidate != &next)
            {
                pending_change = Some((next, Instant::now()));
            }
        } else {
            pending_change = None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn refresh_dev_bundle(
    project: &Project,
    native_home: &Path,
    workspace: &Path,
    version: &mut String,
    bundle: &mut Vec<u8>,
) -> Result<(), String> {
    let runtime = resolve_runtime(project, &pam_home()?)?;
    configure_android(
        project,
        native_home,
        &runtime,
        workspace,
        &[connected_abi()?],
    )?;
    generate_modules(project, workspace)?;
    generate_views(project, workspace)?;
    stage_project(project, workspace)?;
    let next = encode_dev_bundle(&workspace.join("app/src/main/assets/pam"))?;
    if next.len() > MAX_DEV_BUNDLE_BYTES {
        return Err("hot reload bundle exceeds 16 MiB; reduce development assets".to_owned());
    }
    *version = format!("{:x}", Sha256::digest(&next));
    *bundle = next;
    Ok(())
}

fn encode_dev_bundle(root: &Path) -> Result<Vec<u8>, String> {
    let files = files_in(root)?;
    let count = u32::try_from(files.len()).map_err(|_| "too many hot reload files".to_owned())?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PNA1");
    bytes.extend_from_slice(&count.to_le_bytes());
    for file in files {
        let relative = file
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let path = relative.as_bytes();
        let contents =
            fs::read(&file).map_err(|error| format!("cannot read {}: {error}", file.display()))?;
        let path_length =
            u16::try_from(path.len()).map_err(|_| "hot reload path is too long".to_owned())?;
        let content_length =
            u32::try_from(contents.len()).map_err(|_| "hot reload file is too large".to_owned())?;
        bytes.extend_from_slice(&path_length.to_le_bytes());
        bytes.extend_from_slice(path);
        bytes.extend_from_slice(&content_length.to_le_bytes());
        bytes.extend_from_slice(&contents);
    }
    Ok(bytes)
}

fn respond_hot_reload(stream: &mut TcpStream, version: &str, bundle: &[u8]) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .map_err(|error| error.to_string())?;
    let mut request = [0_u8; 4096];
    let read = stream
        .read(&mut request)
        .map_err(|error| error.to_string())?;
    let request = String::from_utf8_lossy(&request[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    if path.starts_with("/status") {
        http_response(stream, "text/plain", version.as_bytes())
    } else if path.starts_with("/bundle") {
        http_response(stream, "application/octet-stream", bundle)
    } else {
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .map_err(|error| error.to_string())
    }
}

fn http_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) -> Result<(), String> {
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .and_then(|()| stream.write_all(body))
        .map_err(|error| error.to_string())
}

fn project_fingerprint(root: &Path) -> Result<(u64, u128), String> {
    fn visit(root: &Path, count: &mut u64, latest: &mut u128) -> Result<(), String> {
        for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(&entry.path())
                .to_path_buf();
            if ignored_project_path(&relative) {
                continue;
            }
            let metadata = entry.metadata().map_err(|error| error.to_string())?;
            if metadata.is_dir() {
                visit(&entry.path(), count, latest)?;
            } else if metadata.is_file() {
                *count = count.saturating_add(metadata.len()).saturating_add(1);
                let changed = metadata
                    .modified()
                    .unwrap_or(SystemTime::UNIX_EPOCH)
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                *latest = (*latest).max(changed);
            }
        }
        Ok(())
    }
    let mut count = 0;
    let mut latest = 0;
    visit(root, &mut count, &mut latest)?;
    Ok((count, latest))
}

fn display_abis(abis: &[AndroidAbi]) -> String {
    abis.iter()
        .map(|abi| abi.android())
        .collect::<Vec<_>>()
        .join(",")
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let temporary = parent.join(format!(
        ".{}.pam-tmp-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    fs::write(&temporary, contents)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("cannot activate {}: {error}", path.display()))
}

fn print_usage() {
    crate::terminal::print_command_help(OsStr::new("pam"), "mobile");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_deep_links_generate_browsable_intent_filters() {
        let manifest = std::env::temp_dir().join(format!(
            "pam-deep-links-{}-{}.xml",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::write(
            &manifest,
            "<manifest><application><activity android:name=\".PamActivity\">\n\
             \x20   </activity></application></manifest>",
        )
        .expect("manifest");
        add_deep_links(
            &manifest,
            &[
                AndroidDeepLink {
                    scheme: "pushin".to_owned(),
                    host: None,
                    path_prefix: None,
                    auto_verify: false,
                },
                AndroidDeepLink {
                    scheme: "https".to_owned(),
                    host: Some("api.zechat.com.br".to_owned()),
                    path_prefix: Some("/reel/".to_owned()),
                    auto_verify: true,
                },
            ],
        )
        .expect("deep-link filters");
        add_deep_links(
            &manifest,
            &[
                AndroidDeepLink {
                    scheme: "pushin".to_owned(),
                    host: None,
                    path_prefix: None,
                    auto_verify: false,
                },
                AndroidDeepLink {
                    scheme: "https".to_owned(),
                    host: Some("api.zechat.com.br".to_owned()),
                    path_prefix: Some("/reel/".to_owned()),
                    auto_verify: true,
                },
            ],
        )
        .expect("idempotent deep-link filters");
        let contents = fs::read_to_string(&manifest).expect("generated manifest");
        assert!(contents.contains("android:scheme=\"pushin\""));
        assert!(contents.contains("android:autoVerify=\"true\""));
        assert!(contents.contains("android:host=\"api.zechat.com.br\""));
        assert!(contents.contains("android:pathPrefix=\"/reel/\""));
        assert_eq!(
            contents
                .matches("android:name=\"android.intent.category.BROWSABLE\"")
                .count(),
            2
        );
        assert_eq!(contents.matches("<!-- pam:deep-links -->").count(), 1);
        fs::remove_file(manifest).expect("cleanup");
    }

    #[test]
    fn generator_names_are_safe_and_human_readable() {
        assert!(valid_pascal_name("Checkout"));
        assert!(valid_pascal_name("HTTPClient2"));
        assert!(!valid_pascal_name("checkout"));
        assert!(!valid_pascal_name("../Checkout"));
        assert_eq!(kebab_case("CheckoutForm"), "checkout-form");
        assert_eq!(kebab_case("HTTPClient"), "http-client");
    }

    #[test]
    fn android_bundle_ignores_hidden_paths_at_every_depth() {
        assert!(ignored_project_path(Path::new(".env")));
        assert!(ignored_project_path(Path::new(".pam-native/android")));
        assert!(ignored_project_path(Path::new(
            "vendor/package/.build/cache.php"
        )));
        assert!(ignored_project_path(Path::new(
            "vendor/package/resources/.generated/value.php"
        )));
        assert!(ignored_project_path(Path::new(
            "node_modules/package/index.js"
        )));
        assert!(ignored_project_path(Path::new("target/release/pam")));
        assert!(ignored_project_path(Path::new(
            "vendor/package/android/build/intermediates/classes.jar"
        )));
        assert!(ignored_project_path(Path::new(
            "vendor/package/examples/demo/vendor/autoload.php"
        )));
        assert!(!ignored_project_path(Path::new(
            "vendor/package/src/View.php"
        )));
        assert!(!ignored_project_path(Path::new(
            "vendor/composer/autoload.php"
        )));
        assert!(!ignored_project_path(Path::new(
            "resources/icons/pam-ui.svg"
        )));
    }

    #[test]
    fn android_bundle_digest_uses_portable_path_order() {
        let root = std::env::temp_dir().join(format!(
            "pam-mobile-digest-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("vendor/pam/native")).expect("pam package");
        fs::create_dir_all(root.join("vendor/pam-community/plugin")).expect("community package");
        fs::write(root.join("vendor/pam/native/file.php"), "native").expect("native file");
        fs::write(
            root.join("vendor/pam-community/plugin/file.php"),
            "community",
        )
        .expect("community file");

        let files = files_in(&root).expect("files");
        let relative = files
            .iter()
            .map(|file| {
                file.strip_prefix(&root)
                    .expect("relative")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            relative,
            [
                "vendor/pam-community/plugin/file.php",
                "vendor/pam/native/file.php",
            ]
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn generators_create_complete_files_and_refuse_overwrites() {
        let root = std::env::temp_dir().join(format!(
            "pam-mobile-generators-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("vendor")).expect("vendor");
        fs::write(root.join("vendor/autoload.php"), "<?php\n").expect("autoload");
        fs::write(root.join("index.php"), "<?php\n").expect("entry");
        fs::write(
            root.join(MANIFEST_NAME),
            r#"{
                "$schema": "vendor/pam/native/resources/pam-native.schema.json",
                "version": 1,
                "applicationId": "app.pam.generated",
                "name": "Generated",
                "entry": "index.php",
                "modules": [],
                "views": []
            }"#,
        )
        .expect("manifest");

        generate_screen(GeneratorOptions {
            name: "Orders".to_owned(),
            project: root.clone(),
        })
        .expect("screen");
        assert!(root.join("src/Screens/Orders.pam.php").is_file());
        assert!(
            generate_screen(GeneratorOptions {
                name: "Orders".to_owned(),
                project: root.clone(),
            })
            .is_err()
        );

        generate_component(GeneratorOptions {
            name: "MetricCard".to_owned(),
            project: root.clone(),
        })
        .expect("component");
        assert!(root.join("src/Components/MetricCard.pam.php").is_file());

        generate_flow(GeneratorOptions {
            name: "Checkout".to_owned(),
            project: root.clone(),
        })
        .expect("flow");
        assert!(root.join("src/Flows/Checkout.php").is_file());
        assert!(root.join("resources/native/checkout-flow.pam").is_file());
        assert!(root.join("tests/CheckoutFlowTest.php").is_file());
        assert!(
            generate_flow(GeneratorOptions {
                name: "Checkout".to_owned(),
                project: root.clone(),
            })
            .is_err()
        );

        generate_native_view(GeneratorOptions {
            name: "CameraPreview".to_owned(),
            project: root.clone(),
        })
        .expect("native view");
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(MANIFEST_NAME)).expect("read manifest"))
                .expect("json");
        assert_eq!(
            manifest["views"][0]["class"],
            "app.pam.generated.views.CameraPreviewFactory"
        );
        assert!(
            root.join("android/src/main/kotlin/app/pam/generated/views/CameraPreviewFactory.kt")
                .is_file()
        );

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn composer_plugins_are_discovered_locked_and_autolinked() {
        let root = std::env::temp_dir().join(format!(
            "pam-mobile-plugins-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let package = root.join("vendor/community/example");
        let native_package = root.join("vendor/pushinbr/pam-native");
        let composer = root.join("vendor/composer");
        let source = package.join("android/src/main/kotlin");
        fs::create_dir_all(&source).expect("plugin source");
        fs::create_dir_all(&native_package).expect("native package");
        fs::create_dir_all(&composer).expect("composer");
        fs::write(root.join("vendor/autoload.php"), "<?php\n").expect("autoload");
        fs::write(root.join("index.php"), "<?php\n").expect("entry");
        fs::write(
            root.join(MANIFEST_NAME),
            r#"{
                "version": 1,
                "applicationId": "app.pam.plugins",
                "name": "Plugins",
                "entry": "index.php",
                "android": {"minSdk": 26, "targetSdk": 36},
                "modules": [],
                "views": []
            }"#,
        )
        .expect("app manifest");
        fs::write(
            composer.join("installed.json"),
            r#"{
                "packages": [{
                    "name": "pushinbr/pam-native",
                    "version": "0.1.35",
                    "install-path": "../pushinbr/pam-native"
                }, {
                    "name": "community/example",
                    "version": "1.2.3",
                    "install-path": "../community/example",
                    "extra": {
                        "pam-native": {"plugin": "pam-native.plugin.json"}
                    }
                }]
            }"#,
        )
        .expect("installed");
        fs::write(
            package.join("pam-native.plugin.json"),
            r#"{
                "version": 1,
                "protocol": 1,
                "pamNative": {
                    "minimum": "0.1.0",
                    "maximumExclusive": "0.2.0"
                },
                "php": {"provider": "Community\\Example\\PluginProvider"},
                "android": {
                    "namespace": "community.example.plugin",
                    "minSdk": 26,
                    "sourceDirs": ["android/src/main/kotlin"],
                    "permissions": ["android.permission.CAMERA"],
                    "dependencies": ["androidx.core:core-ktx:1.17.0"]
                },
                "modules": [{
                    "name": "community.echo",
                    "class": "community.example.EchoModule"
                }],
                "views": [{
                    "name": "community.badge",
                    "class": "community.example.BadgeFactory"
                }]
            }"#,
        )
        .expect("plugin descriptor");

        let project = load_project(&root).expect("discover plugin");
        assert_eq!(project.plugins.len(), 1);
        assert_eq!(project.plugins[0].package, "community/example");

        let workspace = root.join(".pam-native/android");
        fs::create_dir_all(workspace.join("app/src/main/java/dev/pam/nativeapp/modules"))
            .expect("module destination");
        fs::create_dir_all(workspace.join("app/src/main/java/dev/pam/nativeapp/views"))
            .expect("view destination");
        generate_plugin_projects(&project, &workspace).expect("autolink");
        generate_modules(&project, &workspace).expect("module codegen");
        generate_views(&project, &workspace).expect("view codegen");
        write_plugin_lock(&project).expect("plugin lock");

        let build = fs::read_to_string(workspace.join("pam-plugins/plugin-0/build.gradle.kts"))
            .expect("generated Gradle");
        assert!(build.contains("api(project(\":plugin-api\"))"));
        assert!(build.contains("androidx.core:core-ktx:1.17.0"));
        let modules = fs::read_to_string(
            workspace.join("app/src/main/java/dev/pam/nativeapp/modules/GeneratedPamModules.kt"),
        )
        .expect("generated modules");
        assert!(modules.contains("community.example.EchoModule(context)"));
        let lock =
            fs::read_to_string(root.join(".pam-native/plugins.lock.json")).expect("generated lock");
        assert!(lock.contains("\"package\": \"community/example\""));
        assert!(lock.contains("\"protocol\": 1"));

        fs::write(
            root.join(MANIFEST_NAME),
            r#"{
                "version": 1,
                "applicationId": "app.pam.plugins",
                "name": "Plugins",
                "entry": "index.php",
                "android": {"minSdk": 26, "targetSdk": 36},
                "modules": [{
                    "name": "community.echo",
                    "class": "app.pam.plugins.EchoModule"
                }],
                "views": []
            }"#,
        )
        .expect("conflicting app manifest");
        let conflict = load_project(&root)
            .err()
            .expect("duplicate plugin binding must fail");
        assert!(conflict.contains("duplicate native module name"));

        fs::write(
            composer.join("installed.json"),
            r#"{
                "packages": [{
                    "name": "pushinbr/pam-native",
                    "version": "0.1.35",
                    "install-path": "../pushinbr/pam-native"
                }, {
                    "name": "community/example",
                    "version": "1.2.3",
                    "install-path": "../community/example",
                    "extra": {
                        "pam-native": {"plugin": "../../outside.json"}
                    }
                }]
            }"#,
        )
        .expect("unsafe installed metadata");
        let traversal = load_project(&root)
            .err()
            .expect("descriptor traversal must fail");
        assert!(traversal.contains("unsafe project-relative path"));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
#[test]
fn installed_sdk_versions_accept_stable_and_composer_dev_lines() {
    assert_eq!(parse_installed_sdk_version("0.2.1"), Ok((0, 2, 1)));
    assert_eq!(parse_installed_sdk_version("0.2.x-dev"), Ok((0, 2, 0)));
    assert!(parse_installed_sdk_version("dev-main").is_err());
    assert!(parse_release_version("0.2.x-dev").is_err());
}
