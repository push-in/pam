#!/usr/bin/env python3
"""Validate the authoritative PAM Native ecosystem publication matrix."""

from __future__ import annotations

import hashlib
import json
import os
import re
import stat
import sys
import tempfile
from enum import IntEnum
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "config" / "ecosystem-packages.json"
REPOSITORY = re.compile(r"^(?:pam-native-[a-z0-9]+(?:-[a-z0-9]+)*|pam-native-ui)$")
COMPOSER_NAME = re.compile(r"^pushinbr/[a-z0-9]+(?:-[a-z0-9]+)*$")
LOCAL_CHECKOUT_ALIASES = {"pam-native-php": "pam-native/packages/native"}
MAX_LOCAL_EVIDENCE_BYTES = 1_048_576


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


class LocalContractResult(IntEnum):
    PASSED = 1


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


def publication_workflow(directory: Path) -> tuple[Path, bool]:
    publication = directory / ".github" / "workflows" / "publication-compatibility.yml"
    release = directory / ".github" / "workflows" / "release.yml"
    return (publication, False) if publication.is_file() else (release, True)


def read_bounded_regular(path: Path, label: str) -> bytes:
    descriptor = -1
    try:
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError(f"{label} must be a regular file")
        if metadata.st_size <= 0 or metadata.st_size > MAX_LOCAL_EVIDENCE_BYTES:
            raise ValueError(f"{label} exceeds the bounded document size")
        with os.fdopen(descriptor, "rb") as handle:
            descriptor = -1
            return handle.read()
    except OSError as error:
        raise ValueError(f"{label} must be a regular file") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def active_workflow_lines(workflow: str) -> list[str]:
    return [
        line.strip()
        for line in workflow.splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]


def workflow_block(workflow: str, key: str, indentation: int) -> list[str]:
    lines = workflow.splitlines()
    marker = " " * indentation + key
    try:
        start = lines.index(marker) + 1
    except ValueError:
        return []
    end = len(lines)
    for index in range(start, len(lines)):
        line = lines[index]
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        current_indentation = len(line) - len(line.lstrip(" "))
        if current_indentation <= indentation:
            end = index
            break
    return lines[start:end]


def workflow_job(workflow: str, job: str) -> list[str]:
    jobs = workflow_block(workflow, "jobs:", 0)
    return active_workflow_lines("\n".join(workflow_block("\n".join(jobs), f"{job}:", 2)))


def workflow_has_tag_trigger(workflow: str) -> bool:
    on = workflow_block(workflow, "on:", 0)
    push = workflow_block("\n".join(on), "push:", 2)
    return any(
        line.startswith("    tags:") and not line.lstrip().startswith("#")
        for line in push
    )


def workflow_has_reusable_gate(workflow: str, reusable_gate: str) -> bool:
    jobs = workflow_block(workflow, "jobs:", 0)
    expected = f"    uses: {reusable_gate}"
    return any(line == expected for line in jobs)


def verify_checkout(
    packages: list[dict[str, object]],
    directory: Path,
    repository: str,
    workflow_directory: Path | None = None,
) -> tuple[bytes, bytes]:
    package = next(
        (package for package in packages if package["repository"] == repository),
        None,
    )
    if package is None:
        raise ValueError(f"unknown ecosystem repository: {repository}")
    manifest_path = directory / "composer.json"
    if not manifest_path.exists():
        raise ValueError(f"{repository} does not publish composer.json")
    manifest_bytes = read_bounded_regular(manifest_path, f"{repository} composer.json")
    manifest = json.loads(manifest_bytes)
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
    workflow_path, is_release = publication_workflow(workflow_directory or directory)
    if not workflow_path.exists():
        raise ValueError(f"{repository} must certify every publication tag")
    workflow_bytes = read_bounded_regular(workflow_path, f"{repository} publication workflow")
    workflow = workflow_bytes.decode("utf-8")
    reusable_gate = "push-in/pam/.github/workflows/ecosystem-compatibility.yml@main"
    if not workflow_has_tag_trigger(workflow) or not workflow_has_reusable_gate(workflow, reusable_gate):
        raise ValueError(f"{repository} publication workflow must call the ecosystem gate")
    if is_release:
        publisher = workflow_job(workflow, "publish")
        waits_for_gate = "needs: ecosystem-compatibility" in publisher or "- ecosystem-compatibility" in publisher
        if not waits_for_gate:
            raise ValueError(f"{repository} publisher must wait for ecosystem compatibility")
    return manifest_bytes, workflow_bytes


def local_checkout(base: Path, repository: str) -> tuple[Path, Path] | None:
    local_name = LOCAL_CHECKOUT_ALIASES.get(repository, repository)
    candidates = (
        base / "ecosystem" / repository,
        base / repository,
        base / "ecosystem" / local_name,
        base / local_name,
    )
    directory = next((candidate for candidate in candidates if candidate.is_dir()), None)
    if directory is None:
        return None
    aliased = directory in {base / "ecosystem" / local_name, base / local_name} and local_name != repository
    workflow_directory = base / "pam-native" if aliased else directory
    return directory, workflow_directory


def local_report(packages: list[dict[str, object]], base: Path) -> dict[str, object]:
    if not base.is_dir():
        raise ValueError(f"local ecosystem root does not exist: {base}")
    resolved: list[tuple[dict[str, object], Path, Path]] = []
    missing: list[str] = []
    for package in packages:
        repository = str(package["repository"])
        checkout = local_checkout(base, repository)
        if checkout is None:
            missing.append(repository)
        else:
            resolved.append((package, *checkout))
    if missing:
        raise ValueError(f"missing local ecosystem checkouts: {missing}")

    results: list[dict[str, object]] = []
    for package, directory, workflow_directory in resolved:
        repository = str(package["repository"])
        manifest_bytes, workflow_bytes = verify_checkout(
            packages, directory, repository, workflow_directory
        )
        results.append(
            {
                "repository": repository,
                "composerName": package["composerName"],
                "roleCode": package["roleCode"],
                "resultCode": LocalContractResult.PASSED,
                "manifestSha256": hashlib.sha256(manifest_bytes).hexdigest(),
                "publicationWorkflowSha256": hashlib.sha256(workflow_bytes).hexdigest(),
            }
        )
    return {
        "schemaVersion": 1,
        "resultCode": LocalContractResult.PASSED,
        "packageCount": len(results),
        "results": results,
    }


def verify_local(packages: list[dict[str, object]], base: Path, output: Path) -> None:
    report = local_report(packages, base)
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.is_symlink() or (output.exists() and not output.is_file()):
        raise ValueError("local ecosystem evidence output must be a regular path")
    descriptor, temporary = tempfile.mkstemp(prefix=f".{output.name}.", dir=output.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            descriptor = -1
            handle.write(json.dumps(report, indent=2) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, output)
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


def verify_local_evidence(packages: list[dict[str, object]], base: Path, evidence: Path) -> None:
    document = json.loads(read_bounded_regular(evidence, "local ecosystem evidence"))
    if not isinstance(document, dict) or document != local_report(packages, base):
        raise ValueError("local ecosystem evidence is stale or does not match the checkouts")


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
        elif command == "local" and len(sys.argv) == 4:
            verify_local(packages, Path(sys.argv[2]), Path(sys.argv[3]))
            print(f"Verified {len(packages)} local ecosystem contracts.")
        elif command == "local-verify" and len(sys.argv) == 4:
            verify_local_evidence(packages, Path(sys.argv[2]), Path(sys.argv[3]))
            print(f"Verified local ecosystem evidence for {len(packages)} packages.")
        elif command == "evidence" and len(sys.argv) == 4:
            aggregate_evidence(packages, Path(sys.argv[2]), Path(sys.argv[3]))
            print(f"Aggregated {len(packages) * 2} ecosystem evidence combinations.")
        else:
            raise ValueError(
                "usage: ecosystem-compatibility.py matrix | inventory | verify <directory> <repository> | local <root> <output> | local-verify <root> <evidence> | evidence <directory> <output>"
            )
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"ecosystem compatibility error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
