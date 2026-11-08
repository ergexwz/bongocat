"""Summarize isolated process-tree CSVs without mixing GPU and process memory."""
import argparse
import csv
import json
from pathlib import Path
from statistics import median

METRICS = (
    "private_commit", "private_working_set", "working_set", "cpu_percent",
    "handles", "threads", "gpu_dedicated", "gpu_shared",
)


def summarize_snapshots(snapshots):
    groups = {}
    timestamps = sorted(snapshots)
    first_timestamp = timestamps[0]
    for role in ("total", "host-service", "native-plugin", "webview2", "console"):
        series = {key: [] for key in METRICS}
        counts = []
        cpu_weights = []
        previous_timestamp = first_timestamp
        for timestamp in timestamps:
            rows = snapshots[timestamp]
            interval = timestamp - previous_timestamp
            previous_timestamp = timestamp
            selected = [row for row in rows if role == "total" or row["role"] == role]
            counts.append(len(selected))
            for key in METRICS:
                # The first CPU delta crosses the warmup/measurement boundary,
                # and can include the 100 latency requests before readiness.
                if key == "cpu_percent" and timestamp == first_timestamp:
                    continue
                values = [float(row[key]) for row in selected if row[key]]
                # An unavailable counter is unknown, not zero GPU usage.
                if values or (not selected and not key.startswith("gpu_")):
                    series[key].append(sum(values))
                    if key == "cpu_percent":
                        cpu_weights.append(interval)
        groups[role] = {key: median(values) if values else None for key, values in series.items()}
        # Sampling includes CIM work and is not exactly periodic. Integrate CPU
        # rates over their actual intervals rather than overweight short gaps.
        groups[role]["cpu_percent"] = (
            sum(value * weight for value, weight in zip(series["cpu_percent"], cpu_weights)) / sum(cpu_weights)
            if sum(cpu_weights) else None
        )
        groups[role]["processes"] = median(counts)
        # First/last 10% help expose sustained growth during a soak.
        for key in ("private_commit", "handles", "threads"):
            values = series[key]
            window = max(1, len(values) // 10)
            groups[role][f"{key}_end_minus_start"] = median(values[-window:]) - median(values[:window]) if values else None
    return groups


def summarize(report_path):
    report = json.loads(report_path.read_text(encoding="utf-8-sig"))
    if report.get("passed") is not True:
        raise ValueError(f"scenario did not complete: {report_path}")
    snapshots = {}
    with report_path.with_suffix(".csv").open(encoding="utf-8-sig", newline="") as source:
        for row in csv.DictReader(source):
            if row["sample"].lower() != "true":
                continue
            snapshots.setdefault(float(row["seconds"]), []).append(row)
    if not snapshots:
        raise ValueError(f"no post-warmup samples: {report_path}")
    result = {**report, "resource_samples": len(snapshots), "groups": summarize_snapshots(snapshots)}
    if report.get("idle_seconds", 0):
        boundary = min(snapshots) + report["idle_seconds"]
        phases = {"idle": {t: rows for t, rows in snapshots.items() if t < boundary},
                  "active": {t: rows for t, rows in snapshots.items() if t >= boundary}}
        result["phases"] = {name: {"resource_samples": len(rows), "groups": summarize_snapshots(rows)} for name, rows in phases.items() if rows}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    cases = {}
    for path in sorted(arguments.directory.rglob("run-*.json")):
        case = str(path.parent.relative_to(arguments.directory)).replace("\\", "/")
        cases.setdefault(case, []).append(summarize(path))
    if not cases:
        raise ValueError("no completed measurement runs")
    output = {}
    for case, runs in cases.items():
        roles = {}
        for role in runs[0]["groups"]:
            roles[role] = {}
            for key in runs[0]["groups"][role]:
                values = [run["groups"][role][key] for run in runs if run["groups"][role][key] is not None]
                roles[role][key] = median(values) if values else None
        output[case] = {"runs": len(runs), "resource_samples": [run["resource_samples"] for run in runs], "request_p95_us": median(run["request_p95_us"] for run in runs), "groups": roles, "individual_runs": runs}
    arguments.output.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
