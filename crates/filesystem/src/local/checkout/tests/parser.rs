use crate::local::checkout::parse_diff;
use crate::ports::checkout::DiffLineKind;

#[test]
fn embedded_diff_headers_stay_in_source_instead_of_creating_phantom_files() {
    let code = r#"    parse_diff(&format!("diff --git a/{path} b/{path}\n{body}"))"#;
    for (marker, kind) in [
        ('+', DiffLineKind::Add),
        ('-', DiffLineKind::Remove),
        (' ', DiffLineKind::Context),
    ] {
        let patch = format!(
            "diff --git a/tests.rs b/tests.rs\n--- a/tests.rs\n+++ b/tests.rs\n@@ -1,2 +1,2 @@\n{marker}{code}\n{marker}    .pop()\n"
        );
        let files = parse_diff(&patch);
        assert_eq!(files.len(), 1, "{marker}: {files:?}");
        assert_eq!(files[0].path, "tests.rs");
        assert_eq!(files[0].hunks[0].lines[1].content, code);
        assert_eq!(files[0].hunks[0].lines[1].kind, kind);
        assert_eq!(files[0].hunks[0].lines[2].content, "    .pop()");
    }
}

#[test]
fn embedded_headers_do_not_hide_subsequent_real_files() {
    let patch = concat!(
        "diff --git a/first.rs b/first.rs\n",
        "--- a/first.rs\n+++ b/first.rs\n@@ -0,0 +1,2 @@\n",
        "+// diff --git a/fake.rs b/fake.rs\n",
        "+let value = 1;\n",
        "diff --git a/second.ts b/second.ts\n",
        "--- a/second.ts\n+++ b/second.ts\n@@ -0,0 +1 @@\n",
        "+const value = 2;\n",
    );
    let files = parse_diff(patch);
    assert_eq!(
        files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        ["first.rs", "second.ts"]
    );
    assert_eq!(files[0].additions, 2);
    assert_eq!(files[1].additions, 1);
    assert_eq!(files[0].hunks[0].lines[2].content, "let value = 1;");
}

#[test]
fn inline_header_text_without_a_real_file_header_is_not_a_diff() {
    assert!(parse_diff(r#"format!("diff --git a/fake b/fake")"#).is_empty());
    assert!(parse_diff("").is_empty());
}

#[test]
fn genuine_line_start_headers_work_with_crlf_and_no_final_newline() {
    let patch =
        "diff --git a/a.rs b/a.rs\r\n--- a/a.rs\r\n+++ b/a.rs\r\n@@ -0,0 +1 @@\r\n+fn main() {}";
    let files = parse_diff(patch);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "a.rs");
    assert_eq!(files[0].additions, 1);
}
