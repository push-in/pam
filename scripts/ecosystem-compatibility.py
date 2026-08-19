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


class CompatibilityResult(IntEnum):
    PASSED = 1


class DependencyGraph(IntEnum):
    LATEST = 1
    LOWEST = 2


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


def aggregate_evidence(
    packages: list[dict[str, object]], directory: Path, output: Path
) -> None:
    expected = {
        (str(package["repository"]), php): package
        for package in packages
        for php in ("8.4", "8.5")
    }
    results: dict[tuple[str, str], dict[str, object]] = {}
    commits: set[str] = set()
    native_candidate_commits: set[str] = set()
    for path in sorted(directory.glob("*.json")):
        result = json.loads(path.read_text(encoding="utf-8"))
        repository = result.get("repository")
        php = result.get("phpSeries")
        key = (repository, php)
        package = expected.get(key)
        if package is None:
            raise ValueError(f"unexpected ecosystem evidence identity: {key}")
        if key in results:
            raise ValueError(f"duplicate ecosystem evidence identity: {key}")
        if (
            result.get("schemaVersion") != 1
            or result.get("resultCode") != CompatibilityResult.PASSED
            or result.get("composerName") != package["composerName"]
            or result.get("roleCode") != package["roleCode"]
            or result.get("graphCodes")
            != [DependencyGraph.LATEST, DependencyGraph.LOWEST]
        ):
            raise ValueError(f"invalid ecosystem evidence contract: {path.name}")
        commit = result.get("pamCommit")
        if not isinstance(commit, str) or re.fullmatch(r"[0-9a-f]{40}", commit) is None:
            raise ValueError(f"invalid PAM commit in ecosystem evidence: {path.name}")
        package_commit = result.get("packageCommit")
        if not isinstance(package_commit, str) or re.fullmatch(
            r"[0-9a-f]{40}", package_commit
        ) is None:
            raise ValueError(f"invalid package commit in ecosystem evidence: {path.name}")
        native_candidate_commit = result.get("nativeCandidateCommit")
        if native_candidate_commit is not None:
            if not isinstance(native_candidate_commit, str) or re.fullmatch(
                r"[0-9a-f]{40}", native_candidate_commit
            ) is None:
                raise ValueError(
                    f"invalid Native candidate commit in ecosystem evidence: {path.name}"
                )
            if not package["requiresNative"]:
                raise ValueError(
                    f"unexpected Native candidate for independent package: {path.name}"
                )
            native_candidate_commits.add(native_candidate_commit)
        graphs = result.get("graphs")
        if not isinstance(graphs, list) or len(graphs) != 2:
            raise ValueError(f"invalid graph evidence contract: {path.name}")
        for graph, code in zip(graphs, DependencyGraph, strict=True):
            if not isinstance(graph, dict) or graph.get("graphCode") != code:
                raise ValueError(f"invalid graph code in ecosystem evidence: {path.name}")
            lock_sha256 = graph.get("lockSha256")
            if not isinstance(lock_sha256, str) or re.fullmatch(
                r"[0-9a-f]{64}", lock_sha256
            ) is None:
                raise ValueError(f"invalid lock hash in ecosystem evidence: {path.name}")
        commits.add(commit)
        results[key] = result
    missing = sorted(set(expected) - set(results))
    if missing:
        raise ValueError(f"missing ecosystem evidence combinations: {missing}")
    if len(commits) != 1:
        raise ValueError(f"ecosystem evidence must certify one PAM commit: {sorted(commits)}")
    if native_candidate_commits:
        if len(native_candidate_commits) != 1:
            raise ValueError(
                "ecosystem evidence must certify one Native candidate commit: "
                f"{sorted(native_candidate_commits)}"
            )
        missing_candidate = sorted(
            key
            for key, package in expected.items()
            if package["requiresNative"]
            and results[key].get("nativeCandidateCommit") is None
        )
        if missing_candidate:
            raise ValueError(
                f"missing Native candidate evidence combinations: {missing_candidate}"
            )
    report = {
        "schemaVersion": 1,
        "resultCode": CompatibilityResult.PASSED,
        "pamCommit": next(iter(commits)),
        "nativeCandidateCommit": (
            next(iter(native_candidate_commits)) if native_candidate_commits else None
        ),
        "packageCount": len(packages),
        "combinationCount": len(results),
        "graphExecutionCount": len(results) * 2,
        "phpSeries": ["8.4", "8.5"],
        "graphCodes": [DependencyGraph.LATEST, DependencyGraph.LOWEST],
        "results": [results[key] for key in sorted(results)],
    }
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


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
        elif command == "evidence" and len(sys.argv) == 4:
            aggregate_evidence(packages, Path(sys.argv[2]), Path(sys.argv[3]))
            print(f"Aggregated {len(packages) * 2} ecosystem evidence combinations.")
        else:
            raise ValueError(
                "usage: ecosystem-compatibility.py matrix | inventory | verify <directory> <repository> | evidence <directory> <output>"
            )
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"ecosystem compatibility error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
