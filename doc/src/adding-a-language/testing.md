# Testing language configurations

Adding support for a language in Mergiraf doesn't require any code, just declarative configuration, but it's still worth checking that the merging that it enables works as expected, and that it keeps doing so in the future.

### Directory structure

You can add test cases to the end-to-end suite by following the directory structure of other such test cases. Create a directory of the form:
```
examples/csharp/working/add_imports
```

The naming of the `csharp` directory does not matter, nor does `add_imports` which describes the test case we are about to write. In this directory go the following files:
```
Base.cs
Left.cs
Right.cs
Expected.cs
```

All files should have an extension which matches what you defined in the language profile, for them to be parsed correctly. The `Base`, `Left` and `Right` files contain the contents of a sample file at all three revisions, and `Expected` contains the expected merge output of the tool (including any conflict markers).

If the language you're adding is specified using the full file name (`Makefile`/`pyproject.toml`), the test directory should additionally contain a `language` file with one of the `file_names` specified in the language profile.

For example, here's a directory structure of a `Makefile` test:
```
Base
Left
Right
Expected
language // contains "Makefile" (without the quotes)
```

and for `pyproject.toml`:
```
Base.toml
Left.toml
Right.toml
Expected.toml
language // contains "pyproject.toml"
```

### Running the tests
To run an individual test, you can use a helper:
```console
$ helpers/inspect.sh examples/csharp/working/add_imports
```

This will show any differences between the expected output of the merge and the actual one. It also saves the result of some intermediate stages
of the merging process in the `debug` directory, such as the matchings between the three trees as Dotty graphs.
Those can be viewed as SVG files by running `helpers/generate_svg.sh`.


To run a test with a debugger, you can use the test defined in `tests/integration_tests.rs`:
```rust
// use this test to debug a specific test case by changing the path in it.
#[test]
fn debug_test() {
    run_test_from_dir(Path::new("examples/go/working/remove_and_add_imports"))
}
```
You can then use an IDE (such as Codium with Rust-analyzer) to set up breakpoints to inspect the execution of the test.

