use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn rust_tests_live_in_dedicated_files() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve Rust reference workspace");
    let mut files = Vec::new();
    collect_rust_files(&workspace, &workspace, &mut files);

    let mut violations = Vec::new();
    for path in files {
        let file_name = path.file_name().and_then(|name| name.to_str()).unwrap();
        let source = fs::read_to_string(&path).unwrap();
        let relative = path.strip_prefix(&workspace).unwrap().display();
        let dedicated = file_name.ends_with("_tests.rs");
        let contains_test = source.lines().any(|line| line.trim() == "#[test]");

        if contains_test && !dedicated {
            violations.push(format!("{relative}: #[test] must live in *_tests.rs"));
        }

        if path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|directory| directory == "tests")
            && !dedicated
        {
            violations.push(format!(
                "{relative}: integration test must end in _tests.rs"
            ));
        }

        if dedicated
            && !path
                .components()
                .any(|component| component.as_os_str() == "tests")
        {
            let implementation_name = format!("{}.rs", file_name.trim_end_matches("_tests.rs"));
            let implementation = path.with_file_name(implementation_name);
            let declaration = format!("#[path = \"{file_name}\"]");
            let declared = fs::read_to_string(&implementation)
                .is_ok_and(|source| source.contains(&declaration));
            if !declared {
                violations.push(format!(
                    "{relative}: sibling implementation must declare {declaration}"
                ));
            }
        }
    }

    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

fn collect_rust_files(workspace: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path == workspace.join("target") {
                continue;
            }
            collect_rust_files(workspace, &path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
