#!/usr/bin/env python3

import argparse
import json
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--fail-compat', action='store_true')
    parser.add_argument('--fail-noncompat', action='store_true')
    parser.add_argument('--fail-deprecated', action='store_true')
    parser.add_argument('--fail-any', action='store_true')
    args = parser.parse_args()

    result = subprocess.run(
        ['cargo', 'outdated', '--root-deps-only', '--format=json'],
        capture_output=True, text=True, timeout=120,
    )
    if result.returncode not in (0, 1):
        print("Error: 'cargo outdated' failed.", file=sys.stderr)
        if result.stderr:
            print(result.stderr, file=sys.stderr)
        sys.exit(1)

    if not result.stdout.strip():
        print("Error: 'cargo outdated' produced no output.", file=sys.stderr)
        print("Is cargo-outdated installed and is Cargo.lock present?", file=sys.stderr)
        sys.exit(1)

    data = json.loads(result.stdout)
    deps = data.get('dependencies', [])

    compat = []
    noncompat = []
    deprecated = []

    for dep in deps:
        name = dep['name']
        project = dep['project']
        compat_ver = dep.get('compat')
        latest_ver = dep.get('latest')
        is_deprecated = dep.get('deprecated', False)

        if compat_ver and compat_ver != project and compat_ver != '---':
            compat.append((name, project, compat_ver))

        if (latest_ver and latest_ver != project
                and (not compat_ver or latest_ver != compat_ver)):
            noncompat.append((name, project, latest_ver))

        if is_deprecated:
            deprecated.append((name, project))

    n_compat = len(compat)
    n_noncompat = len(noncompat)
    n_deprecated = len(deprecated)
    n_total = n_compat + n_noncompat

    print("=== Outdated Dependencies Report ===")
    print()

    if n_compat:
        print("--- Compatible updates (cargo update -p <pkg>) ---")
        for name, current, ver in compat:
            print(f"  [COMPAT]     {name:<30} {current:<12} -> {ver:<12}   cargo update -p {name}")
        print()

    if n_noncompat:
        print("--- Non-compatible updates (edit Cargo.toml version) ---")
        for name, current, ver in noncompat:
            print(f"  [NON-COMPAT] {name:<30} {current:<12} -> {ver:<12}   cargo +nightly update -Z unstable-options --breaking {name}")
        print()

    if n_deprecated:
        print("--- Deprecated packages ---")
        for name, current in deprecated:
            print(f"  [DEPRECATED] {name:<30} {current:<12}")
        print()

    print(f"Summary: {n_total} outdated, {n_compat} compatible, {n_noncompat} non-compatible, {n_deprecated} deprecated")
    if n_total == 0 and n_deprecated == 0:
        print("All dependencies are up to date.")
    print()

    ret = 0
    if args.fail_any and n_total > 0:
        ret = 1
    if args.fail_compat and n_compat > 0:
        ret = 1
    if args.fail_noncompat and n_noncompat > 0:
        ret = 1
    if args.fail_deprecated and n_deprecated > 0:
        ret = 1
    sys.exit(ret)


if __name__ == '__main__':
    main()
