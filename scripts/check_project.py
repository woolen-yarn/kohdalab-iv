from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
from collections.abc import Callable, Sequence
from pathlib import Path


Runner = Callable[[Sequence[str], Path, dict[str, str] | None], None]


def _run(command: Sequence[str], root: Path, env: dict[str, str] | None = None) -> None:
    print(f"+ {' '.join(command)}", flush=True)
    subprocess.run(command, cwd=root, env=env, check=True)


def run_quality_checks(root: Path, *, runner: Runner = _run) -> None:
    environment = os.environ.copy()
    environment.setdefault("QT_QPA_PLATFORM", "offscreen")
    runner(("uv", "lock", "--check"), root, None)
    runner(("uv", "run", "ruff", "check", "."), root, None)
    runner(("uv", "run", "ruff", "format", "--check", "."), root, None)
    runner(("uv", "run", "mypy"), root, None)
    runner(
        ("uv", "run", "pytest", "--cov", "--cov-branch", "-q"),
        root,
        environment,
    )
    runner(
        (sys.executable, "scripts/verify_release.py"),
        root,
        None,
    )


def _require_empty(directory: Path) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    if any(directory.iterdir()):
        raise RuntimeError(f"Distribution directory must be empty: {directory}")


def run_package_checks(
    root: Path,
    directory: Path,
    *,
    tag: str | None = None,
    runner: Runner = _run,
) -> None:
    _require_empty(directory)
    release_command = [sys.executable, "scripts/verify_release.py"]
    if tag is not None:
        release_command.extend(("--tag", tag))
    runner(release_command, root, None)
    runner(
        ("uv", "build", "--no-sources", "--out-dir", str(directory)),
        root,
        None,
    )
    wheels = sorted(directory.glob("*.whl"))
    source_archives = sorted(directory.glob("*.tar.gz"))
    if len(wheels) != 1 or len(source_archives) != 1:
        raise RuntimeError("Build must produce exactly one wheel and source archive.")
    runner(
        (sys.executable, "scripts/verify_distributions.py", str(directory)),
        root,
        None,
    )
    runner(
        (
            sys.executable,
            "-m",
            "twine",
            "check",
            "--strict",
            str(wheels[0]),
            str(source_archives[0]),
        ),
        root,
        None,
    )
    runner(("check-wheel-contents", str(wheels[0])), root, None)


def run_audit_checks(root: Path, *, runner: Runner = _run) -> None:
    runner(("uv", "lock", "--check"), root, None)
    with tempfile.TemporaryDirectory(prefix="kohdalab-iv-audit-") as temporary:
        lock_directory = Path(temporary)
        runner(
            (
                "uv",
                "export",
                "--format",
                "pylock.toml",
                "--all-extras",
                "--all-groups",
                "--no-emit-project",
                "--locked",
                "--quiet",
                "--output-file",
                str(lock_directory / "pylock.toml"),
            ),
            root,
            None,
        )
        runner(
            (
                "pip-audit",
                "--locked",
                str(lock_directory),
                "--strict",
                "--progress-spinner",
                "off",
            ),
            root,
            None,
        )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run the same KohdaLab IV checks locally and in CI."
    )
    parser.add_argument(
        "mode",
        choices=("quality", "package", "audit", "all"),
        nargs="?",
        default="quality",
    )
    parser.add_argument(
        "--tag", help="Tag to verify in package mode, for example v0.3.0"
    )
    parser.add_argument("--dist-dir", type=Path)
    parser.add_argument("--project-root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    root = args.project_root.resolve()

    if args.mode in {"quality", "all"}:
        run_quality_checks(root)
    if args.mode in {"package", "all"}:
        if args.dist_dir is not None:
            run_package_checks(root, args.dist_dir.resolve(), tag=args.tag)
        else:
            with tempfile.TemporaryDirectory(prefix="kohdalab-iv-dist-") as temporary:
                run_package_checks(root, Path(temporary), tag=args.tag)
    if args.mode == "audit":
        run_audit_checks(root)
    print(f"KohdaLab IV {args.mode} checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
