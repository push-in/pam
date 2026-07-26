use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::terminal::Terminal;
use crate::wasi;

const PROTOCOL_VERSION: u8 = 1;
const METHOD_KIND_UNARY: u8 = 1;
const MESSAGE_KIND_REQUEST: u8 = 1;
const MESSAGE_KIND_SUCCESS: u8 = 2;
const MESSAGE_KIND_FAILURE: u8 = 3;
const DEFAULT_CONTRACTS: &str = "contracts.mobile.json";
const DEFAULT_OUTPUT: &str = "generated/rpc";
const DEFAULT_FUEL: u64 = 100_000_000;
const DEFAULT_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MESSAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_VALIDATION_DEPTH: usize = 64;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u8,
    service: String,
    version: String,
    methods: Vec<Method>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Method {
    kind: u8,
    name: String,
    input: String,
    output: String,
    timeout_ms: u64,
    idempotent: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
    kind: u8,
    name: String,
    php_class: String,
    #[serde(default)]
    properties: Vec<Property>,
    #[serde(default)]
    cases: Vec<ContractCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Property {
    name: String,
    kind: u8,
    #[serde(rename = "type")]
    type_name: String,
    nullable: bool,
    format: Option<String>,
    item_type: Option<String>,
    minimum: Option<serde_json::Number>,
    maximum: Option<serde_json::Number>,
}

#[derive(Deserialize)]
struct ContractCase {
    name: String,
    value: i64,
}

struct Catalog {
    contracts: BTreeMap<String, Contract>,
    php_names: BTreeMap<String, String>,
}

struct CommonOptions {
    manifest: PathBuf,
    contracts: PathBuf,
}

pub fn run(arguments: Vec<OsString>) -> Result<u8, String> {
    let mut arguments = arguments.into_iter();
    let action = required_os(&mut arguments, "rpc requires an action")?;
    match action.as_str() {
        "validate" => {
            let options = parse_common(arguments)?;
            let (manifest, catalog) = load(&options)?;
            print_validated(&manifest, &catalog, &options);
            Ok(0)
        }
        "generate" => generate(arguments),
        "wasi" => invoke_wasi(arguments),
        _ => Err(format!(
            "unknown RPC action {action:?}; expected validate, generate, or wasi"
        )),
    }
}

fn parse_common(mut arguments: impl Iterator<Item = OsString>) -> Result<CommonOptions, String> {
    let manifest = PathBuf::from(required_os(
        &mut arguments,
        "rpc action requires a pam.rpc.json manifest",
    )?);
    let mut contracts = None;
    while let Some(argument) = arguments.next() {
        match os_string(argument)?.as_str() {
            "--contracts" => {
                contracts = Some(PathBuf::from(required_os(
                    &mut arguments,
                    "--contracts requires a JSON file",
                )?));
            }
            option => return Err(format!("unknown RPC option: {option}")),
        }
    }
    let contracts = contracts.unwrap_or_else(|| {
        manifest
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(DEFAULT_CONTRACTS)
    });
    Ok(CommonOptions {
        manifest,
        contracts,
    })
}

fn generate(mut arguments: impl Iterator<Item = OsString>) -> Result<u8, String> {
    let manifest = PathBuf::from(required_os(
        &mut arguments,
        "rpc generate requires a pam.rpc.json manifest",
    )?);
    let mut contracts = None;
    let mut output = PathBuf::from(DEFAULT_OUTPUT);
    while let Some(argument) = arguments.next() {
        match os_string(argument)?.as_str() {
            "--contracts" => {
                contracts = Some(PathBuf::from(required_os(
                    &mut arguments,
                    "--contracts requires a JSON file",
                )?));
            }
            "--output" => {
                output = PathBuf::from(required_os(
                    &mut arguments,
                    "--output requires a directory",
                )?);
            }
            option => return Err(format!("unknown rpc generate option: {option}")),
        }
    }
    let options = CommonOptions {
        contracts: contracts.unwrap_or_else(|| {
            manifest
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(DEFAULT_CONTRACTS)
        }),
        manifest,
    };
    let (manifest, catalog) = load(&options)?;
    if output.exists()
        && output
            .read_dir()
            .map_err(|error| format!("cannot inspect {}: {error}", output.display()))?
            .next()
            .is_some()
    {
        return Err(format!(
            "refusing to overwrite non-empty RPC output {}",
            output.display()
        ));
    }
    fs::create_dir_all(&output)
        .map_err(|error| format!("cannot create {}: {error}", output.display()))?;
    write_json(
        &output.join("pam-rpc.manifest.json"),
        &serde_json::to_value(&manifest)
            .map_err(|error| format!("cannot serialize RPC manifest: {error}"))?,
    )?;
    fs::write(
        output.join("pam-rpc.ts"),
        typescript_sdk(&manifest, &catalog),
    )
    .map_err(|error| format!("cannot write TypeScript RPC SDK: {error}"))?;
    fs::write(output.join("pam_rpc.py"), python_sdk(&manifest, &catalog))
        .map_err(|error| format!("cannot write Python RPC SDK: {error}"))?;
    fs::write(output.join("pam_rpc.rs"), rust_sdk(&manifest, &catalog))
        .map_err(|error| format!("cannot write Rust RPC SDK: {error}"))?;
    fs::write(output.join("RPC.md"), rpc_markdown(&manifest))
        .map_err(|error| format!("cannot write RPC documentation: {error}"))?;

    let ui = Terminal::stdout();
    println!("{}", ui.success("● TYPED RPC SDK GENERATED"));
    println!("{}", ui.rule());
    println!(
        "  {} {}@{}",
        ui.muted(format!("{:<12}", "Service")),
        manifest.service,
        manifest.version
    );
    println!(
        "  {} {} methods",
        ui.muted(format!("{:<12}", "Catalog")),
        manifest.methods.len()
    );
    println!(
        "  {} {}",
        ui.muted(format!("{:<12}", "Output")),
        output.display()
    );
    Ok(0)
}

fn invoke_wasi(mut arguments: impl Iterator<Item = OsString>) -> Result<u8, String> {
    let manifest_path = PathBuf::from(required_os(
        &mut arguments,
        "rpc wasi requires a pam.rpc.json manifest",
    )?);
    let module = PathBuf::from(required_os(
        &mut arguments,
        "rpc wasi requires a WebAssembly module",
    )?);
    let method_name = required_os(&mut arguments, "rpc wasi requires a method")?;
    let request_path = PathBuf::from(required_os(
        &mut arguments,
        "rpc wasi requires a request JSON file",
    )?);
    let mut contracts = None;
    let mut fuel = DEFAULT_FUEL;
    let mut memory_bytes = DEFAULT_MEMORY_BYTES;
    let mut request_id = None;
    while let Some(argument) = arguments.next() {
        match os_string(argument)?.as_str() {
            "--contracts" => {
                contracts = Some(PathBuf::from(required_os(
                    &mut arguments,
                    "--contracts requires a JSON file",
                )?));
            }
            "--fuel" => {
                fuel = parse_number(
                    &required_os(&mut arguments, "--fuel requires an integer")?,
                    "--fuel",
                    1_u64,
                    u64::MAX,
                )?;
            }
            "--memory-bytes" => {
                memory_bytes = parse_number(
                    &required_os(&mut arguments, "--memory-bytes requires an integer")?,
                    "--memory-bytes",
                    64 * 1024,
                    2 * 1024 * 1024 * 1024_usize,
                )?;
            }
            "--request-id" => {
                let value = required_os(&mut arguments, "--request-id requires a value")?;
                if !valid_request_id(&value) {
                    return Err(
                        "--request-id must contain 1-128 safe ASCII identifier characters"
                            .to_owned(),
                    );
                }
                request_id = Some(value);
            }
            option => return Err(format!("unknown rpc wasi option: {option}")),
        }
    }
    let options = CommonOptions {
        contracts: contracts.unwrap_or_else(|| {
            manifest_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(DEFAULT_CONTRACTS)
        }),
        manifest: manifest_path,
    };
    let (manifest, catalog) = load(&options)?;
    let method = manifest
        .methods
        .iter()
        .find(|method| method.name == method_name)
        .ok_or_else(|| format!("unknown RPC method {method_name:?}"))?;
    let request = read_json(&request_path, MAX_MESSAGE_BYTES, "RPC request")?;
    validate_contract(&request, &method.input, &catalog, "$request")?;

    let request_id = request_id.unwrap_or_else(|| {
        format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    });
    let envelope = serde_json::json!({
        "protocolVersion": PROTOCOL_VERSION,
        "id": request_id,
        "kind": MESSAGE_KIND_REQUEST,
        "service": manifest.service,
        "method": method.name,
        "payload": request,
    });
    let mut input = serde_json::to_vec(&envelope)
        .map_err(|error| format!("cannot encode RPC request: {error}"))?;
    input.push(b'\n');
    let output_limit = usize::try_from(MAX_MESSAGE_BYTES)
        .unwrap_or(64 * 1024 * 1024)
        .min(memory_bytes);
    let execution = wasi::execute_rpc(
        &module,
        input,
        fuel,
        memory_bytes,
        output_limit,
        Duration::from_millis(method.timeout_ms),
    )?;
    if !execution.stderr.is_empty() {
        std::io::stderr()
            .write_all(&execution.stderr)
            .map_err(|error| format!("cannot write RPC guest stderr: {error}"))?;
    }
    if execution.status != 0 {
        return Err(format!(
            "WASI RPC guest exited with status {}",
            execution.status
        ));
    }
    let response: Value = serde_json::from_slice(trim_ascii(&execution.stdout))
        .map_err(|error| format!("invalid RPC response JSON: {error}"))?;
    let object = response
        .as_object()
        .ok_or("RPC response must be an object")?;
    require_integer(object.get("protocolVersion"), "RPC protocolVersion", 1)?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .ok_or("RPC response id must be a string")?;
    if id != request_id {
        return Err("RPC response id does not match its request".to_owned());
    }
    let kind = object
        .get("kind")
        .and_then(Value::as_u64)
        .ok_or("RPC response kind must be an integer")?;
    match kind {
        value if value == u64::from(MESSAGE_KIND_SUCCESS) => {
            require_exact_fields(
                object.keys().map(String::as_str),
                &["protocolVersion", "id", "kind", "result"],
                "successful RPC response",
            )?;
            let result = object
                .get("result")
                .ok_or("successful RPC response requires result")?;
            validate_contract(result, &method.output, &catalog, "$response.result")?;
            println!(
                "{}",
                serde_json::to_string_pretty(result)
                    .map_err(|error| format!("cannot render RPC result: {error}"))?
            );
            Ok(0)
        }
        value if value == u64::from(MESSAGE_KIND_FAILURE) => {
            require_exact_fields(
                object.keys().map(String::as_str),
                &["protocolVersion", "id", "kind", "error"],
                "failed RPC response",
            )?;
            let error = object
                .get("error")
                .and_then(Value::as_object)
                .ok_or("failed RPC response requires an error object")?;
            require_exact_fields(
                error.keys().map(String::as_str),
                &["code", "message"],
                "RPC error",
            )?;
            let code = error
                .get("code")
                .and_then(Value::as_u64)
                .filter(|code| *code > 0)
                .ok_or("RPC error code must be a positive integer")?;
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .ok_or("RPC error message must be a string")?;
            Err(format!("RPC guest error {code}: {message}"))
        }
        _ => Err(format!(
            "RPC response kind must be {MESSAGE_KIND_SUCCESS} or {MESSAGE_KIND_FAILURE}"
        )),
    }
}

fn load(options: &CommonOptions) -> Result<(Manifest, Catalog), String> {
    let manifest: Manifest = serde_json::from_value(read_json(
        &options.manifest,
        MAX_DOCUMENT_BYTES,
        "RPC manifest",
    )?)
    .map_err(|error| format!("invalid RPC manifest: {error}"))?;
    let contracts: Vec<Contract> = serde_json::from_value(read_json(
        &options.contracts,
        MAX_DOCUMENT_BYTES,
        "typed contract catalog",
    )?)
    .map_err(|error| format!("invalid typed contract catalog: {error}"))?;
    let mut catalog = Catalog {
        contracts: BTreeMap::new(),
        php_names: BTreeMap::new(),
    };
    for contract in contracts {
        if !is_type_identifier(&contract.name) {
            return Err(format!("invalid RPC contract name {:?}", contract.name));
        }
        if catalog
            .php_names
            .insert(contract.php_class.clone(), contract.name.clone())
            .is_some()
            || catalog
                .contracts
                .insert(contract.name.clone(), contract)
                .is_some()
        {
            return Err("duplicate RPC contract name or PHP class".to_owned());
        }
    }
    validate_catalog(&catalog)?;
    validate_manifest(&manifest, &catalog)?;
    Ok((manifest, catalog))
}

fn validate_catalog(catalog: &Catalog) -> Result<(), String> {
    for contract in catalog.contracts.values() {
        match contract.kind {
            1 if contract.cases.is_empty() => {}
            2 if contract.properties.is_empty() && !contract.cases.is_empty() => {
                let values = contract
                    .cases
                    .iter()
                    .map(|case| case.value)
                    .collect::<Vec<_>>();
                let expected = (1..=i64::try_from(values.len()).unwrap_or(0)).collect::<Vec<_>>();
                if values != expected {
                    return Err(format!(
                        "RPC enum {} values must be sequential integers from 1",
                        contract.name
                    ));
                }
                let mut names = BTreeSet::new();
                if contract
                    .cases
                    .iter()
                    .any(|case| !is_type_identifier(&case.name) || !names.insert(&case.name))
                {
                    return Err(format!(
                        "RPC enum {} has invalid or duplicate cases",
                        contract.name
                    ));
                }
            }
            kind => {
                return Err(format!(
                    "invalid RPC contract kind {kind} for {}",
                    contract.name
                ));
            }
        }
        let mut fields = BTreeSet::new();
        for property in &contract.properties {
            if !is_property_identifier(&property.name) || !fields.insert(&property.name) {
                return Err(format!(
                    "invalid or duplicate RPC field {}.{}",
                    contract.name, property.name
                ));
            }
            if !(1..=7).contains(&property.kind) {
                return Err(format!(
                    "unsupported RPC field kind {} on {}.{}",
                    property.kind, contract.name, property.name
                ));
            }
            let expected_scalar = match property.kind {
                1 => Some("string"),
                2 => Some("int"),
                3 => Some("float"),
                4 => Some("bool"),
                6 => Some("array"),
                _ => None,
            };
            if expected_scalar.is_some_and(|expected| property.type_name != expected) {
                return Err(format!(
                    "RPC field {}.{} kind does not match type {}",
                    contract.name, property.name, property.type_name
                ));
            }
            if property
                .minimum
                .as_ref()
                .zip(property.maximum.as_ref())
                .is_some_and(|(minimum, maximum)| {
                    minimum
                        .as_f64()
                        .zip(maximum.as_f64())
                        .is_some_and(|(minimum, maximum)| minimum > maximum)
                })
            {
                return Err(format!(
                    "RPC field {}.{} minimum exceeds maximum",
                    contract.name, property.name
                ));
            }
            if property.kind == 6 && property.item_type.as_deref().is_none_or(str::is_empty) {
                return Err(format!(
                    "RPC array field {}.{} requires itemType",
                    contract.name, property.name
                ));
            }
            if property.kind == 6
                && let Some(item_type) = property.item_type.as_deref()
                && !matches!(item_type, "string" | "int" | "float" | "bool")
            {
                let target = catalog
                    .php_names
                    .get(item_type)
                    .map(String::as_str)
                    .unwrap_or(item_type);
                if !catalog.contracts.contains_key(target) {
                    return Err(format!(
                        "RPC array field {}.{} references unknown item contract {}",
                        contract.name, property.name, item_type
                    ));
                }
            }
            if matches!(property.kind, 5 | 7) {
                let target = catalog
                    .php_names
                    .get(&property.type_name)
                    .map(String::as_str)
                    .unwrap_or(&property.type_name);
                if !catalog.contracts.contains_key(target) {
                    return Err(format!(
                        "RPC field {}.{} references unknown contract {}",
                        contract.name, property.name, property.type_name
                    ));
                }
                let target_kind = catalog
                    .contracts
                    .get(target)
                    .map(|contract| contract.kind)
                    .unwrap_or_default();
                if (property.kind == 5 && target_kind != 1)
                    || (property.kind == 7 && target_kind != 2)
                {
                    return Err(format!(
                        "RPC field {}.{} kind does not match referenced contract {}",
                        contract.name, property.name, property.type_name
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &Manifest, catalog: &Catalog) -> Result<(), String> {
    if manifest.schema_version != 1 {
        return Err(format!(
            "unsupported RPC manifest schema {}",
            manifest.schema_version
        ));
    }
    if !is_type_identifier(&manifest.service) {
        return Err("RPC service must be a PascalCase identifier".to_owned());
    }
    if !valid_version(&manifest.version) {
        return Err("RPC version must use numeric MAJOR.MINOR.PATCH form".to_owned());
    }
    if manifest.methods.is_empty() {
        return Err("RPC manifest requires at least one method".to_owned());
    }
    let mut names = BTreeSet::new();
    for method in &manifest.methods {
        if method.kind != METHOD_KIND_UNARY {
            return Err(format!(
                "RPC method {} kind must currently be {} (unary)",
                method.name, METHOD_KIND_UNARY
            ));
        }
        if !is_method_identifier(&method.name) || !names.insert(method.name.as_str()) {
            return Err(format!(
                "RPC method names must be unique lowerCamelCase identifiers: {:?}",
                method.name
            ));
        }
        if !catalog.contracts.contains_key(&method.input) {
            return Err(format!(
                "RPC method {} references unknown input {}",
                method.name, method.input
            ));
        }
        if !catalog.contracts.contains_key(&method.output) {
            return Err(format!(
                "RPC method {} references unknown output {}",
                method.name, method.output
            ));
        }
        if !(1..=3_600_000).contains(&method.timeout_ms) {
            return Err(format!(
                "RPC method {} timeoutMs must be between 1 and 3600000",
                method.name
            ));
        }
    }
    Ok(())
}

fn validate_contract(
    value: &Value,
    contract_name: &str,
    catalog: &Catalog,
    path: &str,
) -> Result<(), String> {
    validate_contract_at(value, contract_name, catalog, path, 0)
}

fn validate_contract_at(
    value: &Value,
    contract_name: &str,
    catalog: &Catalog,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_VALIDATION_DEPTH {
        return Err(format!(
            "{path} exceeds the maximum RPC nesting depth of {MAX_VALIDATION_DEPTH}"
        ));
    }
    let contract = catalog
        .contracts
        .get(contract_name)
        .ok_or_else(|| format!("{path} references unknown contract {contract_name}"))?;
    match contract.kind {
        1 => {
            let object = value
                .as_object()
                .ok_or_else(|| format!("{path} must be an object ({contract_name})"))?;
            let fields = contract
                .properties
                .iter()
                .map(|property| property.name.as_str())
                .collect::<BTreeSet<_>>();
            for key in object.keys() {
                if !fields.contains(key.as_str()) {
                    return Err(format!("{path}.{key} is not declared by {contract_name}"));
                }
            }
            for property in &contract.properties {
                let property_path = format!("{path}.{}", property.name);
                match object.get(&property.name) {
                    None if property.nullable => continue,
                    None => return Err(format!("{property_path} is required")),
                    Some(Value::Null) if property.nullable => continue,
                    Some(Value::Null) => return Err(format!("{property_path} cannot be null")),
                    Some(value) => {
                        validate_property(value, property, catalog, &property_path, depth)?
                    }
                }
            }
            Ok(())
        }
        2 => {
            let number = value
                .as_i64()
                .ok_or_else(|| format!("{path} must be an integer enum ({contract_name})"))?;
            if contract.cases.iter().any(|case| case.value == number) {
                Ok(())
            } else {
                Err(format!(
                    "{path} is not a declared integer value of {contract_name}"
                ))
            }
        }
        kind => Err(format!(
            "{path} uses unsupported contract kind {kind} for {contract_name}"
        )),
    }
}

fn validate_property(
    value: &Value,
    property: &Property,
    catalog: &Catalog,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    match property.kind {
        1 => {
            let text = value
                .as_str()
                .ok_or_else(|| format!("{path} must be a string"))?;
            if property.format.as_deref() == Some("uuid") && !looks_like_uuid(text) {
                return Err(format!("{path} must be a UUID"));
            }
        }
        2 => {
            value
                .as_i64()
                .ok_or_else(|| format!("{path} must be an integer"))?;
            validate_numeric(value, property, path)?;
        }
        3 => {
            value
                .as_f64()
                .ok_or_else(|| format!("{path} must be a number"))?;
            validate_numeric(value, property, path)?;
        }
        4 => {
            value
                .as_bool()
                .ok_or_else(|| format!("{path} must be a boolean"))?;
        }
        5 | 7 => {
            let target = catalog
                .php_names
                .get(&property.type_name)
                .map(String::as_str)
                .unwrap_or(&property.type_name);
            validate_contract_at(value, target, catalog, path, depth + 1)?;
        }
        6 => {
            let items = value
                .as_array()
                .ok_or_else(|| format!("{path} must be an array"))?;
            let item_type = property
                .item_type
                .as_deref()
                .ok_or_else(|| format!("{path} has no declared item type"))?;
            for (index, item) in items.iter().enumerate() {
                validate_item(item, item_type, catalog, &format!("{path}[{index}]"), depth)?;
            }
        }
        kind => return Err(format!("{path} has unsupported property kind {kind}")),
    }
    Ok(())
}

fn validate_item(
    value: &Value,
    item_type: &str,
    catalog: &Catalog,
    path: &str,
    depth: usize,
) -> Result<(), String> {
    match item_type {
        "string" if value.is_string() => Ok(()),
        "int" if value.as_i64().is_some() => Ok(()),
        "float" if value.as_f64().is_some() => Ok(()),
        "bool" if value.is_boolean() => Ok(()),
        "string" | "int" | "float" | "bool" => {
            Err(format!("{path} does not match item type {item_type}"))
        }
        _ => {
            let target = catalog
                .php_names
                .get(item_type)
                .map(String::as_str)
                .unwrap_or(item_type);
            validate_contract_at(value, target, catalog, path, depth + 1)
        }
    }
}

fn validate_numeric(value: &Value, property: &Property, path: &str) -> Result<(), String> {
    let number = value
        .as_f64()
        .ok_or_else(|| format!("{path} must be numeric"))?;
    if property
        .minimum
        .as_ref()
        .and_then(serde_json::Number::as_f64)
        .is_some_and(|minimum| number < minimum)
    {
        return Err(format!("{path} is below its minimum"));
    }
    if property
        .maximum
        .as_ref()
        .and_then(serde_json::Number::as_f64)
        .is_some_and(|maximum| number > maximum)
    {
        return Err(format!("{path} exceeds its maximum"));
    }
    Ok(())
}

fn print_validated(manifest: &Manifest, catalog: &Catalog, options: &CommonOptions) {
    let ui = Terminal::stdout();
    println!("{}", ui.success("● TYPED RPC VALID"));
    println!("{}", ui.rule());
    println!(
        "  {} {}@{}",
        ui.muted(format!("{:<12}", "Service")),
        manifest.service,
        manifest.version
    );
    println!(
        "  {} {} methods / {} contracts",
        ui.muted(format!("{:<12}", "Catalog")),
        manifest.methods.len(),
        catalog.contracts.len()
    );
    println!(
        "  {} {}",
        ui.muted(format!("{:<12}", "Manifest")),
        options.manifest.display()
    );
    println!(
        "  {} {}",
        ui.muted(format!("{:<12}", "Contracts")),
        options.contracts.display()
    );
}

fn typescript_sdk(manifest: &Manifest, catalog: &Catalog) -> String {
    let mut output = String::from(concat!(
        "// Generated by `pam rpc generate`. Do not edit.\n",
        "export const PAM_RPC_PROTOCOL_VERSION = 1 as const;\n",
        "export type PamRpcError = { readonly code: number; readonly message: string };\n",
        "export type PamRpcRequest = { readonly protocolVersion: 1; readonly id: string; readonly kind: 1; readonly service: string; readonly method: string; readonly payload: unknown };\n",
        "export type PamRpcResponse = { readonly protocolVersion: 1; readonly id: string; readonly kind: 2; readonly result: unknown } | { readonly protocolVersion: 1; readonly id: string; readonly kind: 3; readonly error: PamRpcError };\n",
        "export type PamRpcTransport = (request: PamRpcRequest, signal?: AbortSignal) => Promise<unknown>;\n",
        "export type PamRpcIdFactory = () => string;\n\n",
        "function defaultPamRpcId(): string {\n",
        "  if (typeof globalThis.crypto?.randomUUID !== \"function\") throw new Error(\"PAM RPC requires an id factory when crypto.randomUUID is unavailable\");\n",
        "  return globalThis.crypto.randomUUID();\n",
        "}\n\n",
        "function pamRpcResponse(value: unknown): PamRpcResponse {\n",
        "  if (typeof value !== \"object\" || value === null || Array.isArray(value)) throw new Error(\"PAM RPC response must be an object\");\n",
        "  const response = value as Record<string, unknown>;\n",
        "  if (response.protocolVersion !== 1 || typeof response.id !== \"string\") throw new Error(\"PAM RPC response metadata is invalid\");\n",
        "  if (response.kind === 2 && \"result\" in response) return value as PamRpcResponse;\n",
        "  if (response.kind === 3 && typeof response.error === \"object\" && response.error !== null) {\n",
        "    const error = response.error as Record<string, unknown>;\n",
        "    if (typeof error.code === \"number\" && Number.isInteger(error.code) && error.code > 0 && typeof error.message === \"string\") return value as PamRpcResponse;\n",
        "  }\n",
        "  throw new Error(\"PAM RPC response envelope is invalid\");\n",
        "}\n\n",
    ));
    append_typescript_contracts(&mut output, catalog);
    output.push_str(&format!(
        "export class {}Client {{\n  public constructor(private readonly transport: PamRpcTransport, private readonly createId: PamRpcIdFactory = defaultPamRpcId) {{}}\n",
        manifest.service
    ));
    for method in &manifest.methods {
        output.push_str(&format!(
            concat!(
                "  public async {}(payload: {}, options: {{ readonly signal?: AbortSignal; readonly id?: string }} = {{}}): Promise<{}> {{\n",
                "    const id = options.id ?? this.createId();\n",
                "    const response = pamRpcResponse(await this.transport({{ protocolVersion: 1, id, kind: 1, service: {:?}, method: {:?}, payload }}, options.signal));\n",
                "    if (response.id !== id) throw new Error(\"PAM RPC response id mismatch\");\n",
                "    if (response.kind === 3) throw new Error(`PAM RPC ${{response.error.code}}: ${{response.error.message}}`);\n",
                "    return response.result as {};\n",
                "  }}\n",
            ),
            method.name,
            method.input,
            method.output,
            manifest.service,
            method.name,
            method.output
        ));
    }
    output.push_str("}\n");
    output
}

fn append_typescript_contracts(output: &mut String, catalog: &Catalog) {
    for contract in catalog.contracts.values() {
        if contract.kind == 2 {
            output.push_str(&format!("export enum {} {{\n", contract.name));
            for case in &contract.cases {
                output.push_str(&format!("  {} = {},\n", case.name, case.value));
            }
            output.push_str("}\n\n");
            continue;
        }
        output.push_str(&format!("export interface {} {{\n", contract.name));
        for property in &contract.properties {
            output.push_str(&format!(
                "  readonly {}{}: {}{};\n",
                property.name,
                if property.nullable { "?" } else { "" },
                language_type(property, catalog, Target::TypeScript),
                if property.nullable { " | null" } else { "" }
            ));
        }
        output.push_str("}\n\n");
    }
}

fn python_sdk(manifest: &Manifest, catalog: &Catalog) -> String {
    let mut output = String::from(concat!(
        "# Generated by `pam rpc generate`. Do not edit.\n",
        "from __future__ import annotations\n",
        "from enum import IntEnum\n",
        "from typing import Any, Awaitable, Callable, Mapping, NotRequired, TypedDict, cast\n",
        "from uuid import uuid4\n\n",
        "PamRpcTransport = Callable[[dict[str, Any]], Awaitable[object]]\n\n",
        "def _pam_rpc_response(value: object, request_id: str) -> Mapping[str, Any]:\n",
        "    if not isinstance(value, Mapping):\n",
        "        raise RuntimeError(\"PAM RPC response must be an object\")\n",
        "    if value.get(\"protocolVersion\") != 1 or value.get(\"id\") != request_id:\n",
        "        raise RuntimeError(\"PAM RPC response metadata mismatch\")\n",
        "    kind = value.get(\"kind\")\n",
        "    if kind == 2 and \"result\" in value:\n",
        "        return value\n",
        "    if kind == 3 and isinstance(value.get(\"error\"), Mapping):\n",
        "        error = cast(Mapping[str, Any], value[\"error\"])\n",
        "        code = error.get(\"code\")\n",
        "        if isinstance(code, int) and not isinstance(code, bool) and code > 0 and isinstance(error.get(\"message\"), str):\n",
        "            return value\n",
        "    raise RuntimeError(\"PAM RPC response envelope is invalid\")\n\n",
    ));
    for contract in catalog.contracts.values() {
        if contract.kind == 2 {
            output.push_str(&format!("class {}(IntEnum):\n", contract.name));
            for case in &contract.cases {
                output.push_str(&format!("    {} = {}\n", case.name, case.value));
            }
            output.push('\n');
            continue;
        }
        output.push_str(&format!("class {}(TypedDict):\n", contract.name));
        if contract.properties.is_empty() {
            output.push_str("    pass\n\n");
            continue;
        }
        for property in &contract.properties {
            let value_type = language_type(property, catalog, Target::Python);
            output.push_str(&format!(
                "    {}: {}{}{}\n",
                property.name,
                if property.nullable {
                    "NotRequired["
                } else {
                    ""
                },
                value_type,
                if property.nullable { " | None]" } else { "" }
            ));
        }
        output.push('\n');
    }
    output.push_str(&format!(
        "class {}Client:\n    def __init__(self, transport: PamRpcTransport) -> None:\n        self._transport = transport\n\n",
        manifest.service
    ));
    for method in &manifest.methods {
        output.push_str(&format!(
            concat!(
                "    async def {}(self, payload: {}, request_id: str | None = None) -> {}:\n",
                "        rpc_id = request_id or str(uuid4())\n",
                "        response = _pam_rpc_response(await self._transport({{\"protocolVersion\": 1, \"id\": rpc_id, \"kind\": 1, \"service\": {:?}, \"method\": {:?}, \"payload\": payload}}), rpc_id)\n",
                "        if response.get(\"kind\") == 3:\n",
                "            error = cast(Mapping[str, Any], response[\"error\"])\n",
                "            raise RuntimeError(f\"PAM RPC {{error.get('code')}}: {{error.get('message')}}\")\n",
                "        return cast({}, response[\"result\"])\n\n",
            ),
            to_snake_case(&method.name),
            method.input,
            method.output,
            manifest.service,
            method.name,
            method.output,
        ));
    }
    output
}

fn rust_sdk(manifest: &Manifest, catalog: &Catalog) -> String {
    let mut output = String::from(
        "// Generated by `pam rpc generate`. Do not edit.\n\
use serde::{Deserialize, Serialize};\n\
use serde_json::{json, Value};\n\
\n\
pub const PAM_RPC_PROTOCOL_VERSION: u8 = 1;\n\n",
    );
    for contract in catalog.contracts.values() {
        if contract.kind == 2 {
            output.push_str(&format!(
                "#[derive(Clone, Copy, Debug, Deserialize, Serialize)]\n#[repr(i64)]\n#[serde(try_from = \"i64\", into = \"i64\")]\npub enum {} {{\n",
                contract.name
            ));
            for case in &contract.cases {
                output.push_str(&format!("    {} = {},\n", case.name, case.value));
            }
            output.push_str("}\n\n");
            output.push_str(&format!(
                "impl TryFrom<i64> for {} {{\n    type Error = String;\n\n    fn try_from(value: i64) -> Result<Self, Self::Error> {{\n        match value {{\n",
                contract.name
            ));
            for case in &contract.cases {
                output.push_str(&format!(
                    "            {} => Ok(Self::{}),\n",
                    case.value, case.name
                ));
            }
            output.push_str(&format!(
                "            _ => Err(format!(\"invalid {} value {{value}}\")),\n        }}\n    }}\n}}\n\n",
                contract.name
            ));
            output.push_str(&format!(
                "impl From<{}> for i64 {{\n    fn from(value: {}) -> Self {{\n        value as i64\n    }}\n}}\n\n",
                contract.name, contract.name
            ));
            continue;
        }
        output.push_str(&format!(
            "#[derive(Clone, Debug, Deserialize, Serialize)]\n#[serde(deny_unknown_fields)]\npub struct {} {{\n",
            contract.name
        ));
        for property in &contract.properties {
            output.push_str(&format!(
                "    pub {}: {}{},\n",
                property.name,
                if property.nullable { "Option<" } else { "" },
                if property.nullable {
                    format!("{}>", language_type(property, catalog, Target::Rust))
                } else {
                    language_type(property, catalog, Target::Rust)
                }
            ));
        }
        output.push_str("}\n\n");
    }
    output.push_str(concat!(
        "#[derive(Clone, Debug, Serialize)]\n",
        "#[serde(rename_all = \"camelCase\")]\n",
        "pub struct PamRpcRequest<T> {\n",
        "    pub protocol_version: u8,\n",
        "    pub id: String,\n",
        "    pub kind: u8,\n",
        "    pub service: &'static str,\n",
        "    pub method: &'static str,\n",
        "    pub payload: T,\n",
        "}\n\n",
    ));
    output.push_str(&format!(
        "pub const PAM_RPC_SERVICE: &str = {:?};\npub const PAM_RPC_SERVICE_VERSION: &str = {:?};\n",
        manifest.service, manifest.version
    ));
    output.push_str(concat!(
        "\npub trait PamRpcTransport {\n",
        "    fn call(&self, request: Value) -> Result<Value, String>;\n",
        "}\n\n",
    ));
    output.push_str(&format!(
        "pub struct {}Client<T> {{\n    transport: T,\n}}\n\nimpl<T: PamRpcTransport> {}Client<T> {{\n    pub fn new(transport: T) -> Self {{\n        Self {{ transport }}\n    }}\n",
        manifest.service, manifest.service
    ));
    for method in &manifest.methods {
        output.push_str(&format!(
            concat!(
                "\n    pub fn {}(\n",
                "        &self,\n",
                "        payload: {},\n",
                "        request_id: impl Into<String>,\n",
                "    ) -> Result<{}, String> {{\n",
                "        let id = request_id.into();\n",
                "        if id.is_empty() {{\n",
                "            return Err(\"PAM RPC request id cannot be empty\".to_owned());\n",
                "        }}\n",
                "        let response = self.transport.call(json!({{\n",
                "            \"protocolVersion\": 1,\n",
                "            \"id\": id.clone(),\n",
                "            \"kind\": 1,\n",
                "            \"service\": {:?},\n",
                "            \"method\": {:?},\n",
                "            \"payload\": payload,\n",
                "        }}))?;\n",
                "        let object = response\n",
                "            .as_object()\n",
                "            .ok_or(\"PAM RPC response must be an object\")?;\n",
                "        if object.get(\"protocolVersion\").and_then(Value::as_u64) != Some(1)\n",
                "            || object.get(\"id\").and_then(Value::as_str) != Some(id.as_str())\n",
                "        {{\n",
                "            return Err(\"PAM RPC response metadata mismatch\".to_owned());\n",
                "        }}\n",
                "        match object.get(\"kind\").and_then(Value::as_u64) {{\n",
                "            Some(2) => serde_json::from_value(\n",
                "                object\n",
                "                    .get(\"result\")\n",
                "                    .cloned()\n",
                "                    .ok_or(\"PAM RPC success requires result\")?,\n",
                "            )\n",
                "            .map_err(|error| format!(\"invalid PAM RPC result: {{error}}\")),\n",
                "            Some(3) => {{\n",
                "                let error = object\n",
                "                    .get(\"error\")\n",
                "                    .and_then(Value::as_object)\n",
                "                    .ok_or(\"PAM RPC failure requires error\")?;\n",
                "                let code = error\n",
                "                    .get(\"code\")\n",
                "                    .and_then(Value::as_u64)\n",
                "                    .filter(|code| *code > 0)\n",
                "                    .ok_or(\"PAM RPC error code must be positive\")?;\n",
                "                let message = error\n",
                "                    .get(\"message\")\n",
                "                    .and_then(Value::as_str)\n",
                "                    .ok_or(\"PAM RPC error message must be a string\")?;\n",
                "                Err(format!(\"PAM RPC {{code}}: {{message}}\"))\n",
                "            }}\n",
                "            _ => Err(\"PAM RPC response kind is invalid\".to_owned()),\n",
                "        }}\n",
                "    }}\n",
            ),
            to_snake_case(&method.name),
            method.input,
            method.output,
            manifest.service,
            method.name,
        ));
    }
    output.push_str("}\n");
    output
}

#[derive(Clone, Copy)]
enum Target {
    TypeScript,
    Python,
    Rust,
}

fn language_type(property: &Property, catalog: &Catalog, target: Target) -> String {
    let reference = || {
        catalog
            .php_names
            .get(&property.type_name)
            .cloned()
            .unwrap_or_else(|| property.type_name.clone())
    };
    match (target, property.kind) {
        (Target::TypeScript, 1) => "string".to_owned(),
        (Target::TypeScript, 2 | 3) => "number".to_owned(),
        (Target::TypeScript, 4) => "boolean".to_owned(),
        (Target::Python, 1) => "str".to_owned(),
        (Target::Python, 2) => "int".to_owned(),
        (Target::Python, 3) => "float".to_owned(),
        (Target::Python, 4) => "bool".to_owned(),
        (Target::Rust, 1) => "String".to_owned(),
        (Target::Rust, 2) => "i64".to_owned(),
        (Target::Rust, 3) => "f64".to_owned(),
        (Target::Rust, 4) => "bool".to_owned(),
        (Target::Rust, 5) => format!("Box<{}>", reference()),
        (_, 5 | 7) => reference(),
        (target, 6) => {
            let item = property.item_type.as_deref().unwrap_or("mixed");
            let item = match (target, item) {
                (Target::TypeScript, "string") => "string".to_owned(),
                (Target::TypeScript, "int" | "float") => "number".to_owned(),
                (Target::TypeScript, "bool") => "boolean".to_owned(),
                (Target::Python, "string") => "str".to_owned(),
                (Target::Python, "int") => "int".to_owned(),
                (Target::Python, "float") => "float".to_owned(),
                (Target::Python, "bool") => "bool".to_owned(),
                (Target::Rust, "string") => "String".to_owned(),
                (Target::Rust, "int") => "i64".to_owned(),
                (Target::Rust, "float") => "f64".to_owned(),
                (Target::Rust, "bool") => "bool".to_owned(),
                (_, other) => catalog
                    .php_names
                    .get(other)
                    .cloned()
                    .unwrap_or_else(|| other.to_owned()),
            };
            match target {
                Target::TypeScript => format!("ReadonlyArray<{item}>"),
                Target::Python => format!("list[{item}]"),
                Target::Rust => format!("Vec<{item}>"),
            }
        }
        (Target::TypeScript, _) => "unknown".to_owned(),
        (Target::Python, _) => "Any".to_owned(),
        (Target::Rust, _) => "serde_json::Value".to_owned(),
    }
}

fn rpc_markdown(manifest: &Manifest) -> String {
    let mut output = format!(
        "# {} RPC\n\nVersion `{}` · PAM RPC protocol `1`\n\n",
        manifest.service, manifest.version
    );
    output.push_str("| Method | Kind | Input | Output | Deadline | Idempotent |\n");
    output.push_str("| --- | ---: | --- | --- | ---: | --- |\n");
    for method in &manifest.methods {
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | {} ms | {} |\n",
            method.name,
            method.kind,
            method.input,
            method.output,
            method.timeout_ms,
            if method.idempotent { "yes" } else { "no" }
        ));
    }
    output.push_str(
        "\nKinds are sequential integers: request `1`, success `2`, failure `3`. \
Unknown fields and contract violations are rejected before guest execution and \
again on the response boundary.\n",
    );
    output
}

fn read_json(path: &Path, limit: u64, label: &str) -> Result<Value, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect {label} {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{label} is not a file: {}", path.display()));
    }
    if metadata.len() > limit {
        return Err(format!("{label} exceeds the {limit} byte limit"));
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read {label} {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid {label} JSON: {error}"))
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("cannot serialize {}: {error}", path.display()))?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn required_os(
    arguments: &mut impl Iterator<Item = OsString>,
    message: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| message.to_owned())
        .and_then(os_string)
}

fn os_string(value: OsString) -> Result<String, String> {
    value
        .into_string()
        .map_err(|_| "RPC arguments must be valid UTF-8".to_owned())
}

fn parse_number<T>(value: &str, option: &str, minimum: T, maximum: T) -> Result<T, String>
where
    T: std::str::FromStr + Ord + Copy + std::fmt::Display,
{
    let parsed = value
        .parse::<T>()
        .map_err(|_| format!("{option} requires an integer"))?;
    if parsed < minimum || parsed > maximum {
        return Err(format!("{option} must be between {minimum} and {maximum}"));
    }
    Ok(parsed)
}

fn require_integer(value: Option<&Value>, label: &str, expected: u64) -> Result<(), String> {
    if value.and_then(Value::as_u64) == Some(expected) {
        Ok(())
    } else {
        Err(format!("{label} must be {expected}"))
    }
}

fn require_exact_fields<'a>(
    actual: impl Iterator<Item = &'a str>,
    expected: &[&str],
    label: &str,
) -> Result<(), String> {
    let actual = actual.collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{label} contains missing or unknown fields"))
    }
}

fn trim_ascii(value: &[u8]) -> &[u8] {
    let start = value
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index + 1);
    &value[start..end]
}

fn is_type_identifier(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_uppercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && value.len() <= 128
}

fn is_method_identifier(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && value.len() <= 128
}

fn is_property_identifier(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && value.len() <= 128
}

fn valid_version(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn to_snake_case(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_ascii_uppercase() {
            if !output.is_empty() && !output.ends_with('_') {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

fn looks_like_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}
