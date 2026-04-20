use diffy_imara::{PatchFormatter, create_patch};

mod common;
use common::merge;

#[test]
fn timeout_support() {
    let output = merge()
        .arg("examples/java/working/move_and_modify_conflict/Base.java")
        .arg("examples/java/working/move_and_modify_conflict/Left.java")
        .arg("examples/java/working/move_and_modify_conflict/Right.java")
        .arg("--base-name=BASE")
        .arg("--left-name=LEFT")
        .arg("--right-name=RIGHT")
        .arg("--timeout=1")
        .output()
        .expect("failed to execute `mergiraf merge`");

    let actual = str::from_utf8(&output.stdout).unwrap();
    let expected =
        include_str!("../examples/java/working/move_and_modify_conflict/ExpectedLineBased.java");

    if expected != actual {
        let patch = create_patch(expected, actual);
        let f = PatchFormatter::new().with_color();
        print!("{}", f.fmt_patch(&patch));
        eprintln!("test failed: outputs differ");
        panic!();
    }
}
