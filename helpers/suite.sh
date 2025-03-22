#!/bin/bash

if [ "$#" -ne 1 ]; then
    echo "usage: ./helpers/suite.sh <path>"
    echo "where <path> is a path to a directory containing mergiraf tests, such as examples/java/working"
    exit 1
fi

suite=$1

script_path="$(realpath "${BASH_SOURCE[0]}")"
script_dir="$(dirname "${script_path}")"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

failed=0
total_count=0
failed_count=0
for testid in `find $suite -type d`; do
    ext=`ls $testid | grep Base. | sed -e 's/Base.//'`
    if [ -z "$ext" ]; then
        continue
    fi
    if [ ! -e "$testid/Expected.$ext" ]; then
        continue
    fi
    total_count=$((total_count+1))

    if [ -e $testid/_skip ]; then
        echo -e "${YELLOW}SKIP${NC}: ${testid}"
        continue
    fi

    NO_DEBUG="true" /usr/bin/time -o /tmp/timings -f "%e" ${script_dir}/run.sh $testid > /tmp/our_merge_raw.$ext 2> /dev/null
    retcode=$?
    cat /tmp/our_merge_raw.$ext | sed -e '$a\' > /tmp/our_merge.$ext
    if [ -e "$testid/Better.$ext" ]; then
        expected_orig="$testid/Better.$ext"
    elif [ -e "$testid/ExpectedDiff3.$ext" ]; then
        expected_orig="$testid/ExpectedDiff3.$ext"
    else
        expected_orig="$testid/Expected.$ext"
    fi
    cat $expected_orig | sed -e '$a\' > /tmp/expected_merge.$ext

    # Count conflicts in both files
    expected_conflicts=`cat $expected_orig | grep "<<<<<<<" | wc -l`
    our_conflicts=`cat /tmp/our_merge_raw.$ext | grep "<<<<<<<" | wc -l`
    line_base_conflicts=`git merge-file -p $testid/Left.$ext $testid/Base.$ext $testid/Right.$ext | grep "<<<<<<<" | wc -l`
    conflict_summary="$expected_conflicts -> $our_conflicts -> $line_base_conflicts"
    timing=`tail -1 /tmp/timings`
    conflict_summary="$conflict_summary, $timing"
    
    if diff -B /tmp/our_merge.$ext /tmp/expected_merge.$ext > /dev/null 2>&1; then
        echo -e "${GREEN}PASS${NC}: (${conflict_summary}) ${testid}"
    else
        failed=1
        failed_count=$((failed_count+1))
        if cargo run --bin mgf_dev -- compare --commmutative /tmp/our_merge.$ext /tmp/expected_merge.$ext > /dev/null 2>&1; then
            echo -e "${PURPLE}FORM${NC}: (${conflict_summary}) ${testid}"
            continue
        fi
        if [ $retcode -ge 128 ]; then
            echo -e "${RED}BOOM${NC}: (${conflict_summary}) ${testid}"
        else
            echo -e "${RED}FAIL${NC}: (${conflict_summary}) ${testid}"
        fi
    fi;
done;

echo "Failed: ${failed_count}, total: ${total_count}"

exit $failed
