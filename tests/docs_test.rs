//! The README and the onboarding document lift every Rust sample verbatim from `examples/`.
//! A sample that stops compiling fails `cargo build --examples --features client`; a sample
//! that drifts from its example fails here.

use std::fs;
use std::path::{Path, PathBuf};

const DOCS: &[&str] = &["README.md", "docs/sync-service-onboarding.md"];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Whitespace-insensitive form: a sample is shown de-indented and without blank lines.
fn normalise(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn rust_fences(markdown: &str) -> Vec<String> {
    let mut fences = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in markdown.lines() {
        match &mut current {
            Some(body) if line.trim_start().starts_with("```") => {
                fences.push(body.join("\n"));
                current = None;
            }
            Some(body) => body.push(line),
            None if line.trim() == "```rust" => current = Some(Vec::new()),
            None => {}
        }
    }
    assert!(current.is_none(), "unterminated code fence");
    fences
}

fn examples() -> Vec<(PathBuf, String)> {
    let mut files: Vec<PathBuf> = fs::read_dir(root().join("examples"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let text = normalise(&fs::read_to_string(&p).unwrap());
            (p, text)
        })
        .collect()
}

#[test]
fn every_rust_sample_is_lifted_verbatim_from_an_example() {
    let examples = examples();
    assert!(!examples.is_empty());
    for doc in DOCS {
        let fences = rust_fences(&read(doc));
        assert!(!fences.is_empty(), "{doc} has no rust samples");
        for fence in fences {
            let needle = normalise(&fence);
            assert!(
                examples.iter().any(|(_, hay)| hay.contains(&needle)),
                "{doc}: this sample is not a verbatim excerpt of any file in examples/:\n{fence}"
            );
        }
    }
}

#[test]
fn every_example_named_in_the_docs_exists() {
    for doc in DOCS {
        let text = read(doc);
        for (i, _) in text.match_indices("examples/") {
            let name: String = text[i + "examples/".len()..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                .collect();
            if name.ends_with(".rs") {
                assert!(
                    root().join("examples").join(&name).is_file(),
                    "{doc} names examples/{name}, which does not exist"
                );
            }
        }
    }
}

#[test]
fn the_readme_install_snippet_pins_the_crate_major_minor() {
    let cargo = read("Cargo.toml");
    let version = cargo
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|rest| rest.split('"').next())
        .expect("Cargo.toml version");
    let major_minor = version.rsplit_once('.').map(|(mm, _)| mm).unwrap();
    let expected = format!("river-data-core = {{ version = \"{major_minor}\"");
    assert!(
        read("README.md").contains(&expected),
        "README install snippet must read `{expected}`, crate is {version}"
    );
}
