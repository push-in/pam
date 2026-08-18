#!/usr/bin/env python3
"""Validate the authoritative PAM Native ecosystem publication matrix."""

from __future__ import annotations

import json
import re
import sys
from enum import IntEnum
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "config" / "ecosystem-packages.json"
REPOSITORY = re.compile(r"^pam-native-[a-z0-9]+(?:-[a-z0-9]+)*$")
COMPOSER_NAME = re.compile(r"^pushinbr/[a-z0-9]+(?:-[a-z0-9]+)*$")


class EcosystemRole(IntEnum):
    CORE_DISTRIBUTION = 1
    DEVICE_CAPABILITY = 2
    PRODUCT_INTEGRATION = 3
    TOOLING = 4


def catalog() -> list[dict[str, object]]:
    document = json.loads(CATALOG.read_text(encoding="utf-8"))
    if document.get("schemaVersion") != 1 or not isinstance(document.get("packages"), list):
        raise ValueError("ecosystem catalog must use schemaVersion 1 and a packages list")
    packages = document["packages"]
    repositories: set[str] = set()
    composer_names: set[str] = set()
    for package in packages:
        if not isinstance(package, dict):
            raise ValueError("ecosystem package entries must be objects")
        repository = package.get("repository")
        composer_name = package.get("composerName")
        role_code = package.get("roleCode")
        if not isinstance(repository, str) or REPOSITORY.fullmatch(repository) is None:
            raise ValueError("ecosystem repository name is invalid")
        if not isinstance(composer_name, str) or COMPOSER_NAME.fullmatch(composer_name) is None:
            raise ValueError(f"{repository} has an invalid Composer name")
        if role_code not in {role.value for role in EcosystemRole}:
            raise ValueError(f"{repository} roleCode must be a sequential integer from 1 to 4")
        if not isinstance(package.get("requiresNative"), bool) or not isinstance(
            package.get("testRequired"), bool
        ):
            raise ValueError(f"{repository} compatibility flags must be booleans")
        if repository in repositories or composer_name in composer_names:
            raise ValueError(f"duplicate ecosystem identity: {repository}")
        repositories.add(repository)
        composer_names.add(composer_name)
    return sorted(packages, key=lambda package: str(package["repository"]))


def verify_inventory(packages: list[dict[str, object]]) -> None:
    remote = sorted(line.strip() for line in sys.stdin if line.strip())
    expected = sorted(str(package["repository"]) for package in packages)
    if remote != expected:
        missing = sorted(set(remote) - set(expected))
        stale = sorted(set(expected) - set(remote))
        raise ValueError(f"ecosystem inventory mismatch; missing={missing}, stale={stale}")


def verify_checkout(packages: list[dict[str, object]], directory: Path, repository: str) -> None:
    package = next(
        (package for package in packages if package["repository"] == repository),
        None,
    )
    if package is None:
        raise ValueError(f"unknown ecosystem repository: {repository}")
    manifest_path = directory / "composer.json"
    if not manifest_path.is_file():
        raise ValueError(f"{repository} does not publish composer.json")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("name") != package["composerName"]:
        raise ValueError(f"{repository} Composer identity does not match the catalog")
    requirements = manifest.get("require")
    if not isinstance(requirements, dict) or requirements.get("php") != "^8.4":
        raise ValueError(f"{repository} must declare PHP ^8.4 compatibility")
    native = requirements.get("pushinbr/pam-native")
    if package["requiresNative"] and not isinstance(native, str):
        raise ValueError(f"{repository} must constrain pushinbr/pam-native")
    if not package["requiresNative"] and native is not None:
        raise ValueError(f"{repository} unexpectedly depends on itself/the Native aggregate")
    scripts = manifest.get("scripts", {})
    if not isinstance(scripts, dict):
        raise ValueError(f"{repository} Composer scripts must be an object")
    if package["testRequired"] and not isinstance(scripts.get("test"), str):
        raise ValueError(f"{repository} must expose composer test")
    publication_workflow = directory / ".github" / "workflows" / "publication-compatibility.yml"
    release_workflow = directory / ".github" / "workflows" / "release.yml"
    workflow_path = publication_workflow if publication_workflow.is_file() else release_workflow
    if not workflow_path.is_file():
        raise ValueError(f"{repository} must certify every publication tag")
    workflow = workflow_path.read_text(encoding="utf-8")
    reusable_gate = "push-in/pam/.github/workflows/ecosystem-compatibility.yml@main"
    if "tags:" not in workflow or reusable_gate not in workflow:
        raise ValueError(f"{repository} publication workflow must call the ecosystem gate")
    if workflow_path == release_workflow and "needs: ecosystem-compatibility" not in workflow:
        raise ValueError(f"{repository} publisher must wait for ecosystem compatibility")


def main() -> int:
    try:
        packages = catalog()
        command = sys.argv[1] if len(sys.argv) > 1 else ""
        if command == "matrix":
            print(json.dumps(packages, separators=(",", ":")))
        elif command == "inventory":
            verify_inventory(packages)
            print(f"Verified exact public ecosystem inventory: {len(packages)} repositories.")
        elif command == "verify" and len(sys.argv) == 4:
            verify_checkout(packages, Path(sys.argv[2]), sys.argv[3])
            print(f"Verified ecosystem contract: {sys.argv[3]}.")
        else:
            raise ValueError(
                "usage: ecosystem-compatibility.py matrix | inventory | verify <directory> <repository>"
            )
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"ecosystem compatibility error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
