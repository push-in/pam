use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MANIFEST_NAME: &str = "pam-native.json";
const DEFAULT_PORT: u16 = 39_100;
const MAX_PROJECT_FILES: usize = 10_000;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PROJECT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DEV_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
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
    ios: IosOptions,
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
    #[serde(default = "default_ios_minimum_version")]
    ios_minimum_version: String,
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
    ios_root: PathBuf,
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
    #[serde(default)]
    share_targets: Vec<String>,
}

impl Default for AndroidOptions {
    fn default() -> Self {
        Self {
            min_sdk: default_min_sdk(),
            target_sdk: default_target_sdk(),
            permissions: Vec::new(),
            deep_links: Vec::new(),
            share_targets: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IosOptions {
    #[serde(default = "default_ios_minimum_version")]
    minimum_version: String,
}

impl Default for IosOptions {
    fn default() -> Self {
        Self {
            minimum_version: default_ios_minimum_version(),
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
    #[serde(rename = "iosClass", default)]
    ios_class: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeView {
    name: String,
    class: String,
    #[serde(rename = "iosClass", default)]
    ios_class: Option<String>,
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
    ios: PluginIos,
    #[serde(default)]
    idl: Option<PathBuf>,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginIos {
    #[serde(default = "default_ios_minimum_version")]
    minimum_version: String,
    #[serde(default)]
    source_dirs: Vec<PathBuf>,
    #[serde(default)]
    resource_dirs: Vec<PathBuf>,
    #[serde(default)]
    swift_packages: Vec<PluginSwiftPackage>,
    #[serde(default)]
    frameworks: Vec<String>,
    #[serde(default)]
    usage_descriptions: BTreeMap<String, String>,
    #[serde(default)]
    entitlements: Option<PathBuf>,
    #[serde(default)]
    info_plist: Option<PathBuf>,
    #[serde(default)]
    extensions: Vec<PluginIosExtension>,
}

impl Default for PluginIos {
    fn default() -> Self {
        Self {
            minimum_version: default_ios_minimum_version(),
            source_dirs: Vec::new(),
            resource_dirs: Vec::new(),
            swift_packages: Vec::new(),
            frameworks: Vec::new(),
            usage_descriptions: BTreeMap::new(),
            entitlements: None,
            info_plist: None,
            extensions: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginSwiftPackage {
    url: String,
    requirement: PluginSwiftPackageRequirement,
    products: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PluginSwiftPackageRequirement {
    kind: SwiftPackageRequirementKind,
    value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(try_from = "u8", into = "u8")]
enum SwiftPackageRequirementKind {
    Exact = 1,
    From = 2,
    Branch = 3,
    Revision = 4,
    UpToNextMinor = 5,
}

impl TryFrom<u8> for SwiftPackageRequirementKind {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Exact),
            2 => Ok(Self::From),
            3 => Ok(Self::Branch),
            4 => Ok(Self::Revision),
            5 => Ok(Self::UpToNextMinor),
            _ => {
                Err("Swift package requirement kind must be an integer from 1 through 5".to_owned())
            }
        }
    }
}

impl From<SwiftPackageRequirementKind> for u8 {
    fn from(value: SwiftPackageRequirementKind) -> Self {
        value as Self
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PluginIosExtension {
    kind: IosExtensionKind,
    name: String,
    bundle_suffix: String,
    #[serde(default)]
    source_dirs: Vec<PathBuf>,
    #[serde(default)]
    resource_dirs: Vec<PathBuf>,
    #[serde(default)]
    entitlements: Option<PathBuf>,
    #[serde(default)]
    info_plist: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(try_from = "u8", into = "u8")]
enum IosExtensionKind {
    Share = 1,
    Widget = 2,
    NotificationService = 3,
    Intents = 4,
    LiveActivity = 5,
}

impl TryFrom<u8> for IosExtensionKind {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Share),
            2 => Ok(Self::Widget),
            3 => Ok(Self::NotificationService),
            4 => Ok(Self::Intents),
            5 => Ok(Self::LiveActivity),
            _ => Err("iOS extension kind must be an integer from 1 through 5".to_owned()),
        }
    }
}

impl From<IosExtensionKind> for u8 {
    fn from(value: IosExtensionKind) -> Self {
        value as Self
    }
}

#[derive(Debug)]
struct NativePlugin {
    package: String,
    package_version: String,
    root: PathBuf,
    descriptor: PathBuf,
    descriptor_digest: String,
    idl_digest: Option<String>,
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
    idl_sha256: Option<&'a str>,
    php_provider: Option<&'a str>,
    modules: Vec<&'a str>,
    views: Vec<&'a str>,
    android_dependencies: Vec<&'a str>,
    ios_minimum_version: &'a str,
    ios_source_directories: Vec<&'a Path>,
    ios_resource_directories: Vec<&'a Path>,
    ios_swift_packages: Vec<&'a str>,
    ios_frameworks: Vec<&'a str>,
    ios_extensions: Vec<&'a str>,
}

fn default_min_sdk() -> u32 {
    26
}

fn default_ios_minimum_version() -> String {
    "15.0".to_owned()
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
        "ios:prepare" => {
            let project = load_project(&parse_project_only(arguments)?)?;
            let workspace = prepare_ios(&project)?;
            println!(
                "Prepared {} for iOS at {}",
                project.manifest.name,
                workspace.display()
            );
            Ok(0)
        }
        "ios:doctor" => doctor_ios(parse_project_only(arguments)?),
        "ios:build" => build_ios(parse_project_only(arguments)?, false).map(|_| 0),
        "ios:run" => run_ios(parse_project_only(arguments)?),
        "ios:dev" => dev_ios(parse_project_only(arguments)?),
        "ios:sign" => signing_status_ios(parse_project_only(arguments)?),
        "ios:package" => package_ios(parse_project_only(arguments)?),
        "ios:logs" => logs_ios(parse_project_only(arguments)?),
        "ios:devices" => devices_ios(parse_project_only(arguments)?),
        "build" => {
            let options = parse_options(arguments, true)?;
            build(options).map(|_| 0)
        }
        "package" => {
            let mut options = parse_options(arguments, true)?;
            options.mode = BuildMode::Release;
            package_android(options)
        }
        "sign" => signing_status(parse_project_only(arguments)?),
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
        "logs" => logs(parse_project_only(arguments)?),
        "devices" => devices(parse_project_only(arguments)?),
        "plugin:list" => list_plugins(parse_project_only(arguments)?),
        "plugin:doctor" => doctor_plugins(parse_project_only(arguments)?),
        "runtime:list" => list_runtimes(parse_project_only(arguments)?),
        "runtime:info" => runtime_info(parse_project_only(arguments)?),
        "runtime:use" => runtime_use(parse_runtime_use(arguments)?),
        "runtime:update" => runtime_update(parse_project_only(arguments)?),
        "make:screen" => generate_screen(parse_generator(arguments)?),
        "make:component" => generate_component(parse_generator(arguments)?),
        "make:native-view" => generate_native_view(parse_generator(arguments)?),
        unknown => Err(format!(
            "unknown mobile command {unknown:?}; run `pam mobile --help`"
        )),
    }
}

fn logs(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let application_id = debug_application_id(&project);
    println!("Streaming Android logs for {application_id}. Press Ctrl+C to stop.");
    let status = Command::new("adb")
        .args([
            "logcat",
            &format!("--pid={}", android_pid(&application_id)?),
        ])
        .status()
        .map_err(|error| format!("cannot start adb logcat: {error}"))?;
    Ok(if status.success() { 0 } else { 1 })
}

fn android_pid(application_id: &str) -> Result<String, String> {
    let output = Command::new("adb")
        .args(["shell", "pidof", application_id])
        .output()
        .map_err(|error| format!("cannot query {application_id}: {error}"))?;
    let pid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() && !pid.is_empty() {
        Ok(pid)
    } else {
        Err(format!(
            "{application_id} is not running on the connected Android device"
        ))
    }
}

fn devices(project_path: PathBuf) -> Result<u8, String> {
    load_project(&project_path)?;
    println!("Android devices");
    command_status("adb", &["devices", "-l"])?;
    if cfg!(target_os = "macos") {
        println!("\niOS simulators");
        command_status("xcrun", &["simctl", "list", "devices", "available"])?;
    }
    Ok(0)
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
    let ios_version = parse_ios_version(&manifest.ios.minimum_version)
        .ok_or_else(|| "ios.minimumVersion must use major.minor format".to_owned())?;
    if ios_version < (15, 0) {
        return Err("PAM Native requires iOS 15.0 or newer".to_owned());
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
    for mime_type in &manifest.android.share_targets {
        if !valid_mime_type(mime_type) {
            return Err(format!(
                "invalid Android share-target MIME type {mime_type:?}"
            ));
        }
    }
    Ok(())
}

fn valid_mime_type(value: &str) -> bool {
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    let valid = |part: &str| {
        !part.is_empty()
            && part.chars().all(|character| {
                character.is_ascii_alphanumeric()
                    || matches!(
                        character,
                        '!' | '#' | '$' | '&' | '^' | '_' | '.' | '+' | '*' | '-'
                    )
            })
    };
    valid(kind) && (subtype == "*" || valid(subtype))
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
        .find(|package| package.name == "pushinbr/pam-native")
        .map(|package| package.version.clone())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned());
    let current_version = parse_release_version(&current_version_text)?;
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
                "Composer package {} install path escapes the project vendor directory",
                package.name
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
        let idl_digest = plugin_manifest
            .idl
            .as_ref()
            .map(|path| digest_plugin_idl(&package.name, &package_root, path))
            .transpose()?;
        plugins.push(NativePlugin {
            package: package.name,
            package_version: package.version,
            root: package_root,
            descriptor,
            descriptor_digest: format!("{:x}", Sha256::digest(&descriptor_bytes)),
            idl_digest,
            manifest: plugin_manifest,
        });
    }

    plugins.sort_by(|left, right| left.package.cmp(&right.package));
    validate_plugin_bindings(app, &plugins)?;
    Ok(plugins)
}

fn installed_pam_native_version(root: &Path) -> Result<String, String> {
    let installed_path = root.join("vendor/composer/installed.json");
    let contents = fs::read_to_string(&installed_path)
        .map_err(|error| format!("cannot read {}: {error}", installed_path.display()))?;
    let installed: ComposerInstalled = serde_json::from_str(&contents)
        .map_err(|error| format!("invalid {}: {error}", installed_path.display()))?;
    installed
        .packages()
        .iter()
        .find(|package| package.name == "pushinbr/pam-native")
        .map(|package| package.version.clone())
        .ok_or_else(|| "Composer package pushinbr/pam-native is not installed".to_owned())
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
            current_version_text
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
    for path in android_plugin_paths(&manifest.android) {
        validate_plugin_path(package, root, path)?;
    }
    if !valid_ios_version(&manifest.ios.minimum_version) {
        return Err(format!(
            "plugin {package} has invalid iOS minimumVersion {:?}; expected major.minor",
            manifest.ios.minimum_version
        ));
    }
    let plugin_ios =
        parse_ios_version(&manifest.ios.minimum_version).expect("validated plugin iOS version");
    let app_ios =
        parse_ios_version(&app.ios.minimum_version).expect("validated application iOS version");
    if plugin_ios > app_ios {
        return Err(format!(
            "plugin {package} requires iOS {}, but the app minimum is {}; raise ios.minimumVersion in {MANIFEST_NAME}",
            manifest.ios.minimum_version, app.ios.minimum_version
        ));
    }
    for path in ios_plugin_paths(&manifest.ios) {
        validate_plugin_path(package, root, path)?;
        let expects_directory = manifest.ios.source_dirs.contains(path)
            || manifest.ios.resource_dirs.contains(path)
            || manifest.ios.extensions.iter().any(|extension| {
                extension.source_dirs.contains(path) || extension.resource_dirs.contains(path)
            });
        if expects_directory && !root.join(path).is_dir() {
            return Err(format!(
                "plugin {package} iOS path {} must be a directory",
                path.display()
            ));
        }
    }
    validate_ios_integration(package, &manifest.ios)?;
    if let Some(idl) = &manifest.idl {
        validate_plugin_path(package, root, idl)?;
        if !root.join(idl).is_file() {
            return Err(format!(
                "plugin {package} IDL {} must be a file",
                idl.display()
            ));
        }
    }
    for binding in &manifest.modules {
        validate_binding(package, "module", &binding.name, &binding.class)?;
        validate_ios_binding(package, "module", binding.ios_class.as_deref())?;
    }
    for binding in &manifest.views {
        validate_binding(package, "view", &binding.name, &binding.class)?;
        validate_ios_binding(package, "view", binding.ios_class.as_deref())?;
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

fn android_plugin_paths(android: &PluginAndroid) -> Vec<&PathBuf> {
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

fn ios_plugin_paths(ios: &PluginIos) -> Vec<&PathBuf> {
    let mut paths = ios
        .source_dirs
        .iter()
        .chain(ios.resource_dirs.iter())
        .collect::<Vec<_>>();
    paths.extend(ios.entitlements.iter());
    paths.extend(ios.info_plist.iter());
    for extension in &ios.extensions {
        paths.extend(extension.source_dirs.iter());
        paths.extend(extension.resource_dirs.iter());
        paths.extend(extension.entitlements.iter());
        paths.extend(extension.info_plist.iter());
    }
    paths
}

fn validate_ios_integration(package: &str, ios: &PluginIos) -> Result<(), String> {
    let mut package_urls = HashSet::new();
    for dependency in &ios.swift_packages {
        if !dependency.url.starts_with("https://")
            || !dependency.url.ends_with(".git")
            || dependency.url.len() > 2048
            || dependency.url.contains(['\n', '\r', '\0', '"'])
        {
            return Err(format!(
                "plugin {package} Swift package URL must be a safe HTTPS .git URL"
            ));
        }
        if !package_urls.insert(&dependency.url) {
            return Err(format!(
                "plugin {package} declares Swift package {} more than once",
                dependency.url
            ));
        }
        if dependency.requirement.value.is_empty()
            || dependency.requirement.value.len() > 160
            || dependency
                .requirement
                .value
                .contains(['\n', '\r', '\0', '"'])
        {
            return Err(format!(
                "plugin {package} Swift package requirement value is invalid"
            ));
        }
        if dependency.products.is_empty()
            || dependency
                .products
                .iter()
                .any(|product| !valid_apple_identifier(product))
        {
            return Err(format!(
                "plugin {package} Swift package products must be non-empty safe identifiers"
            ));
        }
    }
    if ios
        .frameworks
        .iter()
        .any(|framework| !valid_apple_identifier(framework))
    {
        return Err(format!(
            "plugin {package} contains an invalid Apple framework name"
        ));
    }
    for (key, value) in &ios.usage_descriptions {
        if !(key.starts_with("NS") || key == "NFCReaderUsageDescription")
            || !key.ends_with("UsageDescription")
            || !valid_apple_identifier(key)
            || value.trim().is_empty()
            || value.len() > 1000
            || value.contains('\0')
        {
            return Err(format!(
                "plugin {package} contains an invalid iOS usage description"
            ));
        }
    }
    let mut extension_names = HashSet::new();
    let mut bundle_suffixes = HashSet::new();
    for extension in &ios.extensions {
        if !valid_apple_identifier(&extension.name)
            || !valid_bundle_suffix(&extension.bundle_suffix)
            || !extension_names.insert(&extension.name)
            || !bundle_suffixes.insert(&extension.bundle_suffix)
            || extension.source_dirs.is_empty()
        {
            return Err(format!(
                "plugin {package} contains an invalid iOS extension declaration"
            ));
        }
    }
    Ok(())
}

fn valid_apple_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'+'))
}

fn valid_bundle_suffix(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 120
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
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

fn digest_plugin_idl(package: &str, root: &Path, path: &Path) -> Result<String, String> {
    let resolved = fs::canonicalize(root.join(path)).map_err(|error| {
        format!(
            "plugin {package} IDL {} cannot be resolved: {error}",
            path.display()
        )
    })?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err(format!(
            "plugin {package} IDL {} must be a package file",
            path.display()
        ));
    }
    let metadata = fs::metadata(&resolved)
        .map_err(|error| format!("cannot inspect plugin {package} IDL: {error}"))?;
    if metadata.len() > PLUGIN_MANIFEST_MAX_BYTES {
        return Err(format!(
            "plugin {package} IDL exceeds the one MiB safety limit"
        ));
    }
    let bytes = fs::read(&resolved)
        .map_err(|error| format!("cannot read plugin {package} IDL: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
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

fn validate_ios_binding(package: &str, kind: &str, class: Option<&str>) -> Result<(), String> {
    if let Some(class) = class
        && !valid_swift_class_name(class)
    {
        return Err(format!(
            "plugin {package} has invalid Swift {kind} class {class:?}"
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
    let Some(permission) = value.strip_prefix("android.permission.") else {
        return false;
    };
    if permission.is_empty() || value.len() > 160 {
        return false;
    }
    let parts = permission.split('.').collect::<Vec<_>>();
    parts.iter().enumerate().all(|(index, part)| {
        !part.is_empty()
            && part.bytes().all(|byte| {
                if index + 1 == parts.len() {
                    byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
                } else {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                }
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

fn valid_swift_class_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value.split('.').all(|part| {
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

fn valid_ios_version(value: &str) -> bool {
    parse_ios_version(value).is_some()
}

fn parse_ios_version(value: &str) -> Option<(u32, u32)> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((major, minor))
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
        ios_root: pam_home.join("runtime/ios").join(&id),
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
    let java_version = tool_version("java", &["-version"]);
    let java_ready =
        command_exists("java") && java_major_version(&java_version).is_some_and(|v| v >= 17);
    healthy &= java_ready;
    check("Java 17+", java_ready, java_version);
    healthy &= command_exists("cargo");
    check(
        "Rust",
        command_exists("cargo"),
        tool_version("rustc", &["--version"]),
    );
    let sdk = android_sdk();
    healthy &= sdk.is_ok();
    check(
        "Android SDK",
        sdk.is_ok(),
        sdk.as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|error| error.clone()),
    );
    let ndk_version = &runtime.release.ndk_version;
    let ndk = sdk
        .as_ref()
        .map(|path| path.join("ndk").join(ndk_version))
        .is_ok_and(|path| path.is_dir());
    healthy &= ndk;
    check("Android NDK", ndk, ndk_version.clone());
    let platform = sdk
        .as_ref()
        .map(|path| {
            path.join(format!(
                "platforms/android-{}",
                project.manifest.android.target_sdk
            ))
        })
        .is_ok_and(|path| path.is_dir());
    healthy &= platform;
    check(
        "Android platform",
        platform,
        format!("android-{}", project.manifest.android.target_sdk),
    );
    let cmake = sdk
        .as_ref()
        .map(|path| path.join("cmake/3.22.1"))
        .is_ok_and(|path| path.is_dir());
    healthy &= cmake;
    check("CMake", cmake, "3.22.1".to_owned());
    let adb = sdk
        .as_ref()
        .map(|path| path.join("platform-tools/adb"))
        .is_ok_and(|path| path.is_file());
    healthy &= adb;
    check("ADB", adb, "Android platform-tools".to_owned());
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
    let installed = installed_rust_targets().unwrap_or_default();
    for abi in default_abis() {
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
    if healthy {
        println!("\nPam Native is ready to build Android applications.");
        Ok(0)
    } else {
        Err("Pam Native doctor found blocking Android requirements".to_owned())
    }
}

fn ios_runtime_ready(runtime: &ResolvedRuntime) -> bool {
    runtime
        .ios_root
        .join("PamPhp.xcframework/Info.plist")
        .is_file()
        && runtime
            .ios_root
            .join("PamNativeEngine.xcframework/Info.plist")
            .is_file()
        && runtime.ios_root.join("runtime.json").is_file()
}

fn doctor_ios(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let runtime = resolve_runtime(&project, &pam_home()?)?;
    let mut healthy = cfg!(target_os = "macos");
    println!("Pam Native iOS doctor\n");
    check(
        "macOS host",
        cfg!(target_os = "macos"),
        std::env::consts::OS.to_owned(),
    );
    // `xcodebuild` uses the historical single-dash spelling for this flag.
    // The generic command probe calls `--version`, which makes a healthy
    // Xcode installation look unavailable on macOS runners and developer Macs.
    let xcode = Command::new("xcodebuild")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    healthy &= xcode;
    check("Xcode", xcode, tool_version("xcodebuild", &["-version"]));
    let simctl = Command::new("xcrun")
        .args(["--find", "simctl"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    healthy &= simctl;
    check(
        "Xcode command line tools",
        simctl,
        tool_version("xcrun", &["--find", "simctl"]),
    );
    let runtime_ready = ios_runtime_ready(&runtime);
    healthy &= runtime_ready;
    check(
        &format!("PHP {} iOS runtime", runtime.release.php_version),
        runtime_ready,
        runtime.ios_root.display().to_string(),
    );
    let targets = installed_rust_targets().unwrap_or_default();
    for target in [
        "aarch64-apple-ios",
        "aarch64-apple-ios-sim",
        "x86_64-apple-ios",
    ] {
        let available = targets.contains(target);
        healthy &= available;
        check(
            &format!("Rust target ({target})"),
            available,
            if available {
                "installed".to_owned()
            } else {
                format!("run: rustup target add {target}")
            },
        );
    }
    if healthy {
        println!("\nPam Native is ready to build iOS applications.");
        Ok(0)
    } else {
        Err("Pam Native doctor found blocking iOS requirements".to_owned())
    }
}

fn prepare_ios(project: &Project) -> Result<PathBuf, String> {
    let native_home = native_home()?;
    let runtime = resolve_runtime(project, &pam_home()?)?;
    if !ios_runtime_ready(&runtime) {
        return Err(format!(
            "verified PHP {} iOS runtime is missing; build it on macOS with `runtime-builder/ios/build.sh --php {} all` (expected {})",
            runtime.release.php_version,
            project.manifest.runtime.php,
            runtime.ios_root.display()
        ));
    }
    write_runtime_lock(project, &runtime)?;
    write_plugin_lock(project)?;
    write_ios_plugin_plan(project)?;

    let source = native_home.join("ios-host");
    let workspace = project.root.join(".pam-native/ios/App");
    fs::create_dir_all(&workspace)
        .map_err(|error| format!("cannot create {}: {error}", workspace.display()))?;
    prune_tree(&source, &workspace, &["build", "DerivedData"])?;
    copy_tree(&source, &workspace, &["build", "DerivedData"])?;

    let sdk = workspace.join("PamNativeSDK/ios");
    fs::create_dir_all(&sdk)
        .map_err(|error| format!("cannot create {}: {error}", sdk.display()))?;
    copy_tree(&native_home.join("ios"), &sdk, &[".build", ".swiftpm"])?;

    let bridge = workspace.join("Bridge");
    fs::create_dir_all(bridge.join("include"))
        .map_err(|error| format!("cannot create iOS bridge directory: {error}"))?;
    fs::copy(
        native_home.join("ios/Sources/PamNative/Bridge/pam_native_ios_bridge.cpp"),
        bridge.join("pam_native_ios_bridge.cpp"),
    )
    .map_err(|error| format!("cannot stage iOS bridge: {error}"))?;
    fs::copy(
        native_home.join("ios/include/pam_native_ios_bridge.h"),
        bridge.join("include/pam_native_ios_bridge.h"),
    )
    .map_err(|error| format!("cannot stage iOS bridge header: {error}"))?;

    let runtime_destination = workspace.join("Runtime");
    fs::create_dir_all(&runtime_destination)
        .map_err(|error| format!("cannot create iOS runtime directory: {error}"))?;
    for framework in ["PamPhp.xcframework", "PamNativeEngine.xcframework"] {
        let destination = runtime_destination.join(framework);
        if destination.exists() {
            fs::remove_dir_all(&destination)
                .map_err(|error| format!("cannot replace {}: {error}", destination.display()))?;
        }
        copy_tree(&runtime.ios_root.join(framework), &destination, &[])?;
    }

    write_ios_plugin_package(project, &workspace.join("PamNativeSDK"))?;
    let generated_plugins = project.root.join(".pam-native/ios/PamNativePlugins");
    let host_plugins = workspace.join("PamNativePlugins");
    if host_plugins.exists() {
        fs::remove_dir_all(&host_plugins)
            .map_err(|error| format!("cannot replace {}: {error}", host_plugins.display()))?;
    }
    copy_tree(&generated_plugins, &host_plugins, &[".build", ".swiftpm"])?;
    stage_project_at(project, &runtime_destination.join("PamBundle"))?;

    let has_app_entitlements = merge_ios_app_metadata(project, &workspace)?;
    integrate_ios_extensions(project, &workspace)?;

    let team = std::env::var("PAM_IOS_DEVELOPMENT_TEAM").unwrap_or_default();
    let code_sign_entitlements = if has_app_entitlements {
        "CODE_SIGN_ENTITLEMENTS = App/PamNativeApp.entitlements; "
    } else {
        ""
    };
    replace_ios_placeholders(
        &workspace.join("PamNativeApp.xcodeproj/project.pbxproj"),
        &[
            (
                "__PAM_APPLICATION_ID__",
                project.manifest.application_id.as_str(),
            ),
            ("__PAM_APPLICATION_NAME__", project.manifest.name.as_str()),
            (
                "__PAM_VERSION_CODE__",
                &project.manifest.version_code.to_string(),
            ),
            (
                "__PAM_VERSION_NAME__",
                project.manifest.version_name.as_str(),
            ),
            (
                "__PAM_IOS_MINIMUM__",
                project.manifest.ios.minimum_version.as_str(),
            ),
            ("__PAM_DEVELOPMENT_TEAM__", team.as_str()),
            ("__PAM_CODE_SIGN_ENTITLEMENTS__", code_sign_entitlements),
        ],
    )?;
    replace_ios_placeholders(
        &workspace.join("App/PamAppDelegate.swift"),
        &[
            ("__PAM_ENTRY_BASENAME__", "index"),
            ("__PAM_ENTRY_EXTENSION__", "php"),
        ],
    )?;
    Ok(workspace)
}

fn merge_ios_app_metadata(project: &Project, workspace: &Path) -> Result<bool, String> {
    let application_id = &project.manifest.application_id;
    let info_path = workspace.join("App/Info.plist");
    let mut info = read_apple_plist(&info_path)?;
    let mut entitlements = serde_json::json!({});
    let mut has_entitlements = false;

    for plugin in &project.plugins {
        let ios = &plugin.manifest.ios;
        if let Some(path) = &ios.info_plist {
            let path = canonical_plugin_path(plugin, path)?;
            let value = replace_plist_application_id(read_apple_plist(&path)?, application_id);
            merge_plist_value(
                &mut info,
                value,
                &format!("plugin {} Info.plist", plugin.package),
            )?;
        }
        for (key, value) in &ios.usage_descriptions {
            let object = info.as_object_mut().ok_or_else(|| {
                format!(
                    "{} must contain a top-level dictionary",
                    info_path.display()
                )
            })?;
            match object.get(key) {
                Some(existing) if existing != value => {
                    return Err(format!(
                        "plugin {} conflicts with the existing iOS value for {key}",
                        plugin.package
                    ));
                }
                Some(_) => {}
                None => {
                    object.insert(key.clone(), serde_json::Value::String(value.clone()));
                }
            }
        }
        if let Some(path) = &ios.entitlements {
            let path = canonical_plugin_path(plugin, path)?;
            let value = replace_plist_application_id(read_apple_plist(&path)?, application_id);
            merge_plist_value(
                &mut entitlements,
                value,
                &format!("plugin {} entitlements", plugin.package),
            )?;
            has_entitlements = true;
        }
    }

    write_apple_plist(&info_path, &info)?;
    if has_entitlements {
        write_apple_plist(
            &workspace.join("App/PamNativeApp.entitlements"),
            &entitlements,
        )?;
    }
    Ok(has_entitlements)
}

fn integrate_ios_extensions(project: &Project, workspace: &Path) -> Result<(), String> {
    let extensions = project
        .plugins
        .iter()
        .flat_map(|plugin| {
            plugin
                .manifest
                .ios
                .extensions
                .iter()
                .map(move |extension| (plugin, extension))
        })
        .collect::<Vec<_>>();
    if extensions.is_empty() {
        return Ok(());
    }

    let mut build_files = String::new();
    let mut file_references = String::new();
    let mut framework_phases = String::new();
    let mut groups = String::new();
    let mut native_targets = String::new();
    let mut resource_phases = String::new();
    let mut source_phases = String::new();
    let mut target_proxies = String::new();
    let mut target_dependencies = String::new();
    let mut configurations = String::new();
    let mut configuration_lists = String::new();
    let mut foundation_embed_files = Vec::new();
    let mut extensionkit_embed_files = Vec::new();
    let mut product_references = Vec::new();
    let mut extension_groups = Vec::new();
    let mut extension_targets = Vec::new();
    let mut app_dependencies = Vec::new();

    for (plugin, extension) in extensions {
        let name = &extension.name;
        let extension_root = workspace.join("Extensions").join(name);
        let sources_root = extension_root.join("Sources");
        let resources_root = extension_root.join("Resources");
        fs::create_dir_all(&sources_root)
            .map_err(|error| format!("cannot create iOS extension {name}: {error}"))?;
        for source in &extension.source_dirs {
            copy_ios_extension_sources(&canonical_plugin_path(plugin, source)?, &sources_root)?;
        }
        for resources in &extension.resource_dirs {
            fs::create_dir_all(&resources_root)
                .map_err(|error| format!("cannot create iOS extension resources: {error}"))?;
            copy_tree(
                &canonical_plugin_path(plugin, resources)?,
                &resources_root,
                &[],
            )?;
        }

        let info_path = extension_root.join("Info.plist");
        let mut info = ios_extension_base_plist(project, extension);
        if let Some(path) = &extension.info_plist {
            let value = replace_plist_application_id(
                read_apple_plist(&canonical_plugin_path(plugin, path)?)?,
                &project.manifest.application_id,
            );
            merge_plist_value(
                &mut info,
                value,
                &format!("plugin {} extension {name} Info.plist", plugin.package),
            )?;
        }
        normalize_ios_extension_plist(&mut info, extension.kind)?;
        write_apple_plist(&info_path, &info)?;

        let entitlements_setting = if let Some(path) = &extension.entitlements {
            let value = replace_plist_application_id(
                read_apple_plist(&canonical_plugin_path(plugin, path)?)?,
                &project.manifest.application_id,
            );
            write_apple_plist(&extension_root.join("Extension.entitlements"), &value)?;
            "CODE_SIGN_ENTITLEMENTS = Extensions/".to_owned() + name + "/Extension.entitlements; "
        } else {
            String::new()
        };

        let target_id = pbx_id(&format!("extension:{name}:target"));
        let product_id = pbx_id(&format!("extension:{name}:product"));
        let group_id = pbx_id(&format!("extension:{name}:group"));
        let sources_phase_id = pbx_id(&format!("extension:{name}:sources"));
        let resources_phase_id = pbx_id(&format!("extension:{name}:resources"));
        let frameworks_phase_id = pbx_id(&format!("extension:{name}:frameworks"));
        let debug_config_id = pbx_id(&format!("extension:{name}:debug"));
        let release_config_id = pbx_id(&format!("extension:{name}:release"));
        let config_list_id = pbx_id(&format!("extension:{name}:config-list"));
        let proxy_id = pbx_id(&format!("extension:{name}:proxy"));
        let dependency_id = pbx_id(&format!("extension:{name}:dependency"));
        let embed_id = pbx_id(&format!("extension:{name}:embed"));

        let mut group_children = Vec::new();
        let mut source_build_ids = Vec::new();
        let mut resource_build_ids = Vec::new();
        for path in collect_tree_files(&extension_root)? {
            let relative = path
                .strip_prefix(&extension_root)
                .map_err(|error| format!("cannot stage iOS extension file: {error}"))?;
            if matches!(
                relative.to_string_lossy().as_ref(),
                "Info.plist" | "Extension.entitlements"
            ) {
                continue;
            }
            let relative_text = relative.to_string_lossy().replace('\\', "/");
            let reference_id = pbx_id(&format!("extension:{name}:file:{relative_text}"));
            let build_id = pbx_id(&format!("extension:{name}:build:{relative_text}"));
            let file_type = xcode_file_type(relative);
            file_references.push_str(&format!(
                "\t\t{reference_id} /* {relative_text} */ = {{isa = PBXFileReference; lastKnownFileType = {file_type}; path = {}; sourceTree = \"<group>\"; }};\n",
                swift_string(&relative_text)
            ));
            group_children.push(format!("{reference_id} /* {relative_text} */"));
            let is_source = matches!(
                relative.extension().and_then(OsStr::to_str),
                Some("swift" | "m" | "mm" | "c" | "cc" | "cpp")
            );
            build_files.push_str(&format!(
                "\t\t{build_id} /* {relative_text} in {} */ = {{isa = PBXBuildFile; fileRef = {reference_id} /* {relative_text} */; }};\n",
                if is_source { "Sources" } else { "Resources" }
            ));
            if is_source {
                source_build_ids.push(format!("{build_id} /* {relative_text} in Sources */"));
            } else {
                resource_build_ids.push(format!("{build_id} /* {relative_text} in Resources */"));
            }
        }

        let is_extensionkit = extension.kind == IosExtensionKind::Intents;
        let product_name = format!("{name}.appex");
        let product_file_type = if is_extensionkit {
            "wrapper.extensionkit-extension"
        } else {
            "wrapper.app-extension"
        };
        let product_type = if is_extensionkit {
            "com.apple.product-type.extensionkit-extension"
        } else {
            "com.apple.product-type.app-extension"
        };
        let embed_phase_name = if is_extensionkit {
            "Embed ExtensionKit Extensions"
        } else {
            "Embed App Extensions"
        };
        build_files.push_str(&format!(
            "\t\t{embed_id} /* {product_name} in {embed_phase_name} */ = {{isa = PBXBuildFile; fileRef = {product_id} /* {product_name} */; settings = {{ATTRIBUTES = (CodeSignOnCopy, RemoveHeadersOnCopy, ); }}; }};\n"
        ));
        file_references.push_str(&format!(
            "\t\t{product_id} /* {product_name} */ = {{isa = PBXFileReference; explicitFileType = \"{product_file_type}\"; path = {}; sourceTree = BUILT_PRODUCTS_DIR; }};\n",
            swift_string(&product_name)
        ));
        groups.push_str(&format!(
            "\t\t{group_id} /* {name} */ = {{isa = PBXGroup; children = ({}); path = {}; sourceTree = \"<group>\"; }};\n",
            group_children.join(", "),
            swift_string(&format!("Extensions/{name}"))
        ));
        framework_phases.push_str(&format!(
            "\t\t{frameworks_phase_id} = {{isa = PBXFrameworksBuildPhase; buildActionMask = 2147483647; files = (); runOnlyForDeploymentPostprocessing = 0; }};\n"
        ));
        source_phases.push_str(&format!(
            "\t\t{sources_phase_id} = {{isa = PBXSourcesBuildPhase; buildActionMask = 2147483647; files = ({}); runOnlyForDeploymentPostprocessing = 0; }};\n",
            source_build_ids.join(", ")
        ));
        resource_phases.push_str(&format!(
            "\t\t{resources_phase_id} = {{isa = PBXResourcesBuildPhase; buildActionMask = 2147483647; files = ({}); runOnlyForDeploymentPostprocessing = 0; }};\n",
            resource_build_ids.join(", ")
        ));
        native_targets.push_str(&format!(
            "\t\t{target_id} /* {name} */ = {{isa = PBXNativeTarget; buildConfigurationList = {config_list_id}; buildPhases = ({sources_phase_id}, {frameworks_phase_id}, {resources_phase_id}); buildRules = (); dependencies = (); name = {name}; productName = {name}; productReference = {product_id} /* {product_name} */; productType = \"{product_type}\"; }};\n"
        ));
        target_proxies.push_str(&format!(
            "\t\t{proxy_id} = {{isa = PBXContainerItemProxy; containerPortal = 500000000000000000000002 /* Project object */; proxyType = 1; remoteGlobalIDString = {target_id}; remoteInfo = {name}; }};\n"
        ));
        target_dependencies.push_str(&format!(
            "\t\t{dependency_id} = {{isa = PBXTargetDependency; target = {target_id} /* {name} */; targetProxy = {proxy_id}; }};\n"
        ));
        let settings = format!(
            "APPLICATION_EXTENSION_API_ONLY = YES; {entitlements_setting}CODE_SIGN_STYLE = Automatic; CURRENT_PROJECT_VERSION = __PAM_VERSION_CODE__; DEVELOPMENT_TEAM = \"__PAM_DEVELOPMENT_TEAM__\"; GENERATE_INFOPLIST_FILE = NO; INFOPLIST_FILE = Extensions/{name}/Info.plist; IPHONEOS_DEPLOYMENT_TARGET = {}; MARKETING_VERSION = __PAM_VERSION_NAME__; PRODUCT_BUNDLE_IDENTIFIER = __PAM_APPLICATION_ID__.{}; PRODUCT_NAME = \"{name}\"; SDKROOT = iphoneos; SKIP_INSTALL = YES; SWIFT_VERSION = 5.0; TARGETED_DEVICE_FAMILY = \"1,2\"; ",
            plugin.manifest.ios.minimum_version, extension.bundle_suffix
        );
        configurations.push_str(&format!(
            "\t\t{debug_config_id} = {{isa = XCBuildConfiguration; buildSettings = {{{settings}}}; name = Debug; }};\n\t\t{release_config_id} = {{isa = XCBuildConfiguration; buildSettings = {{{settings}SWIFT_COMPILATION_MODE = wholemodule; VALIDATE_PRODUCT = YES; }}; name = Release; }};\n"
        ));
        configuration_lists.push_str(&format!(
            "\t\t{config_list_id} = {{isa = XCConfigurationList; buildConfigurations = ({debug_config_id}, {release_config_id}); defaultConfigurationIsVisible = 0; defaultConfigurationName = Release; }};\n"
        ));
        let embed_reference = format!("{embed_id} /* {product_name} in {embed_phase_name} */");
        if is_extensionkit {
            extensionkit_embed_files.push(embed_reference);
        } else {
            foundation_embed_files.push(embed_reference);
        }
        product_references.push(format!("{product_id} /* {product_name} */"));
        extension_groups.push(format!("{group_id} /* {name} */"));
        extension_targets.push(format!("{target_id} /* {name} */"));
        app_dependencies.push(dependency_id);
    }

    let foundation_copy_phase_id = pbx_id("extensions:foundation-embed-phase");
    let extensionkit_copy_phase_id = pbx_id("extensions:extensionkit-embed-phase");
    let mut copy_phases = String::from("/* Begin PBXCopyFilesBuildPhase section */\n");
    let mut app_copy_phases = Vec::new();
    if !foundation_embed_files.is_empty() {
        copy_phases.push_str(&format!(
            "\t\t{foundation_copy_phase_id} /* Embed App Extensions */ = {{isa = PBXCopyFilesBuildPhase; buildActionMask = 2147483647; dstPath = \"\"; dstSubfolderSpec = 13; files = ({}); name = \"Embed App Extensions\"; runOnlyForDeploymentPostprocessing = 0; }};\n",
            foundation_embed_files.join(", ")
        ));
        app_copy_phases.push(foundation_copy_phase_id.clone());
    }
    if !extensionkit_embed_files.is_empty() {
        copy_phases.push_str(&format!(
            "\t\t{extensionkit_copy_phase_id} /* Embed ExtensionKit Extensions */ = {{isa = PBXCopyFilesBuildPhase; buildActionMask = 2147483647; dstPath = \"$(EXTENSIONS_FOLDER_PATH)\"; dstSubfolderSpec = 16; files = ({}); name = \"Embed ExtensionKit Extensions\"; runOnlyForDeploymentPostprocessing = 0; }};\n",
            extensionkit_embed_files.join(", ")
        ));
        app_copy_phases.push(extensionkit_copy_phase_id.clone());
    }
    copy_phases.push_str("/* End PBXCopyFilesBuildPhase section */\n\n");
    let project_path = workspace.join("PamNativeApp.xcodeproj/project.pbxproj");
    let mut pbx = fs::read_to_string(&project_path)
        .map_err(|error| format!("cannot read {}: {error}", project_path.display()))?;
    insert_pbx_section(&mut pbx, "/* End PBXBuildFile section */", &build_files)?;
    insert_pbx_section(
        &mut pbx,
        "/* Begin PBXFileReference section */",
        &copy_phases,
    )?;
    insert_pbx_section(
        &mut pbx,
        "/* End PBXFileReference section */",
        &file_references,
    )?;
    insert_pbx_section(
        &mut pbx,
        "/* End PBXFrameworksBuildPhase section */",
        &framework_phases,
    )?;
    insert_pbx_section(&mut pbx, "/* End PBXGroup section */", &groups)?;
    insert_pbx_section(
        &mut pbx,
        "/* End PBXNativeTarget section */",
        &native_targets,
    )?;
    insert_pbx_section(
        &mut pbx,
        "/* End PBXResourcesBuildPhase section */",
        &resource_phases,
    )?;
    insert_pbx_section(
        &mut pbx,
        "/* End PBXSourcesBuildPhase section */",
        &source_phases,
    )?;
    insert_pbx_section(
        &mut pbx,
        "/* Begin PBXBuildFile section */",
        &format!(
            "/* Begin PBXContainerItemProxy section */\n{target_proxies}/* End PBXContainerItemProxy section */\n\n"
        ),
    )?;
    insert_pbx_section(
        &mut pbx,
        "/* Begin XCBuildConfiguration section */",
        &format!(
            "/* Begin PBXTargetDependency section */\n{target_dependencies}/* End PBXTargetDependency section */\n\n"
        ),
    )?;
    insert_pbx_section(
        &mut pbx,
        "/* End XCBuildConfiguration section */",
        &configurations,
    )?;
    insert_pbx_section(
        &mut pbx,
        "/* End XCConfigurationList section */",
        &configuration_lists,
    )?;
    replace_pbx_once(
        &mut pbx,
        "buildPhases = (300000000000000000000003, 300000000000000000000001, 300000000000000000000002);",
        &format!(
            "buildPhases = (300000000000000000000003, 300000000000000000000001, 300000000000000000000002, {});",
            app_copy_phases.join(", ")
        ),
    )?;
    replace_pbx_once(
        &mut pbx,
        "dependencies = (); name = PamNativeApp;",
        &format!(
            "dependencies = ({}); name = PamNativeApp;",
            app_dependencies.join(", ")
        ),
    )?;
    replace_pbx_once(
        &mut pbx,
        "children = (400000000000000000000002, 400000000000000000000003, 400000000000000000000004, 400000000000000000000005);",
        &format!(
            "children = (400000000000000000000002, 400000000000000000000003, 400000000000000000000004, {}, 400000000000000000000005);",
            extension_groups.join(", ")
        ),
    )?;
    replace_pbx_once(
        &mut pbx,
        "children = (200000000000000000000006); name = Products;",
        &format!(
            "children = (200000000000000000000006, {}); name = Products;",
            product_references.join(", ")
        ),
    )?;
    replace_pbx_once(
        &mut pbx,
        "targets = (500000000000000000000001);",
        &format!(
            "targets = (500000000000000000000001, {});",
            extension_targets.join(", ")
        ),
    )?;
    write_atomic(&project_path, pbx.as_bytes())
}

fn ios_extension_base_plist(
    project: &Project,
    extension: &PluginIosExtension,
) -> serde_json::Value {
    let point = match extension.kind {
        IosExtensionKind::Share => "com.apple.share-services",
        IosExtensionKind::Widget | IosExtensionKind::LiveActivity => {
            "com.apple.widgetkit-extension"
        }
        IosExtensionKind::NotificationService => "com.apple.usernotifications.service",
        IosExtensionKind::Intents => "com.apple.appintents-extension",
    };
    let mut plist = serde_json::json!({
        "CFBundleDevelopmentRegion": "$(DEVELOPMENT_LANGUAGE)",
        "CFBundleDisplayName": extension.name,
        "CFBundleExecutable": "$(EXECUTABLE_NAME)",
        "CFBundleIdentifier": "$(PRODUCT_BUNDLE_IDENTIFIER)",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": "$(PRODUCT_NAME)",
        "CFBundlePackageType": "$(PRODUCT_BUNDLE_PACKAGE_TYPE)",
        "CFBundleShortVersionString": project.manifest.version_name,
        "CFBundleVersion": project.manifest.version_code.to_string()
    });
    let extension_key = if extension.kind == IosExtensionKind::Intents {
        "EXAppExtensionAttributes"
    } else {
        "NSExtension"
    };
    let point_key = if extension.kind == IosExtensionKind::Intents {
        "EXExtensionPointIdentifier"
    } else {
        "NSExtensionPointIdentifier"
    };
    plist[extension_key] = serde_json::json!({point_key: point});
    plist
}

fn normalize_ios_extension_plist(
    plist: &mut serde_json::Value,
    kind: IosExtensionKind,
) -> Result<(), String> {
    let object = plist
        .as_object_mut()
        .ok_or_else(|| "iOS extension Info.plist must be a dictionary".to_owned())?;
    if kind == IosExtensionKind::Intents {
        object.remove("NSExtension");
        object.insert(
            "EXAppExtensionAttributes".to_owned(),
            serde_json::json!({
                "EXExtensionPointIdentifier": "com.apple.appintents-extension"
            }),
        );
    }
    Ok(())
}

fn collect_tree_files(root: &Path) -> Result<Vec<PathBuf>, String> {
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
    files.sort();
    Ok(files)
}

fn copy_ios_extension_sources(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in fs::read_dir(source)
        .map_err(|error| format!("cannot read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "refusing symlink in iOS extension sources: {}",
                entry.path().display()
            ));
        }
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
            copy_ios_extension_sources(&entry.path(), &target)?;
        } else if file_type.is_file()
            && !matches!(
                entry.path().extension().and_then(OsStr::to_str),
                Some("plist" | "entitlements")
            )
        {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("cannot copy {}: {error}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn xcode_file_type(path: &Path) -> &'static str {
    match path.extension().and_then(OsStr::to_str) {
        Some("swift") => "sourcecode.swift",
        Some("m") => "sourcecode.c.objc",
        Some("mm") => "sourcecode.cpp.objcpp",
        Some("c") => "sourcecode.c.c",
        Some("cc" | "cpp") => "sourcecode.cpp.cpp",
        Some("plist") => "text.plist.xml",
        Some("xcassets") => "folder.assetcatalog",
        Some("strings") => "text.plist.strings",
        Some("json") => "text.json",
        _ => "file",
    }
}

fn pbx_id(label: &str) -> String {
    Sha256::digest(label.as_bytes())[..12]
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

fn insert_pbx_section(project: &mut String, marker: &str, contents: &str) -> Result<(), String> {
    let position = project
        .find(marker)
        .ok_or_else(|| format!("generated Xcode template is missing {marker}"))?;
    project.insert_str(position, contents);
    Ok(())
}

fn replace_pbx_once(project: &mut String, expected: &str, replacement: &str) -> Result<(), String> {
    if project.matches(expected).count() != 1 {
        return Err(format!(
            "generated Xcode template must contain exactly one {expected:?}"
        ));
    }
    *project = project.replacen(expected, replacement, 1);
    Ok(())
}

fn read_apple_plist(path: &Path) -> Result<serde_json::Value, String> {
    let output = Command::new("plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(path)
        .output()
        .map_err(|error| format!("cannot start plutil for {}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "cannot read Apple property list {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid property list {}: {error}", path.display()))
}

fn write_apple_plist(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot encode {}: {error}", path.display()))?;
    bytes.push(b'\n');
    write_atomic(path, &bytes)?;
    let output = Command::new("plutil")
        .args(["-convert", "xml1"])
        .arg(path)
        .output()
        .map_err(|error| format!("cannot start plutil for {}: {error}", path.display()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "cannot write Apple property list {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn replace_plist_application_id(
    value: serde_json::Value,
    application_id: &str,
) -> serde_json::Value {
    match value {
        serde_json::Value::String(value) => {
            serde_json::Value::String(value.replace("$(PAM_NATIVE_APPLICATION_ID)", application_id))
        }
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .map(|value| replace_plist_application_id(value, application_id))
                .collect(),
        ),
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, replace_plist_application_id(value, application_id)))
                .collect(),
        ),
        value => value,
    }
}

fn merge_plist_value(
    target: &mut serde_json::Value,
    incoming: serde_json::Value,
    source: &str,
) -> Result<(), String> {
    match (target, incoming) {
        (serde_json::Value::Object(target), serde_json::Value::Object(incoming)) => {
            for (key, value) in incoming {
                if let Some(existing) = target.get_mut(&key) {
                    merge_plist_value(existing, value, source)
                        .map_err(|error| format!("{error} at {key}"))?;
                } else {
                    target.insert(key, value);
                }
            }
            Ok(())
        }
        (serde_json::Value::Array(target), serde_json::Value::Array(incoming)) => {
            for value in incoming {
                if !target.contains(&value) {
                    target.push(value);
                }
            }
            Ok(())
        }
        (target, incoming) if *target == incoming => Ok(()),
        _ => Err(format!(
            "conflicting Apple property-list values from {source}"
        )),
    }
}

fn replace_ios_placeholders(path: &Path, replacements: &[(&str, &str)]) -> Result<(), String> {
    let mut contents = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    for (placeholder, value) in replacements {
        if value.contains(['\n', '\r', '\0', '"']) {
            return Err(format!("unsafe iOS project value for {placeholder}"));
        }
        contents = contents.replace(placeholder, value);
    }
    write_atomic(path, contents.as_bytes())
}

fn build_ios(project_path: PathBuf, release: bool) -> Result<PathBuf, String> {
    if !cfg!(target_os = "macos") {
        return Err("iOS builds require macOS with Xcode".to_owned());
    }
    let project = load_project(&project_path)?;
    let workspace = prepare_ios(&project)?;
    let configuration = if release { "Release" } else { "Debug" };
    let derived = workspace.join("DerivedData");
    let status = Command::new("xcodebuild")
        .args([
            "-project",
            "PamNativeApp.xcodeproj",
            "-scheme",
            "PamNativeApp",
            "-configuration",
            configuration,
            "-destination",
            "generic/platform=iOS Simulator",
            "-derivedDataPath",
        ])
        .arg(&derived)
        .args(["CODE_SIGNING_ALLOWED=NO", "build"])
        .current_dir(&workspace)
        .status()
        .map_err(|error| format!("cannot start xcodebuild: {error}"))?;
    if !status.success() {
        return Err(format!("xcodebuild exited with {status}"));
    }
    let app = derived
        .join("Build/Products")
        .join(format!("{configuration}-iphonesimulator"))
        .join(format!("{}.app", project.manifest.name));
    if !app.is_dir() {
        return Err(format!("Xcode did not produce {}", app.display()));
    }
    println!("Built {}", app.display());
    Ok(app)
}

fn booted_ios_simulator() -> Result<String, String> {
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "booted", "--json"])
        .output()
        .map_err(|error| format!("cannot query iOS simulators: {error}"))?;
    if !output.status.success() {
        return Err("cannot query booted iOS simulators".to_owned());
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid simctl response: {error}"))?;
    value["devices"]
        .as_object()
        .into_iter()
        .flat_map(|runtimes| runtimes.values())
        .filter_map(serde_json::Value::as_array)
        .flatten()
        .find_map(|device| device["udid"].as_str().map(str::to_owned))
        .ok_or_else(|| {
            "no booted iOS simulator; open Simulator or run `xcrun simctl boot <udid>`".to_owned()
        })
}

fn run_ios(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let simulator = booted_ios_simulator()?;
    let app = build_ios(project.root.clone(), false)?;
    install_and_launch_ios(&project, &simulator, &app)?;
    Ok(0)
}

fn install_and_launch_ios(project: &Project, simulator: &str, app: &Path) -> Result<(), String> {
    command_status(
        "xcrun",
        &["simctl", "install", simulator, &app.to_string_lossy()],
    )?;
    command_status(
        "xcrun",
        &[
            "simctl",
            "launch",
            simulator,
            &project.manifest.application_id,
        ],
    )?;
    println!("Started {} on {simulator}", project.manifest.application_id);
    Ok(())
}

fn dev_ios(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let simulator = booted_ios_simulator()?;
    let app = build_ios(project.root.clone(), false)?;
    install_and_launch_ios(&project, &simulator, &app)?;
    let mut fingerprint = project_fingerprint(&project.root)?;
    println!(
        "Watching {} for iOS changes. Press Ctrl+C to stop.",
        project.root.display()
    );
    loop {
        std::thread::sleep(Duration::from_millis(350));
        let next = project_fingerprint(&project.root)?;
        if next == fingerprint {
            continue;
        }
        fingerprint = next;
        println!("Change detected; rebuilding {}…", project.manifest.name);
        match build_ios(project.root.clone(), false)
            .and_then(|app| install_and_launch_ios(&project, &simulator, &app))
        {
            Ok(()) => println!("Reloaded {}.", project.manifest.name),
            Err(error) => eprintln!("iOS rebuild failed: {error}"),
        }
    }
}

fn devices_ios(project_path: PathBuf) -> Result<u8, String> {
    load_project(&project_path)?;
    command_status("xcrun", &["simctl", "list", "devices", "available"])?;
    Ok(0)
}

fn logs_ios(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    let simulator = booted_ios_simulator()?;
    let predicate = format!("process == {:?}", project.manifest.name);
    command_status(
        "xcrun",
        &[
            "simctl",
            "spawn",
            &simulator,
            "log",
            "stream",
            "--level",
            "debug",
            "--predicate",
            &predicate,
        ],
    )?;
    Ok(0)
}

fn validate_ios_signing(project: &Project) -> Result<(String, PathBuf), String> {
    let team = std::env::var("PAM_IOS_DEVELOPMENT_TEAM")
        .map_err(|_| "PAM_IOS_DEVELOPMENT_TEAM is required for an iOS release".to_owned())?;
    if team.len() != 10 || !team.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err("PAM_IOS_DEVELOPMENT_TEAM must be a 10-character Apple team ID".to_owned());
    }
    let options = std::env::var_os("PAM_IOS_EXPORT_OPTIONS_PLIST")
        .map(PathBuf::from)
        .ok_or_else(|| {
            "PAM_IOS_EXPORT_OPTIONS_PLIST must point to release export options".to_owned()
        })?;
    if !options.is_file() {
        return Err(format!(
            "iOS export options do not exist: {}",
            options.display()
        ));
    }
    let _ = project;
    Ok((team, options))
}

fn signing_status_ios(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    match validate_ios_signing(&project) {
        Ok((team, options)) => {
            println!(
                "iOS release signing is configured for team {team} with {}.",
                options.display()
            );
            Ok(0)
        }
        Err(error) => {
            println!("{error}");
            Ok(1)
        }
    }
}

fn package_ios(project_path: PathBuf) -> Result<u8, String> {
    if !cfg!(target_os = "macos") {
        return Err("iOS packaging requires macOS with Xcode".to_owned());
    }
    let project = load_project(&project_path)?;
    let (team, export_options) = validate_ios_signing(&project)?;
    let workspace = prepare_ios(&project)?;
    let archive = workspace.join("build/PamNativeApp.xcarchive");
    let export = workspace.join("build/export");
    let status = Command::new("xcodebuild")
        .args([
            "-project",
            "PamNativeApp.xcodeproj",
            "-scheme",
            "PamNativeApp",
            "-configuration",
            "Release",
            "-destination",
            "generic/platform=iOS",
            "-archivePath",
        ])
        .arg(&archive)
        .arg(format!("DEVELOPMENT_TEAM={team}"))
        .args(["-allowProvisioningUpdates", "archive"])
        .current_dir(&workspace)
        .status()
        .map_err(|error| format!("cannot archive iOS application: {error}"))?;
    if !status.success() {
        return Err(format!("iOS archive failed with {status}"));
    }
    let status = Command::new("xcodebuild")
        .args(["-exportArchive", "-archivePath"])
        .arg(&archive)
        .args(["-exportPath"])
        .arg(&export)
        .args(["-exportOptionsPlist"])
        .arg(&export_options)
        .arg("-allowProvisioningUpdates")
        .status()
        .map_err(|error| format!("cannot export iOS application: {error}"))?;
    if !status.success() {
        return Err(format!("iOS export failed with {status}"));
    }
    let ipa = files_in(&export)?
        .into_iter()
        .find(|path| path.extension() == Some(OsStr::new("ipa")))
        .ok_or_else(|| format!("Xcode did not produce an IPA in {}", export.display()))?;
    let dist = project.root.join("dist");
    fs::create_dir_all(&dist)
        .map_err(|error| format!("cannot create {}: {error}", dist.display()))?;
    let destination = dist.join(format!(
        "{}-{}-ios.ipa",
        project.manifest.name.to_ascii_lowercase().replace(' ', "-"),
        project.manifest.version_name
    ));
    fs::copy(&ipa, &destination).map_err(|error| format!("cannot copy IPA: {error}"))?;
    write_artifact_checksum(&destination)?;
    println!("Packaged {}", destination.display());
    Ok(0)
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

fn java_major_version(version_output: &str) -> Option<u32> {
    let marker = version_output.find("version \"")? + "version \"".len();
    let version = version_output.get(marker..)?.split('"').next()?;
    let first = version.split('.').next()?.parse::<u32>().ok()?;
    if first == 1 {
        version.split('.').nth(1)?.parse().ok()
    } else {
        Some(first)
    }
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
    write_ios_plugin_plan(project)?;
    write_ios_plugin_package(project, native_home)?;
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
    )?;
    add_share_targets(
        &workspace.join("app/src/main/AndroidManifest.xml"),
        &project.manifest.android.share_targets,
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
    const START: &str = "            <!-- pam-native:deep-links:start -->";
    const END: &str = "            <!-- pam-native:deep-links:end -->";

    let mut contents = fs::read_to_string(manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    if let Some(start) = contents.find(START)
        && let Some(end_offset) = contents[start..].find(END)
    {
        let end = start + end_offset + END.len();
        let trailing = usize::from(contents.as_bytes().get(end) == Some(&b'\n'));
        contents.replace_range(start..end + trailing, "");
    }
    if links.is_empty() {
        return write_atomic(manifest, contents.as_bytes());
    }
    let activity = contents
        .find("<activity\n            android:name=\".PamActivity\"")
        .ok_or_else(|| "Android manifest has no PamActivity element".to_owned())?;
    let close = contents[activity..]
        .find("        </activity>")
        .map(|offset| activity + offset)
        .ok_or_else(|| "PamActivity element is not closed".to_owned())?;
    let mut filters = String::new();
    filters.push_str(START);
    filters.push('\n');
    for link in links {
        filters.push_str("            <intent-filter");
        if link.auto_verify {
            filters.push_str(" android:autoVerify=\"true\"");
        }
        filters.push_str(">\n");
        filters
            .push_str("                <action android:name=\"android.intent.action.VIEW\" />\n");
        filters.push_str(
            "                <category android:name=\"android.intent.category.DEFAULT\" />\n",
        );
        filters.push_str(
            "                <category android:name=\"android.intent.category.BROWSABLE\" />\n",
        );
        filters.push_str("                <data android:scheme=\"");
        filters.push_str(&link.scheme);
        filters.push('"');
        if let Some(host) = &link.host {
            filters.push_str(" android:host=\"");
            filters.push_str(host);
            filters.push('"');
        }
        if let Some(path) = &link.path_prefix {
            filters.push_str(" android:pathPrefix=\"");
            filters.push_str(path);
            filters.push('"');
        }
        filters.push_str(" />\n");
        filters.push_str("            </intent-filter>\n");
    }
    filters.push_str(END);
    filters.push('\n');
    contents.insert_str(close, &filters);
    write_atomic(manifest, contents.as_bytes())
}

fn add_share_targets(manifest: &Path, mime_types: &[String]) -> Result<(), String> {
    const START: &str = "            <!-- pam-native:share-targets:start -->";
    const END: &str = "            <!-- pam-native:share-targets:end -->";

    let mut contents = fs::read_to_string(manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    let start = contents
        .find(START)
        .ok_or_else(|| "Android manifest has no share-target start marker".to_owned())?;
    let end_offset = contents[start..]
        .find(END)
        .ok_or_else(|| "Android manifest has no share-target end marker".to_owned())?;
    let end = start + end_offset + END.len();
    let mut filters = String::new();
    filters.push_str(START);
    filters.push('\n');
    for mime_type in mime_types {
        filters.push_str("            <intent-filter>\n");
        filters
            .push_str("                <action android:name=\"android.intent.action.SEND\" />\n");
        filters.push_str(
            "                <action android:name=\"android.intent.action.SEND_MULTIPLE\" />\n",
        );
        filters.push_str(
            "                <category android:name=\"android.intent.category.DEFAULT\" />\n",
        );
        filters.push_str("                <data android:mimeType=\"");
        filters.push_str(mime_type);
        filters.push_str("\" />\n");
        filters.push_str("            </intent-filter>\n");
    }
    filters.push_str(END);
    contents.replace_range(start..end, &filters);
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
            idl_sha256: plugin.idl_digest.as_deref(),
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
            ios_minimum_version: &plugin.manifest.ios.minimum_version,
            ios_source_directories: plugin
                .manifest
                .ios
                .source_dirs
                .iter()
                .map(PathBuf::as_path)
                .collect(),
            ios_resource_directories: plugin
                .manifest
                .ios
                .resource_dirs
                .iter()
                .map(PathBuf::as_path)
                .collect(),
            ios_swift_packages: plugin
                .manifest
                .ios
                .swift_packages
                .iter()
                .map(|package| package.url.as_str())
                .collect(),
            ios_frameworks: plugin
                .manifest
                .ios
                .frameworks
                .iter()
                .map(String::as_str)
                .collect(),
            ios_extensions: plugin
                .manifest
                .ios
                .extensions
                .iter()
                .map(|extension| extension.name.as_str())
                .collect(),
        })
        .collect();
    let pam_native_version = installed_pam_native_version(&project.root)?;
    let lock = PluginLock {
        version: PLUGIN_LOCK_VERSION,
        protocol: PLUGIN_PROTOCOL_VERSION,
        pam_native: &pam_native_version,
        plugins: entries,
    };
    let bytes = serde_json::to_vec_pretty(&lock)
        .map_err(|error| format!("cannot encode plugin lock: {error}"))?;
    let target = project.root.join(".pam-native/plugins.lock.json");
    let mut bytes = bytes;
    bytes.push(b'\n');
    write_atomic(&target, &bytes)
}

fn write_ios_plugin_plan(project: &Project) -> Result<(), String> {
    let plugins = project
        .plugins
        .iter()
        .filter(|plugin| has_ios_payload(&plugin.manifest))
        .map(|plugin| {
            let ios = &plugin.manifest.ios;
            let source_dirs = canonical_plugin_paths(plugin, &ios.source_dirs)?;
            let resource_dirs = canonical_plugin_paths(plugin, &ios.resource_dirs)?;
            let entitlements = ios
                .entitlements
                .as_ref()
                .map(|path| canonical_plugin_path(plugin, path))
                .transpose()?;
            let info_plist = ios
                .info_plist
                .as_ref()
                .map(|path| canonical_plugin_path(plugin, path))
                .transpose()?;
            let extensions = ios
                .extensions
                .iter()
                .map(|extension| {
                    Ok(serde_json::json!({
                        "kind": extension.kind,
                        "name": extension.name,
                        "bundleSuffix": extension.bundle_suffix,
                        "sourceDirs": canonical_plugin_paths(plugin, &extension.source_dirs)?,
                        "resourceDirs": canonical_plugin_paths(plugin, &extension.resource_dirs)?,
                        "entitlements": extension.entitlements.as_ref()
                            .map(|path| canonical_plugin_path(plugin, path)).transpose()?,
                        "infoPlist": extension.info_plist.as_ref()
                            .map(|path| canonical_plugin_path(plugin, path)).transpose()?,
                    }))
                })
                .collect::<Result<Vec<serde_json::Value>, String>>()?;
            Ok(serde_json::json!({
                "package": plugin.package,
                "packageVersion": plugin.package_version,
                "descriptorSha256": plugin.descriptor_digest,
                "minimumVersion": ios.minimum_version,
                "sourceDirs": source_dirs,
                "resourceDirs": resource_dirs,
                "swiftPackages": ios.swift_packages,
                "frameworks": ios.frameworks,
                "usageDescriptions": ios.usage_descriptions,
                "entitlements": entitlements,
                "infoPlist": info_plist,
                "extensions": extensions,
                "modules": plugin.manifest.modules.iter().filter_map(|binding| {
                    binding.ios_class.as_ref().map(|class| serde_json::json!({
                        "name": binding.name,
                        "class": class,
                    }))
                }).collect::<Vec<_>>(),
                "views": plugin.manifest.views.iter().filter_map(|binding| {
                    binding.ios_class.as_ref().map(|class| serde_json::json!({
                        "name": binding.name,
                        "class": class,
                    }))
                }).collect::<Vec<_>>(),
            }))
        })
        .collect::<Result<Vec<serde_json::Value>, String>>()?;
    let plan = serde_json::json!({
        "version": 1,
        "protocol": PLUGIN_PROTOCOL_VERSION,
        "applicationId": project.manifest.application_id,
        "plugins": plugins,
    });
    let mut bytes = serde_json::to_vec_pretty(&plan)
        .map_err(|error| format!("cannot encode iOS plugin plan: {error}"))?;
    bytes.push(b'\n');
    write_atomic(&project.root.join(".pam-native/ios/plugins.json"), &bytes)
}

fn canonical_plugin_paths(
    plugin: &NativePlugin,
    paths: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    paths
        .iter()
        .map(|path| canonical_plugin_path(plugin, path))
        .collect()
}

fn has_ios_payload(manifest: &PluginManifest) -> bool {
    manifest
        .modules
        .iter()
        .any(|binding| binding.ios_class.is_some())
        || manifest
            .views
            .iter()
            .any(|binding| binding.ios_class.is_some())
        || !manifest.ios.source_dirs.is_empty()
        || !manifest.ios.resource_dirs.is_empty()
        || !manifest.ios.swift_packages.is_empty()
        || !manifest.ios.frameworks.is_empty()
        || !manifest.ios.usage_descriptions.is_empty()
        || manifest.ios.entitlements.is_some()
        || manifest.ios.info_plist.is_some()
        || !manifest.ios.extensions.is_empty()
}

fn write_ios_plugin_package(project: &Project, native_home: &Path) -> Result<(), String> {
    let package_root = project.root.join(".pam-native/ios/PamNativePlugins");
    if package_root.exists() {
        fs::remove_dir_all(&package_root).map_err(|error| {
            format!(
                "cannot clean generated iOS plugin package {}: {error}",
                package_root.display()
            )
        })?;
    }
    fs::create_dir_all(package_root.join("Sources/PamNativePlugins"))
        .map_err(|error| format!("cannot create generated iOS package: {error}"))?;

    let plugins = project
        .plugins
        .iter()
        .filter(|plugin| has_ios_payload(&plugin.manifest))
        .collect::<Vec<_>>();
    let mut package_dependencies = BTreeMap::<String, &PluginSwiftPackage>::new();
    let mut target_names = Vec::new();
    let mut targets = String::new();
    let mut registry_imports = String::from("import PamNative\n");
    let mut module_entries = Vec::new();
    let mut view_entries = Vec::new();

    for (index, plugin) in plugins.iter().enumerate() {
        let target = swift_target_name(index, &plugin.package);
        target_names.push(target.clone());
        registry_imports.push_str(&format!("import {target}\n"));
        for binding in &plugin.manifest.modules {
            if let Some(class) = &binding.ios_class {
                module_entries.push(format!(
                    "            {}: {}(),",
                    swift_string(&binding.name),
                    swift_class_reference(class)
                ));
            }
        }
        for binding in &plugin.manifest.views {
            if let Some(class) = &binding.ios_class {
                view_entries.push(format!(
                    "            {}: {}(),",
                    swift_string(&binding.name),
                    swift_class_reference(class)
                ));
            }
        }
        let target_root = package_root.join("Sources").join(&target);
        fs::create_dir_all(&target_root)
            .map_err(|error| format!("cannot create Swift target {target}: {error}"))?;
        for source in &plugin.manifest.ios.source_dirs {
            copy_tree(&canonical_plugin_path(plugin, source)?, &target_root, &[])?;
        }
        let resources_root = target_root.join("Resources");
        for resources in &plugin.manifest.ios.resource_dirs {
            fs::create_dir_all(&resources_root)
                .map_err(|error| format!("cannot create Swift resources: {error}"))?;
            copy_tree(
                &canonical_plugin_path(plugin, resources)?,
                &resources_root,
                &[],
            )?;
        }
        if plugin.manifest.ios.source_dirs.is_empty() {
            write_atomic(
                &target_root.join("GeneratedPlugin.swift"),
                format!("public enum {target}Plugin {{}}\n").as_bytes(),
            )?;
        }

        let mut dependencies = vec![".product(name: \"PamNative\", package: \"ios\")".to_owned()];
        for package in &plugin.manifest.ios.swift_packages {
            let identity = swift_package_identity(&package.url)?;
            if let Some(existing) = package_dependencies.get(&package.url) {
                if existing.requirement.kind != package.requirement.kind
                    || existing.requirement.value != package.requirement.value
                {
                    return Err(format!(
                        "Swift package {} has conflicting requirements across plugins",
                        package.url
                    ));
                }
            } else {
                package_dependencies.insert(package.url.clone(), package);
            }
            for product in &package.products {
                dependencies.push(format!(
                    ".product(name: {}, package: {})",
                    swift_string(product),
                    swift_string(&identity)
                ));
            }
        }
        let frameworks = plugin
            .manifest
            .ios
            .frameworks
            .iter()
            .map(|framework| format!(".linkedFramework({})", swift_string(framework)))
            .collect::<Vec<_>>();
        targets.push_str(&format!(
            "        .target(\n            name: {},\n            dependencies: [{}],\n            path: {},{}{}\n        ),\n",
            swift_string(&target),
            dependencies.join(", "),
            swift_string(&format!("Sources/{target}")),
            if plugin.manifest.ios.resource_dirs.is_empty() {
                ""
            } else {
                "\n            resources: [.process(\"Resources\")],"
            },
            if frameworks.is_empty() {
                String::new()
            } else {
                format!("\n            linkerSettings: [{}],", frameworks.join(", "))
            },
        ));
    }

    let aggregate_target_dependencies = target_names
        .iter()
        .map(|target| swift_string(target))
        .collect::<Vec<_>>()
        .join(", ");
    let aggregate_dependencies = if aggregate_target_dependencies.is_empty() {
        ".product(name: \"PamNative\", package: \"ios\")".to_owned()
    } else {
        format!(".product(name: \"PamNative\", package: \"ios\"), {aggregate_target_dependencies}")
    };
    let swift_dictionary = |entries: &[String]| {
        if entries.is_empty() {
            "[:]".to_owned()
        } else {
            format!("[\n{}\n        ]", entries.join("\n"))
        }
    };
    let registry = format!(
        "{}\npublic enum PamNativePluginRegistry {{\n    public static func modules() -> [String: NativeModule] {{\n        {}\n    }}\n\n    public static func views() -> [String: NativeViewFactory] {{\n        {}\n    }}\n}}\n",
        registry_imports,
        swift_dictionary(&module_entries),
        swift_dictionary(&view_entries),
    );
    write_atomic(
        &package_root.join("Sources/PamNativePlugins/PamNativePlugins.swift"),
        registry.as_bytes(),
    )?;
    targets.push_str(&format!(
        "        .target(\n            name: \"PamNativePlugins\",\n            dependencies: [{aggregate_dependencies}],\n            path: \"Sources/PamNativePlugins\"\n        ),\n"
    ));
    let external_dependencies = package_dependencies
        .values()
        .map(|package| swift_package_declaration(package))
        .collect::<Result<Vec<_>, _>>()?;
    let pam_native_path = native_home.join("ios");
    let manifest = format!(
        "// swift-tools-version: 5.9\n\nimport PackageDescription\n\nlet package = Package(\n    name: \"PamNativePlugins\",\n    platforms: [.iOS({})],\n    products: [.library(name: \"PamNativePlugins\", targets: [\"PamNativePlugins\"])],\n    dependencies: [\n        .package(path: {}),{}\n    ],\n    targets: [\n{}    ]\n)\n",
        swift_string(&project.manifest.ios.minimum_version),
        swift_string(&pam_native_path.to_string_lossy()),
        if external_dependencies.is_empty() {
            String::new()
        } else {
            format!("\n        {},", external_dependencies.join(",\n        "))
        },
        targets,
    );
    write_atomic(&package_root.join("Package.swift"), manifest.as_bytes())
}

fn swift_target_name(index: usize, package: &str) -> String {
    let name = package
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect::<String>();
    format!("PamPlugin{index}{name}")
}

fn swift_package_identity(url: &str) -> Result<String, String> {
    url.rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".git"))
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| format!("cannot derive Swift package identity from {url}"))
}

fn swift_class_reference(value: &str) -> &str {
    value.rsplit('.').next().unwrap_or(value)
}

fn swift_package_declaration(package: &PluginSwiftPackage) -> Result<String, String> {
    let url = swift_string(&package.url);
    let value = swift_string(&package.requirement.value);
    match package.requirement.kind {
        SwiftPackageRequirementKind::Exact => Ok(format!(".package(url: {url}, exact: {value})")),
        SwiftPackageRequirementKind::From => Ok(format!(".package(url: {url}, from: {value})")),
        SwiftPackageRequirementKind::Branch => Ok(format!(".package(url: {url}, branch: {value})")),
        SwiftPackageRequirementKind::Revision => {
            Ok(format!(".package(url: {url}, revision: {value})"))
        }
        SwiftPackageRequirementKind::UpToNextMinor => Ok(format!(
            ".package(url: {url}, .upToNextMinor(from: {value}))"
        )),
    }
}

fn swift_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
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
    stage_project_at(project, &workspace.join("app/src/main/assets/pam"))
}

fn stage_project_at(project: &Project, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        fs::remove_dir_all(destination)
            .map_err(|error| format!("cannot clean {}: {error}", destination.display()))?;
    }
    fs::create_dir_all(destination)
        .map_err(|error| format!("cannot create {}: {error}", destination.display()))?;
    let mut budget = CopyBudget::default();
    copy_project_files(&project.root, &project.root, destination, &mut budget)?;
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
    let version = directory_digest(destination)?;
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
    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some(first) = components.first().copied() else {
        return false;
    };

    first.starts_with('.')
        || components
            .iter()
            .any(|component| component.starts_with('.'))
        || components
            .iter()
            .any(|component| matches!(*component, "node_modules" | "target" | "dist" | "build"))
        || components
            .iter()
            .skip(1)
            .any(|component| *component == "vendor")
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
    files.sort_by(|left, right| {
        left.strip_prefix(root)
            .unwrap_or(left)
            .cmp(right.strip_prefix(root).unwrap_or(right))
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
                "verified PHP {} Android runtime is missing for {}; build it with `runtime-builder/android/build.sh --php {} {}` (expected {})",
                runtime.release.php_version,
                abi.android(),
                project.manifest.runtime.php,
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

fn package_android(options: MobileOptions) -> Result<u8, String> {
    let signing_project = load_project(&options.project)?;
    validate_android_signing(&signing_project)?;
    let built = build(options)?;
    let workspace = built.project.root.join(".pam-native/android");
    let gradlew = workspace.join("gradlew");
    let status = Command::new(&gradlew)
        .args([":app:bundleRelease", "--stacktrace"])
        .env(
            "GRADLE_USER_HOME",
            built.project.root.join(".pam-native/gradle-home"),
        )
        .current_dir(&workspace)
        .status()
        .map_err(|error| format!("cannot create Android App Bundle: {error}"))?;
    if !status.success() {
        return Err(format!("Android App Bundle failed with {status}"));
    }
    let source = workspace.join("app/build/outputs/bundle/release/app-release.aab");
    if !source.is_file() {
        return Err(format!("Gradle did not produce {}", source.display()));
    }
    let output = built.project.root.join("dist");
    fs::create_dir_all(&output)
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;
    let name = built
        .project
        .manifest
        .name
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let destination = output.join(format!(
        "{}-{}-android.aab",
        name.trim_matches('-'),
        built.project.manifest.version_name
    ));
    fs::copy(&source, &destination)
        .map_err(|error| format!("cannot copy {}: {error}", destination.display()))?;
    write_artifact_checksum(&destination)?;
    let apk_destination = output.join(format!(
        "{}-{}-android.apk",
        name.trim_matches('-'),
        built.project.manifest.version_name
    ));
    fs::copy(&built.path, &apk_destination)
        .map_err(|error| format!("cannot copy {}: {error}", apk_destination.display()))?;
    write_artifact_checksum(&apk_destination)?;
    let metadata = serde_json::json!({
        "schemaVersion": 1,
        "platform": 1,
        "applicationId": built.project.manifest.application_id,
        "versionCode": built.project.manifest.version_code,
        "versionName": built.project.manifest.version_name,
        "runtime": built.project.manifest.runtime.php,
        "artifacts": [
            destination.file_name().unwrap_or_default().to_string_lossy(),
            apk_destination.file_name().unwrap_or_default().to_string_lossy()
        ]
    });
    let metadata_path = output.join("android-release.json");
    let mut metadata_bytes = serde_json::to_vec_pretty(&metadata)
        .map_err(|error| format!("cannot serialize Android release metadata: {error}"))?;
    metadata_bytes.push(b'\n');
    write_atomic(&metadata_path, &metadata_bytes)?;
    println!("Packaged {}", destination.display());
    println!("Packaged {}", apk_destination.display());
    println!("Metadata {}", metadata_path.display());
    Ok(0)
}

fn write_artifact_checksum(path: &Path) -> Result<(), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot hash {}: {error}", path.display()))?;
    let checksum = format!(
        "{:x}  {}\n",
        Sha256::digest(bytes),
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    let extension = format!(
        "{}.sha256",
        path.extension().unwrap_or_default().to_string_lossy()
    );
    fs::write(path.with_extension(extension), checksum)
        .map_err(|error| format!("cannot write package checksum: {error}"))
}

fn signing_status(project_path: PathBuf) -> Result<u8, String> {
    let project = load_project(&project_path)?;
    match validate_android_signing(&project) {
        Ok(()) => {
            println!(
                "Android release signing is configured for {}.",
                project.manifest.name
            );
            Ok(0)
        }
        Err(error) => {
            println!("{error}");
            Ok(1)
        }
    }
}

fn validate_android_signing(project: &Project) -> Result<(), String> {
    let required = [
        "PAM_ANDROID_KEYSTORE",
        "PAM_ANDROID_KEY_ALIAS",
        "PAM_ANDROID_KEYSTORE_PASSWORD",
        "PAM_ANDROID_KEY_PASSWORD",
    ];
    let missing = required
        .into_iter()
        .filter(|name| std::env::var_os(name).is_none_or(|value| value.is_empty()))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "Android release signing is not configured for {}. Set: {}. PAM never writes signing passwords into the project.",
            project.manifest.name,
            missing.join(", ")
        ));
    }
    let keystore = PathBuf::from(std::env::var_os("PAM_ANDROID_KEYSTORE").unwrap_or_default());
    if !keystore.is_file() {
        return Err(format!(
            "Android keystore does not exist: {}",
            keystore.display()
        ));
    }
    Ok(())
}

fn benchmark(project_path: PathBuf) -> Result<u8, String> {
    run_android_performance_suite(
        project_path,
        "dev.pam.nativeapp.benchmark.PamNativeBenchmark",
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
    let application_id = debug_application_id(&project);
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
        .join(format!("{}.pam", options.name));
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
        .join(format!("{}.pam", options.name));
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
        BuildMode::Debug => debug_application_id(project),
        BuildMode::Release => project.manifest.application_id.clone(),
    };
    command_status(
        "adb",
        &[
            "shell",
            "am",
            "start",
            "-W",
            "-n",
            &format!("{application_id}/dev.pam.nativeapp.PamActivity"),
        ],
    )?;
    println!("Started {application_id}");
    Ok(())
}

fn debug_application_id(project: &Project) -> String {
    let firebase_enabled = [
        project.root.join(".pam/google-services.json"),
        project.root.join("google-services.json"),
    ]
    .into_iter()
    .any(|path| path.is_file());

    if firebase_enabled {
        project.manifest.application_id.clone()
    } else {
        format!("{}.debug", project.manifest.application_id)
    }
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
            fingerprint = next;
            match refresh_dev_bundle(project, native_home, workspace, &mut version, &mut bundle) {
                Ok(()) => println!("Reload ready · {}", &version[..16]),
                Err(error) => eprintln!("pam mobile dev: {error}"),
            }
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
    fn detects_supported_java_major_versions() {
        assert_eq!(java_major_version("openjdk version \"17.0.12\""), Some(17));
        assert_eq!(java_major_version("java version \"1.8.0_402\""), Some(8));
        assert_eq!(java_major_version("openjdk version \"21\""), Some(21));
        assert_eq!(java_major_version("not java"), None);
    }

    #[test]
    fn generator_names_are_safe_and_human_readable() {
        assert!(valid_pascal_name("Checkout"));
        assert!(valid_pascal_name("HTTPClient2"));
        assert!(!valid_pascal_name("checkout"));
        assert!(!valid_pascal_name("../Checkout"));
        assert_eq!(kebab_case("CheckoutForm"), "checkout-form");
        assert_eq!(kebab_case("HTTPClient"), "http-client");
        assert!(valid_mime_type("image/*"));
        assert!(valid_mime_type("application/vnd.example+json"));
        assert!(!valid_mime_type("image"));
        assert!(!valid_mime_type("image/<script>"));
    }

    #[test]
    fn ios_property_lists_merge_capabilities_without_losing_values() {
        let mut target = serde_json::json!({
            "com.apple.security.application-groups": ["group.app.pam.demo.pam-native"],
            "existing": true
        });
        merge_plist_value(
            &mut target,
            serde_json::json!({
                "com.apple.security.application-groups": [
                    "group.app.pam.demo.pam-native",
                    "group.app.pam.demo.shared"
                ],
                "com.apple.developer.healthkit": true
            }),
            "test plugin",
        )
        .expect("compatible capabilities");
        assert_eq!(
            target["com.apple.security.application-groups"],
            serde_json::json!(["group.app.pam.demo.pam-native", "group.app.pam.demo.shared"])
        );
        assert_eq!(target["com.apple.developer.healthkit"], true);
        assert!(
            merge_plist_value(
                &mut target,
                serde_json::json!({"existing": false}),
                "conflicting plugin"
            )
            .is_err()
        );

        let replaced = replace_plist_application_id(
            serde_json::json!({
                "groups": ["group.$(PAM_NATIVE_APPLICATION_ID).pam-native"]
            }),
            "app.pam.demo",
        );
        assert_eq!(replaced["groups"][0], "group.app.pam.demo.pam-native");
    }

    #[test]
    fn app_intents_use_extensionkit_metadata() {
        let mut plist = serde_json::json!({
            "NSExtension": {
                "NSExtensionPointIdentifier": "com.apple.intents-service"
            }
        });
        normalize_ios_extension_plist(&mut plist, IosExtensionKind::Intents)
            .expect("App Intents plist");
        assert!(plist.get("NSExtension").is_none());
        assert_eq!(
            plist["EXAppExtensionAttributes"]["EXExtensionPointIdentifier"],
            "com.apple.appintents-extension"
        );
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
        assert!(root.join("src/Screens/Orders.pam").is_file());
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
        assert!(root.join("src/Components/MetricCard.pam").is_file());

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

        let project = load_project(&root).expect("project");
        assert_eq!(debug_application_id(&project), "app.pam.generated.debug");
        fs::create_dir_all(root.join(".pam")).expect("pam state");
        fs::write(root.join(".pam/google-services.json"), "{}").expect("firebase config");
        assert_eq!(debug_application_id(&project), "app.pam.generated");

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
        let composer = root.join("vendor/composer");
        let source = package.join("android/src/main/kotlin");
        let ios_source = package.join("ios/Sources");
        let ios_resources = package.join("ios/Resources");
        let ios_extension = package.join("ios/ShareExtension");
        fs::create_dir_all(&source).expect("plugin source");
        fs::create_dir_all(&ios_source).expect("iOS plugin source");
        fs::create_dir_all(&ios_resources).expect("iOS plugin resources");
        fs::create_dir_all(&ios_extension).expect("iOS extension source");
        fs::write(
            package.join("ios/App.entitlements"),
            "<?xml version=\"1.0\"?><plist/>",
        )
        .expect("iOS entitlements");
        fs::write(
            package.join("ios/ShareInfo.plist"),
            "<?xml version=\"1.0\"?><plist/>",
        )
        .expect("extension Info.plist");
        fs::write(
            package.join("pam-native.idl.json"),
            r#"{"version":1,"namespace":"Community.Example"}"#,
        )
        .expect("plugin IDL");
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
                "ios": {"minimumVersion": "18.0"},
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
                    "version": "0.6.0"
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
                    "minimum": "0.6.0",
                    "maximumExclusive": "0.7.0"
                },
                "php": {"provider": "Community\\Example\\PluginProvider"},
                "android": {
                    "namespace": "community.example.plugin",
                    "minSdk": 26,
                    "sourceDirs": ["android/src/main/kotlin"],
                    "permissions": ["android.permission.CAMERA"],
                    "dependencies": ["androidx.core:core-ktx:1.17.0"]
                },
                "ios": {
                    "minimumVersion": "15.0",
                    "sourceDirs": ["ios/Sources"],
                    "resourceDirs": ["ios/Resources"],
                    "swiftPackages": [{
                        "url": "https://github.com/apple/swift-collections.git",
                        "requirement": {"kind": 1, "value": "1.1.0"},
                        "products": ["Collections"]
                    }],
                    "frameworks": ["AuthenticationServices"],
                    "usageDescriptions": {
                        "NSFaceIDUsageDescription": "Authenticate your account."
                    },
                    "entitlements": "ios/App.entitlements",
                    "extensions": [{
                        "kind": 1,
                        "name": "CommunityShare",
                        "bundleSuffix": "share",
                        "sourceDirs": ["ios/ShareExtension"],
                        "infoPlist": "ios/ShareInfo.plist"
                    }]
                },
                "idl": "pam-native.idl.json",
                "modules": [{
                    "name": "community.echo",
                    "class": "community.example.EchoModule",
                    "iosClass": "CommunityExample.EchoModule"
                }],
                "views": [{
                    "name": "community.badge",
                    "class": "community.example.BadgeFactory",
                    "iosClass": "CommunityExample.BadgeFactory"
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
        write_ios_plugin_plan(&project).expect("iOS plugin plan");
        let native_home = root.join("pam-native-home");
        fs::create_dir_all(native_home.join("ios")).expect("native iOS package");
        fs::write(native_home.join("ios/Package.swift"), "// fixture\n")
            .expect("native Package.swift");
        write_ios_plugin_package(&project, &native_home).expect("generated Swift package");

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
        assert!(lock.contains("\"iosMinimumVersion\": \"15.0\""));
        assert!(lock.contains("\"iosSourceDirectories\": ["));
        assert!(lock.contains("\"ios/Resources\""));
        assert!(lock.contains("\"idlSha256\":"));
        assert!(lock.contains("https://github.com/apple/swift-collections.git"));
        assert!(lock.contains("AuthenticationServices"));
        let ios_plan = fs::read_to_string(root.join(".pam-native/ios/plugins.json"))
            .expect("generated iOS plugin plan");
        assert!(ios_plan.contains("CommunityShare"));
        assert!(ios_plan.contains("NSFaceIDUsageDescription"));
        assert!(ios_plan.contains("swift-collections.git"));
        let swift_package =
            fs::read_to_string(root.join(".pam-native/ios/PamNativePlugins/Package.swift"))
                .expect("generated Swift Package.swift");
        assert!(swift_package.contains("platforms: [.iOS(\"18.0\")]"));
        assert!(swift_package.contains(".linkedFramework(\"AuthenticationServices\")"));
        assert!(swift_package.contains(
            ".package(url: \"https://github.com/apple/swift-collections.git\", exact: \"1.1.0\")"
        ));
        assert!(
            swift_package
                .contains(".product(name: \"Collections\", package: \"swift-collections\")")
        );
        let swift_registry = fs::read_to_string(root.join(
            ".pam-native/ios/PamNativePlugins/Sources/PamNativePlugins/PamNativePlugins.swift",
        ))
        .expect("generated Swift plugin registry");
        assert!(swift_registry.contains("\"community.echo\": EchoModule()"));
        assert!(swift_registry.contains("\"community.badge\": BadgeFactory()"));

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

    #[test]
    fn ios_plugin_metadata_rejects_unsafe_versions_and_class_names() {
        assert!(valid_ios_version("15.0"));
        assert!(valid_ios_version("26.1"));
        assert!(!valid_ios_version("15"));
        assert!(!valid_ios_version("15.0.1"));
        assert!(!valid_ios_version("v15.0"));

        assert!(valid_swift_class_name("PamFirebase.FirebaseModule"));
        assert!(valid_swift_class_name("FirebaseModule"));
        assert!(!valid_swift_class_name("PamFirebase/FirebaseModule"));
        assert!(!valid_swift_class_name("PamFirebase..FirebaseModule"));
        assert!(!valid_swift_class_name("1FirebaseModule"));
    }
}
