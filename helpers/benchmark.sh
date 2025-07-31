#!/usr/bin/env bash

if [ "$#" -ne 2 ]; then
    echo "usage: $0 <path> <executable>"
    echo "where <path> is a path to a directory containing test cases, such as examples/java/working,"
    echo "and <executable> is a path to a mergiraf binary to benchmark."
    echo "This prints a TSV file summarizing benchmarking results on the standard output."
    echo "It can then be used with helpers/summarize_benchmark.py to get statistics in Markdown."
    exit 1
fi

suite=$1
executable=$2

script_path="$(realpath "${BASH_SOURCE[0]}")"
script_dir="$(dirname "${script_path}")"

tmp_dir=/tmp/tmp$$
mkdir -p $tmp_dir

# Print TSV headers
echo -e "status\ttiming\tlanguage\tcase"

# For each test case…

find -L "$suite" -type d | while read -r testid ; do
    # Detect its language
    ext=$(ls "$testid" | grep Base | sed -e 's/Base//')
    if [ ! -e "$testid/Expected$ext" ]; then
        continue
    fi

    language="*$ext"
    extra_args=""
    if [ -e "$testid/language" ]; then
        language=$(cat "$testid/language")
        extra_args="$extra_args --language $language"
    fi

    # Run the executable to benchmark
    /usr/bin/env time -o $tmp_dir/timings -f "%e" "$executable" merge -v "$testid/Base$ext" "$testid/Left$ext" "$testid/Right$ext" -s BASE -x LEFT -y RIGHT $extra_args > "$tmp_dir/our_merge_raw$ext" 2> $tmp_dir/mergiraf_log
    retcode=$?

    # Normalize line endings both for the expected value and our output
    sed -e '$a\' "$tmp_dir/our_merge_raw$ext" > "$tmp_dir/our_merge$ext"
    sed -e '$a\' "$testid/Expected$ext" > "$tmp_dir/expected_merge$ext"

    timing=$(tail -1 $tmp_dir/timings)

    # Categorize the test outcome
    if [ $retcode -ge 128 ]; then
        outcome="Panic"
    elif diff -B "$tmp_dir/our_merge$ext" "$tmp_dir/expected_merge$ext" > /dev/null 2>&1; then
        outcome="Exact"
    else
        conflict_count=$(grep -c "<<<<<<<" "$tmp_dir/our_merge_raw$ext")
        if [ "$conflict_count" -ge 1 ]; then
            # Check that we were able to parse the files correctly
            if grep "encountered an error: parse error at " $tmp_dir/mergiraf_log > /dev/null; then
                outcome="Parse"
            else
                outcome="Conflict"
            fi
        elif cargo compare --commutative "$tmp_dir/our_merge$ext" "$tmp_dir/expected_merge$ext" > /dev/null 2>&1; then
            outcome="Format"
        else
            outcome="Differ"
        fi;
    fi;
    echo -e "${outcome}\t${timing}\t${language}\t${testid}"
done;

rm -rf ${tmp_dir}
