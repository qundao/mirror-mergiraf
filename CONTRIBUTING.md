We welcome your contributions. Feel invited to open an issue to discuss your plans.
If you are looking into adding support for a new language, check out [the tutorial](https://mergiraf.org/adding-a-language.html). 

We try to invite recurring contributors to join the team following our [governance model](./GOVERNANCE.md). You can also apply on your own.

### Helpers for testing

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

#### Inspecting the tree matchings

After running a test case with `helpers/inspect.sh`, you can run `helpers/generate_svg.sh` which will output SVG files in the `debug` directory.
Those represent the matchings between the trees.

#### Showing the differences between files to merge

The `diff_left.sh`, `diff_right.sh` and `diff_between.sh` scripts in the helpers directory can be used to inspect the differences between pairs of revision with `vimdiff`. Their only argument is the path to a directory representing a test case.
