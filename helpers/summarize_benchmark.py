#!/usr/bin/env python3
import sys
import csv
import math
from collections import defaultdict

"""
Utility that summarizes the outcomes of a benchmark run with the `helpers/benchmark.sh` script.
When supplied with the path to a single benchmark TSV log, it gives statistics about how many
test cases ended up in different categories, as a Markdown table.
When supplied with two such paths, it generates
- a comparison between the logs, as a similar table
- a confusion matrix indicating from which status to which status test cases changed
"""

# The order in which test case categories should be presented
# Roughly from "best to worst", so that the confusion matrix
# can be intuitively interpreted as satisfying if it is lower-triangular
status_order = ["Exact", "Format", "Conflict", "Differ", "Parse", "Panic"]


def to_dict(generator):
    """
    Takes a stream from a TSV file, eats the first row as a header
    and yields the rest of rows as dictionaries instead of lists
    """
    header = next(generator)
    for row in generator:
        yield dict(zip(header, row))


class TimingStats:
    """
    Utility to compute an average out of many data points.
    We could also compute the standard deviation by computing the sum of squares.
    """

    def __init__(self):
        self.count = 0
        self.sum = 0

    def add(self, timing):
        self.count += 1
        self.sum += timing

    def average(self):
        return float(self.sum) / self.count


class StatsLine:
    """
    Statistics about how many test cases land in each status category,
    and how long they took.
    """

    def __init__(self):
        self.timing = TimingStats()
        self.states = defaultdict(int)

    def add(self, case):
        timing = float(case["timing"])
        self.timing.add(timing)
        status = case["status"]
        self.states[status] += 1

    def to_markdown(self):
        cases = self.timing.count
        parts = [f"{cases:,}"]
        for status in status_order:
            count = self.states[status]
            if count:
                parts.append(f"{count:,} ({100 * count / cases:.0f}%)")
            else:
                parts.append("0")

        timing = self.timing.average()
        parts.append(f"{timing:.3f}")
        return "| " + (" | ".join(parts)) + " |"


class StatsDiff:
    """
    Difference between two `StatsLine` from two different benchmark runs
    """

    def __init__(self, first: StatsLine, second: StatsLine):
        self.first = first
        self.second = second

    def to_markdown(self):
        timing = self.second.timing.average()
        timing_diff = timing - self.first.timing.average()
        parts = [f"{self.second.timing.count:,}"]
        for status in status_order:
            first = self.first.states[status]
            second = self.second.states[status]
            if second - first and self.first.timing.count:
                percent_change = 100 * (second - first) / self.first.timing.count
                parts.append(f"{second - first:+,} **({percent_change:+.1f}%)**")
            else:
                parts.append(f"{second - first:+,}")

        if math.fabs(timing_diff) > 0.001:
            if timing > 0:
                percent_change = 100 * timing_diff / self.first.timing.average()
                parts.append(f"{timing_diff:+.3f} ({percent_change:+.1f}%)")
            else:
                parts.append(f"{timing_diff:+.3f}%)")
        else:
            parts.append(f"{timing:.3f}")
        return "| " + (" | ".join(parts)) + " |"


class BenchmarkLog:
    def __init__(self, path: str, restrict_to=None):
        """
        Parses a benchmark log as a TSV file,
        returning global and per-language statistics.
        If another BenchmarkLog is passed as `restrict_to`,
        then only cases which are also covered by the other
        log are taken into account.
        """
        self.global_stats = StatsLine()
        self.per_language = defaultdict(StatsLine)
        self.case_to_status = {}
        with open(path, "r") as f:
            csv_reader = csv.reader(f, delimiter="\t")
            for case in to_dict(csv_reader):
                if restrict_to is None or case["case"] in restrict_to.case_to_status:
                    self.global_stats.add(case)
                    self.per_language[case["language"]].add(case)
                    self.case_to_status[case["case"]] = case["status"]


def print_header():
    """
    Prints the header of a Markdown table representing test case categories
    """
    # fmt: off
    print("| Language | Cases | " + " | ".join(status_order) + " | Time (s) |")
    print("| -------- | ----- | " + " | ".join(["-" * len(status) for status in status_order]) + " | -------- |")
    # fmt: on

def summarize_benchmark_log(path: str):
    """
    Prints a summary of a single benchmark
    """
    print_header()
    log = BenchmarkLog(path)
    lang_stats = list(log.per_language.items())
    lang_stats.sort(key=lambda pair: -pair[1].timing.count)
    for lang, stats in lang_stats:
        print(f"| `{lang}` {stats.to_markdown()}")

    if len(log.per_language) > 1:
        print(f"| **Total** {log.global_stats.to_markdown()}")


def compare_benchmark_logs(path_before: str, path_after: str):
    """
    Prints a summary of the differences between two benchmark runs
    """
    print_header()
    after_log = BenchmarkLog(path_after)
    before_log = BenchmarkLog(path_before, restrict_to=after_log)
    after_lang_stats = list(after_log.per_language.items())
    after_lang_stats.sort(key=lambda pair: -pair[1].timing.count)
    for lang, stats in after_lang_stats:
        diff = StatsDiff(before_log.per_language[lang], stats)
        print(f"| `{lang}` {diff.to_markdown()}")

    if len(after_log.per_language) > 1:
        global_diff = StatsDiff(before_log.global_stats, after_log.global_stats)
        print(f"| **Total** {global_diff.to_markdown()}")

    print("")

    confusion_matrix = defaultdict(set)
    for case, status in after_log.case_to_status.items():
        previous_status = before_log.case_to_status.get(case)
        if not previous_status:
            print(
                f"warning: case not found in previous benchmark: {case}",
                file=sys.stderr,
            )
        else:
            confusion_matrix[(previous_status, status)].add(case)

    # fmt: off
    print("| ↓ Before \\ After → | " + " | ".join(status_order) + " |")
    print("| ------------------ | "  + " | ".join(["-" * len(status) for status in status_order]) + " |")
    # fmt: on

    for status in status_order:
        row = []
        for new_status in status_order:
            cell = ""
            if confusion_matrix[(status, new_status)]:
                cell = f"{len(confusion_matrix[(status, new_status)]):,}"
            if cell and status != new_status:
                cell = f"**{cell}**"
            row.append(cell)
        print("| " + status + " | " + " | ".join(row) + " |")

    print("")
    print("## Suspicious status changes")
    print("")
    for index, status in enumerate(status_order):
        for new_status in status_order[(index + 1) :]:
            cases = confusion_matrix[(status, new_status)]
            if cases:
                print(f"### {status} → {new_status}")
                for case in cases:
                    print(case)
                print("")


if __name__ == "__main__":
    if len(sys.argv) == 3:
        compare_benchmark_logs(sys.argv[1], sys.argv[2])
    else:
        summarize_benchmark_log(sys.argv[1])
