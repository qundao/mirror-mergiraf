We welcome your contributions.

You are welcome to ask for help or reach out about any topic in the [issue tracker](https://codeberg.org/mergiraf/mergiraf/issues).

We try to invite recurring contributors to join the team following our [governance model](./GOVERNANCE.md). You can also apply on your own.

## Documentation contributions

Documentation is stored in the `doc` folder, and rendered by the [`mdbook`](https://rust-lang.github.io/mdBook/) tool.
It is deployed to [mergiraf.org](https://mergiraf.org/) at each release.

To preview the effect of your changes, run `mdbook serve`.

## Code contributions

Feel invited to open an issue to discuss your plans.
If you are looking into adding support for a new language, check out [the tutorial](https://mergiraf.org/adding-a-language.html).
Mergiraf is written in Rust, see [Getting started with Rust](https://rust-lang.org/learn/get-started/) to set up your environment for working on it.

### Testing your changes

The `examples/` directory collects end-to-end test cases. Each test case is defined by a directory containing the inputs and expected output as separate files.
Running `cargo test` executes this test suite, as well as other Rust tests.

To run mergiraf on a single test case, such as the one stored in `examples/java/working/add_same_import`, you can run:
```
helpers/inspect.sh examples/java/working/add_same_import
```
This will show detailed information about the execution of the test case, including mergiraf's logs.

To run mergiraf on a set of test cases, you can run:
```
helpers/suite.sh my_test_suite
```
where `my_test_suite` is the path to a directory containing test cases (such as `examples/java/working`).

#### Inspecting the parse tree

To inspect how Mergiraf parses a file, you can run:
```
cargo parse my_file.java
```

#### Inspecting the tree matchings

After running a test case with `helpers/inspect.sh`, you can run `helpers/generate_svg.sh` which will output SVG files in the `debug` directory.
Those represent the matchings between the trees.

#### Showing the differences between files to merge

The `diff_left.sh`, `diff_right.sh` and `diff_between.sh` scripts in the helpers directory can be used to inspect the differences between pairs of revision with `vimdiff`. Their only argument is the path to a directory representing a test case.

### Minimizing test cases

When mergiraf behaves incorrectly on a particular merge scenario, it can be useful to obtain a minimal example where the problem occurs.
The `cargo minimize` command (shorthand for `cargo run --bin mgf_dev minimize`) can be used to compute such a minimal example from a real-world one.

It requires:
* a directory containing the `Base.*`, `Left.*` and `Right.*` revisions of the merge scenario with matching extensions (like any of the end-to-end test cases in the `examples/` directory)
* a script or command to execute on the merge scenario, whose exit status will be preserved during minimization. The script will be passed one argument: the path to a directory containing the merge scenario being minimized.
* the expected exit status of this script (by default, 0)

For instance, if mergiraf panics on the test case, it will return exit code 134.
To derive a minimal test case where mergiraf panics, you can provide `mergiraf merge $1/Base.xml $1/Left.xml $1/Right.xml` as the script and set 134 as the expected exit status:
```sh
cargo minimize --expected-exit-code 134 /tmp/my_test_case/ "mergiraf merge $1/Base.xml $1/Left.xml $1/Right.xml"
```

If you want to enforce other conditions on the test case, you can use a more complex script. In such cases, it can be useful to write a script as a separate file, for instance checking the presence of certain substrings in certain revisions, and running mergiraf on it in various ways.

**Note:** The minimization algorithm works by parsing the files and aligning them in the same way that mergiraf does, so if the bug you are trying to narrow down is happening in those phases already, this helper might not be so useful.

