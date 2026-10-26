use std::{fs, path::Path, path::PathBuf};

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn markdown_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("documentation directory should be readable") {
        let path = entry
            .expect("documentation entry should be readable")
            .path();
        if path.is_dir() {
            markdown_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            files.push(path);
        }
    }
}

#[test]
fn bundled_brush_tips_are_valid_images() {
    let brushes = project_root().join("assets/default_brushes");
    let entries = fs::read_dir(brushes).expect("assets/default_brushes directory should exist");
    let mut count = 0;

    for entry in entries {
        let path = entry.expect("brush entry should be readable").path();
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        {
            let image = image::open(&path)
                .unwrap_or_else(|error| panic!("invalid brush image {}: {error}", path.display()));
            assert!(
                image.width() > 0 && image.height() > 0,
                "{} is empty",
                path.display()
            );
            count += 1;
        }
    }

    assert!(count > 0, "at least one bundled brush tip should exist");
}

#[test]
fn application_documentation_and_release_assets_exist() {
    let root = project_root();
    for relative in [
        "README.md",
        "assets/default_brushes",
        "assets/PaintAtlas.png",
        "assets/PaintAtlas.svg",
        "docs/reference/ARCHITECTURE.md",
    ] {
        assert!(
            root.join(relative).exists(),
            "missing project resource: {relative}"
        );
    }
}

#[test]
fn every_main_module_path_resolves_in_the_shipped_source_tree() {
    let root = project_root();
    let main = fs::read_to_string(root.join("src/main.rs")).expect("src/main.rs should exist");
    let mut checked = 0;

    for line in main.lines() {
        let Some(path) = line.trim().strip_prefix("#[path = \"") else {
            continue;
        };
        let Some(path) = path.strip_suffix("\"]") else {
            panic!("malformed path module declaration: {line}");
        };
        assert!(
            root.join("src").join(path).is_file(),
            "missing module source: {path}"
        );
        checked += 1;
    }

    assert!(
        checked > 10,
        "expected to validate the application's module tree"
    );
}

#[test]
fn project_documentation_index_links_only_to_existing_local_files() {
    let root = project_root();
    let mut documents = Vec::new();
    markdown_files(&root.join("docs"), &mut documents);
    let mut links_checked = 0;

    for document in documents {
        let contents = fs::read_to_string(&document)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", document.display()));
        for line in contents.lines() {
            let mut rest = line;
            while let Some(open) = rest.find("](") {
                rest = &rest[open + 2..];
                let Some(close) = rest.find(')') else { break };
                let target = rest[..close].split('#').next().unwrap_or_default();
                rest = &rest[close + 1..];
                if target.is_empty() || target.contains("://") || target.starts_with("mailto:") {
                    continue;
                }
                let target = target.trim_start_matches('<').trim_end_matches('>');
                let resolved = document.parent().unwrap().join(target);
                assert!(
                    resolved.exists(),
                    "broken documentation link in {}: {target}",
                    document.display()
                );
                links_checked += 1;
            }
        }
    }

    assert!(
        links_checked > 0,
        "expected local links across project documentation"
    );
}

#[test]
fn bundled_application_background_is_a_decodable_image() {
    let bytes = fs::read(project_root().join("assets/background.jpg"))
        .expect("application background should exist");
    // The application embeds and decodes these bytes directly; use content-based
    // format detection because the historical filename has a .jpg suffix.
    let image = image::load_from_memory(&bytes).expect("application background should decode");
    assert!(image.width() > 0 && image.height() > 0);
}

#[test]
fn ci_workflow_runs_the_locked_complete_test_suite() {
    let workflow = project_root().join(".github/workflows/tests.yml");
    let contents = fs::read_to_string(workflow).expect("test workflow should exist");
    assert!(contents.contains("cargo test --all-targets --locked"));
    assert!(contents.contains("ubuntu-latest") && contents.contains("windows-latest"));
}
