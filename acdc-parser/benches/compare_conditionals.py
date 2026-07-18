#!/usr/bin/env python3
"""Run paired Criterion comparisons for conditional preprocessing.

BEWARE!: this was all written by Claude, not by me (@nlopes)! I think that's fine, as I
just wanted a comparison harness but this is trash code. You've been warned.

The old and new worktrees must contain the same conditional benchmark source,
Cargo.lock, and Rust toolchain file. The runner builds serially into separate
target directories, alternates revision order for seven pairs, adjusts each
conditional result by the same-size plain-parser control, and writes JSON plus
Markdown reports.

"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import random
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


CASES = (
    "active/1000",
    "inactive/1000",
    "plain_control/1000",
    "slow_path_control/1000",
    "active/10000",
    "inactive/10000",
    "plain_control/10000",
    "slow_path_control/10000",
)
REQUIRED_CASES = ("active/1000", "inactive/1000", "active/10000", "inactive/10000")
CONTROL_CASES = ("plain_control/1000", "plain_control/10000")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--old", required=True, type=Path, help="pre-fix worktree")
    parser.add_argument("--new", required=True, type=Path, help="candidate worktree")
    parser.add_argument("--pairs", type=int, default=7)
    parser.add_argument("--samples", type=int, default=50)
    parser.add_argument("--warm-up-seconds", type=float, default=1.0)
    parser.add_argument("--measurement-seconds", type=float, default=3.0)
    parser.add_argument("--minimum-improvement", type=float, default=2.0)
    parser.add_argument("--maximum-control-drift", type=float, default=1.0)
    parser.add_argument("--bootstrap-resamples", type=int, default=20_000)
    parser.add_argument(
        "--output-dir",
        type=Path,
        help="report/build directory (default: NEW/target/conditional-paired)",
    )
    parser.add_argument(
        "--no-enforce",
        action="store_true",
        help="write reports but do not fail acceptance thresholds",
    )
    args = parser.parse_args()
    if args.pairs < 2:
        parser.error("--pairs must be at least 2")
    if args.samples < 10:
        parser.error("--samples must be at least 10")
    return args


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_inputs(old: Path, new: Path) -> dict[str, str]:
    compared = (
        Path("Cargo.lock"),
        Path("rust-toolchain.toml"),
        Path("acdc-parser/benches/conditional_bench.rs"),
    )
    hashes: dict[str, str] = {}
    for relative in compared:
        old_path = old / relative
        new_path = new / relative
        if not old_path.is_file() or not new_path.is_file():
            raise RuntimeError(f"both worktrees must contain {relative}")
        old_hash = sha256(old_path)
        new_hash = sha256(new_path)
        if old_hash != new_hash:
            raise RuntimeError(f"paired inputs differ: {relative}")
        hashes[str(relative)] = old_hash
    return hashes


def revision(worktree: Path) -> dict[str, Any]:
    head = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=worktree,
        check=True,
        capture_output=True,
        text=True,
    )
    parser_diff = subprocess.run(
        ["git", "diff", "--binary", "HEAD", "--", "acdc-parser/src"],
        cwd=worktree,
        check=True,
        capture_output=True,
    ).stdout
    return {
        "head": head.stdout.strip(),
        "parser_diff_sha256": hashlib.sha256(parser_diff).hexdigest(),
        "parser_dirty": bool(parser_diff),
    }


def build_benchmark(worktree: Path, target_dir: Path) -> Path:
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(target_dir)
    command = [
        "cargo",
        "bench",
        "--locked",
        "-p",
        "acdc-parser",
        "--bench",
        "conditional_bench",
        "--all-features",
        "--no-run",
        "--message-format=json-render-diagnostics",
    ]
    process = subprocess.run(
        command,
        cwd=worktree,
        env=env,
        check=True,
        capture_output=True,
        text=True,
    )
    executable: Path | None = None
    for line in process.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        target = message.get("target", {})
        if (
            message.get("reason") == "compiler-artifact"
            and target.get("name") == "conditional_bench"
            and message.get("executable")
        ):
            executable = Path(message["executable"])
    if executable is None:
        raise RuntimeError(f"cargo did not report the benchmark executable:\n{process.stderr}")
    return executable


def read_median(criterion_home: Path, case: str, baseline: str) -> float:
    estimates = criterion_home / "conditionals" / case / baseline / "estimates.json"
    try:
        data = json.loads(estimates.read_text(encoding="utf-8"))
        return float(data["median"]["point_estimate"])
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        raise RuntimeError(f"could not read Criterion median from {estimates}") from error


def run_case(
    executable: Path,
    worktree: Path,
    criterion_home: Path,
    case: str,
    baseline: str,
    args: argparse.Namespace,
) -> float:
    env = os.environ.copy()
    env["CRITERION_HOME"] = str(criterion_home)
    command = [
        str(executable),
        "--bench",
        f"conditionals/{case}",
        "--exact",
        "--color",
        "never",
        "--noplot",
        "--sample-size",
        str(args.samples),
        "--warm-up-time",
        str(args.warm_up_seconds),
        "--measurement-time",
        str(args.measurement_seconds),
        "--save-baseline",
        baseline,
    ]
    process = subprocess.run(
        command,
        cwd=worktree,
        env=env,
        capture_output=True,
        text=True,
    )
    if process.returncode != 0:
        raise RuntimeError(
            f"benchmark failed ({' '.join(command)}):\n{process.stdout}\n{process.stderr}"
        )
    return read_median(criterion_home, case, baseline)


def percentile(sorted_values: list[float], probability: float) -> float:
    position = probability * (len(sorted_values) - 1)
    lower = int(position)
    upper = min(lower + 1, len(sorted_values) - 1)
    fraction = position - lower
    return sorted_values[lower] * (1.0 - fraction) + sorted_values[upper] * fraction


def median_ci(values: list[float], resamples: int, seed: int) -> tuple[float, float]:
    rng = random.Random(seed)
    count = len(values)
    bootstrap = [
        statistics.median(values[rng.randrange(count)] for _ in range(count))
        for _ in range(resamples)
    ]
    bootstrap.sort()
    return percentile(bootstrap, 0.025), percentile(bootstrap, 0.975)


def summarize(
    measurements: dict[str, list[dict[str, float]]], resamples: int
) -> dict[str, dict[str, Any]]:
    summaries: dict[str, dict[str, Any]] = {}
    for case, pairs in measurements.items():
        raw = [100.0 * (pair["old_ns"] - pair["new_ns"]) / pair["old_ns"] for pair in pairs]
        raw_ci = median_ci(raw, resamples, seed=0xACDC + sum(map(ord, case)))
        summary: dict[str, Any] = {
            "raw_improvement_percent": {
                "paired_values": raw,
                "median": statistics.median(raw),
                "ci95": list(raw_ci),
            }
        }
        kind, size = case.split("/")
        if kind != "plain_control":
            controls = measurements[f"plain_control/{size}"]
            adjusted = []
            for pair, control in zip(pairs, controls, strict=True):
                case_ratio = pair["new_ns"] / pair["old_ns"]
                control_ratio = control["new_ns"] / control["old_ns"]
                adjusted.append(100.0 * (1.0 - case_ratio / control_ratio))
            adjusted_ci = median_ci(adjusted, resamples, seed=0x423 + sum(map(ord, case)))
            summary["control_adjusted_improvement_percent"] = {
                "paired_values": adjusted,
                "median": statistics.median(adjusted),
                "ci95": list(adjusted_ci),
            }
        summaries[case] = summary
    return summaries


def acceptance_failures(
    summaries: dict[str, dict[str, Any]],
    minimum_improvement: float,
    maximum_control_drift: float,
) -> tuple[list[str], list[str]]:
    invalidations = []
    failures = []
    for case in CONTROL_CASES:
        median = summaries[case]["raw_improvement_percent"]["median"]
        if abs(median) > maximum_control_drift:
            invalidations.append(
                f"{case} median drift {median:+.3f}% exceeds +/-{maximum_control_drift:.3f}%"
            )
    for case in REQUIRED_CASES:
        result = summaries[case]["control_adjusted_improvement_percent"]
        median = result["median"]
        lower = result["ci95"][0]
        if median < minimum_improvement:
            failures.append(
                f"{case} adjusted median {median:+.3f}% is below {minimum_improvement:.3f}%"
            )
        if lower <= 0.0:
            failures.append(f"{case} adjusted 95% CI crosses zero: {result['ci95']}")
    return invalidations, failures


def markdown_report(report: dict[str, Any]) -> str:
    lines = [
        "# Conditional paired benchmark",
        "",
        f"- Old: `{report['revisions']['old']['head']}` "
        f"(parser diff `{report['revisions']['old']['parser_diff_sha256']}`)",
        f"- New: `{report['revisions']['new']['head']}` "
        f"(parser diff `{report['revisions']['new']['parser_diff_sha256']}`)",
        f"- Pairs: {report['configuration']['pairs']} (alternating order)",
        f"- Samples: {report['configuration']['samples']}",
        f"- Measurement: {report['configuration']['measurement_seconds']} seconds per run",
        "",
        "| Case | Raw median | Adjusted median | Adjusted 95% CI |",
        "|---|---:|---:|---:|",
    ]
    for case in CASES:
        summary = report["summaries"][case]
        raw = summary["raw_improvement_percent"]["median"]
        adjusted = summary.get("control_adjusted_improvement_percent")
        if adjusted is None:
            lines.append(f"| `{case}` | {raw:+.3f}% | control | - |")
        else:
            lower, upper = adjusted["ci95"]
            lines.append(
                f"| `{case}` | {raw:+.3f}% | {adjusted['median']:+.3f}% | "
                f"[{lower:+.3f}%, {upper:+.3f}%] |"
            )
    if report["invalidations"]:
        result = "INVALID"
    else:
        result = "PASS" if report["passed"] else "FAIL"
    lines.extend(["", f"Result: **{result}**"])
    if report["invalidations"]:
        lines.extend(["", "Invalidations:"])
        lines.extend(f"- {reason}" for reason in report["invalidations"])
    if report["failures"]:
        lines.extend(["", "Failures:"])
        lines.extend(f"- {failure}" for failure in report["failures"])
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    args = parse_args()
    old = args.old.resolve()
    new = args.new.resolve()
    output_dir = (args.output_dir or new / "target/conditional-paired").resolve()
    run_id = f"{time.strftime('%Y%m%d-%H%M%S')}-{os.getpid()}"
    output_dir.mkdir(parents=True, exist_ok=True)

    try:
        input_hashes = validate_inputs(old, new)
        revisions = {"old": revision(old), "new": revision(new)}
        executables = {
            "old": build_benchmark(old, output_dir / "build-old"),
            "new": build_benchmark(new, output_dir / "build-new"),
        }
        criterion_homes = {
            "old": output_dir / "criterion-old",
            "new": output_dir / "criterion-new",
        }

        measurements: dict[str, list[dict[str, float]]] = {case: [] for case in CASES}
        for pair in range(args.pairs):
            order = ("old", "new") if pair % 2 == 0 else ("new", "old")
            rotated_cases = CASES[pair % len(CASES) :] + CASES[: pair % len(CASES)]
            print(f"pair {pair + 1}/{args.pairs}: {' -> '.join(order)}", flush=True)
            for case in rotated_cases:
                result: dict[str, float] = {}
                for label in order:
                    baseline = f"paired-{run_id}-{pair}-{case.replace('/', '-')}-{label}"
                    print(f"  {case} {label}", flush=True)
                    result[f"{label}_ns"] = run_case(
                        executables[label],
                        old if label == "old" else new,
                        criterion_homes[label],
                        case,
                        baseline,
                        args,
                    )
                measurements[case].append(result)

        summaries = summarize(measurements, args.bootstrap_resamples)
        invalidations, failures = acceptance_failures(
            summaries, args.minimum_improvement, args.maximum_control_drift
        )
        report = {
            "revisions": revisions,
            "input_sha256": input_hashes,
            "configuration": {
                "pairs": args.pairs,
                "samples": args.samples,
                "warm_up_seconds": args.warm_up_seconds,
                "measurement_seconds": args.measurement_seconds,
                "minimum_improvement_percent": args.minimum_improvement,
                "maximum_control_drift_percent": args.maximum_control_drift,
                "bootstrap_resamples": args.bootstrap_resamples,
            },
            "measurements": measurements,
            "summaries": summaries,
            "invalidations": invalidations,
            "failures": failures,
            "passed": not invalidations and not failures,
        }
        json_path = output_dir / f"conditional-paired-{run_id}.json"
        markdown_path = output_dir / f"conditional-paired-{run_id}.md"
        json_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        markdown_path.write_text(markdown_report(report), encoding="utf-8")
        print(markdown_report(report), end="")
        print(f"JSON: {json_path}")
        print(f"Markdown: {markdown_path}")
        if args.no_enforce or report["passed"]:
            return 0
        return 2 if invalidations else 1
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
