//! `pam add target <name>` and `pam remove target <name>`: the `targets` list of
//! a PAM Native app (`pam-native.json`). Only that entry is rewritten, so the
//! rest of the manifest keeps its order and formatting.

use std::fs;
use std::path::Path;

const TARGETS: [&str; 5] = ["android", "ios", "android-tv", "tizen", "webos"];
/// Manifests without `targets` build for phones and tablets.
const DEFAULT_TARGETS: [&str; 2] = ["android", "ios"];

pub fn parse(name: &str) -> Result<&'static str, String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "android" => Ok("android"),
        "ios" => Ok("ios"),
        "android-tv" | "androidtv" | "tv" => Ok("android-tv"),
        "tizen" | "samsung" => Ok("tizen"),
        "webos" | "lg" => Ok("webos"),
        other => Err(format!(
            "unknown target `{other}`; use one of: {}",
            TARGETS.join(", ")
        )),
    }
}

pub fn add(root: &Path, name: &str) -> Result<u8, String> {
    change(root, name, true)
}

pub fn remove(root: &Path, name: &str) -> Result<u8, String> {
    change(root, name, false)
}

fn change(root: &Path, name: &str, add: bool) -> Result<u8, String> {
    let target = parse(name)?;
    let path = root.join("pam-native.json");
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let (updated, changed) = apply(&source, target, add)?;
    if !changed {
        println!(
            "{target} is {} a target of this app",
            if add { "already" } else { "not" }
        );
        return Ok(0);
    }
    fs::write(&path, updated)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    println!(
        "{} target {target} in pam-native.json",
        if add { "Added" } else { "Removed" }
    );
    if add && target == "android-tv" {
        println!(
            "`pam mobile run` now installs the TV launcher entry; see docs/tv.md in PAM Native"
        );
    }
    if add && matches!(target, "tizen" | "webos") {
        println!(
            "`pam mobile {target}:build` compiles the app to static HTML/JS (after `pam composer require --dev pushinbr/pam-native-web`); see docs/tv.md in PAM Native"
        );
    }
    Ok(0)
}

/// Returns the new manifest text and whether the target list changed.
pub fn apply(source: &str, target: &str, add: bool) -> Result<(String, bool), String> {
    let manifest: serde_json::Value = serde_json::from_str(source)
        .map_err(|error| format!("pam-native.json is not valid JSON: {error}"))?;
    let object = manifest
        .as_object()
        .ok_or("pam-native.json must be a JSON object")?;
    let mut targets: Vec<String> = match object.get("targets") {
        None => DEFAULT_TARGETS
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "pam-native.json `targets` must list strings".to_owned())
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("pam-native.json `targets` must be a list".to_owned()),
    };
    let present = targets.iter().any(|value| value == target);
    if add == present {
        return Ok((source.to_owned(), false));
    }
    if add {
        targets.push(target.to_owned());
    } else {
        targets.retain(|value| value != target);
        if targets.is_empty() {
            return Err("an app needs at least one target".to_owned());
        }
    }
    let list = format!(
        "[{}]",
        targets
            .iter()
            .map(|value| serde_json::to_string(value).expect("string"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let updated = match object.contains_key("targets") {
        true => replace_targets(source, &list)?,
        false => insert_targets(source, &list)?,
    };
    // The edit is textual: make sure it still parses to the expected list.
    let check: serde_json::Value = serde_json::from_str(&updated)
        .map_err(|error| format!("cannot update pam-native.json `targets`: {error}"))?;
    if check["targets"] != serde_json::json!(targets) {
        return Err("cannot update pam-native.json `targets`; edit it by hand".to_owned());
    }
    Ok((updated, true))
}

fn replace_targets(source: &str, list: &str) -> Result<String, String> {
    let key = source
        .find("\"targets\"")
        .ok_or("cannot find `targets` in pam-native.json")?;
    let open = key + source[key..].find('[').ok_or("`targets` must be a list")?;
    let close = open + source[open..].find(']').ok_or("`targets` must be a list")?;
    Ok(format!(
        "{}{}{}",
        &source[..open],
        list,
        &source[close + 1..]
    ))
}

fn insert_targets(source: &str, list: &str) -> Result<String, String> {
    let open = source
        .find('{')
        .ok_or("pam-native.json must be a JSON object")?;
    let rest = &source[open + 1..];
    let first = rest
        .find(|character: char| !character.is_whitespace())
        .ok_or("pam-native.json is empty")?;
    if rest[first..].starts_with('}') {
        return Ok(format!(
            "{}{{\"targets\": {list}{}",
            &source[..open],
            &rest[first..]
        ));
    }
    let leading = &rest[..first];
    let indent = leading.rsplit('\n').next().unwrap_or("");
    let separator = if leading.contains('\n') {
        format!("\n{indent}")
    } else {
        " ".to_owned()
    };
    Ok(format!(
        "{}{{{separator}\"targets\": {list},{separator}{}",
        &source[..open],
        &rest[first..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_tv_to_a_manifest_without_targets() {
        let source = "{\n    \"version\": 1,\n    \"name\": \"App\"\n}\n";
        let (updated, changed) = apply(source, "android-tv", true).unwrap();
        assert!(changed);
        assert_eq!(
            updated,
            "{\n    \"targets\": [\"android\", \"ios\", \"android-tv\"],\n    \"version\": 1,\n    \"name\": \"App\"\n}\n"
        );
    }

    #[test]
    fn replaces_only_the_target_list() {
        let source = "{\n    \"name\": \"App\",\n    \"targets\": [\n        \"android\",\n        \"android-tv\"\n    ],\n    \"z\": [1]\n}\n";
        let (updated, changed) = apply(source, "android-tv", false).unwrap();
        assert!(changed);
        assert_eq!(
            updated,
            "{\n    \"name\": \"App\",\n    \"targets\": [\"android\"],\n    \"z\": [1]\n}\n"
        );
        assert!(!apply(&updated, "android-tv", false).unwrap().1);
        assert!(apply(&updated, "android", false).is_err());
    }

    #[test]
    fn accepts_tv_aliases_only() {
        assert_eq!(parse("TV").unwrap(), "android-tv");
        assert_eq!(parse("Samsung").unwrap(), "tizen");
        assert_eq!(parse("webOS").unwrap(), "webos");
        assert!(parse("roku").is_err());
    }
}
