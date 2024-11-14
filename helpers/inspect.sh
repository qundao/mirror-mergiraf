#!/usr/bin/env bash

if [ "$#" -ne 1 ]; then
    echo "usage: ./helpers/inspect.sh <path>"
    echo "where <path> is a path to a mergiraf test, such as examples/java/working/demo"
    exit 1
fi

script_path="$(realpath "${BASH_SOURCE[0]}")"
script_dir="$(dirname "${script_path}")"

# infer the extension
ext=`ls $1 | grep Base. | sed -e 's/Base.//'`

${script_dir}/run.sh $1 | sed -e '$a\' > /tmp/out$$

echo "------ RESULT ------"
cat /tmp/out$$

echo "------ BASE --------"
cat $1/Base.$ext | sed -e '$a\'
echo "------ LEFT --------"
cat $1/Left.$ext | sed -e '$a\'
echo "------ RIGHT --------"
cat $1/Right.$ext | sed -e '$a\'
echo "------ EXPECTED --------"
if [ -e $1/Better.$ext ]; then
        cat $1/Better.$ext | sed -e '$a\' | tee /tmp/expected$$
elif [ -e $1/ExpectedDiff3.$ext ]; then
        cat $1/ExpectedDiff3.$ext | sed -e '$a\' | tee /tmp/expected$$
else
        cat $1/Expected.$ext | sed -e '$a\' | tee /tmp/expected$$
fi
echo "------ diff ------"
diff -C 3 --color=auto -B /tmp/expected$$ /tmp/out$$

rm /tmp/expected$$ /tmp/out$$
