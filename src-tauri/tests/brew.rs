mod common;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use common::git::canonical_tmp;
use organizemymac_lib::brew::projects::{parse_brewfile, scan};
use organizemymac_lib::brew::{analyze, classify_change, is_runtime, parse_outdated, read_outdated, risk_of, BrewError, Change, PackageKind, Risk};
use organizemymac_lib::exec::FakeRunner;

const BREW: &str = "/opt/homebrew/bin/brew";

/// The shape `brew outdated --json=v2` documents: formulae with a list of versions, casks with a string.
const JSON: &str = r#"{
  "formulae": [
    {"name": "node", "installed_versions": ["20.11.1"], "current_version": "22.1.0", "pinned": false, "pinned_version": null},
    {"name": "git", "installed_versions": ["2.43.0"], "current_version": "2.44.0", "pinned": false, "pinned_version": null},
    {"name": "wget", "installed_versions": ["1.21.4"], "current_version": "1.21.4_1", "pinned": false, "pinned_version": null},
    {"name": "python@3.11", "installed_versions": ["3.11.7"], "current_version": "3.11.9", "pinned": false, "pinned_version": null},
    {"name": "postgresql@14", "installed_versions": ["14.10"], "current_version": "14.11", "pinned": false, "pinned_version": null},
    {"name": "ffmpeg", "installed_versions": ["6.1"], "current_version": "7.0", "pinned": true, "pinned_version": "6.1"}
  ],
  "casks": [
    {"name": "docker", "installed_versions": "4.27.0", "current_version": "4.28.0"},
    {"name": "figma", "installed_versions": ["124.0"], "current_version": "126.1"},
    {"name": "weirdapp", "installed_versions": "latest", "current_version": "latest"}
  ]
}"#;

#[test]
fn version_changes_are_classified_by_the_first_part_that_differs() {
    let v = |s: &str| vec![s.to_string()];
    assert_eq!(classify_change(&v("20.11.1"), "22.1.0"), Change::Major);
    assert_eq!(classify_change(&v("2.43.0"), "2.44.0"), Change::Minor);
    assert_eq!(classify_change(&v("3.11.7"), "3.11.9"), Change::Patch);
    assert_eq!(classify_change(&v("1.21.4"), "1.21.4_1"), Change::Patch, "a Homebrew revision is a small change");
    assert_eq!(classify_change(&v("v1.2.3"), "1.2.3"), Change::Other, "same numbers");
    assert_eq!(classify_change(&v("latest"), "latest"), Change::Other);
    assert_eq!(classify_change(&[], "1.0"), Change::Other);
    // With several installed versions, the newest one is the baseline.
    assert_eq!(classify_change(&["1.0.0".into(), "2.5.0".into()], "2.6.0"), Change::Minor);
    // Beta labels and shorter versions do not crash or lie.
    assert_eq!(classify_change(&v("1.2"), "1.2.1"), Change::Patch);
    assert_eq!(classify_change(&v("1.9.9"), "1.10.0"), Change::Minor, "numbers, not text: 10 > 9");
}

#[test]
fn risk_follows_the_jump_and_runtimes_are_treated_with_more_care() {
    use PackageKind::*;
    assert_eq!(risk_of(Formula, Change::Major, false, false), Risk::High);
    assert_eq!(risk_of(Formula, Change::Minor, false, false), Risk::Medium);
    assert_eq!(risk_of(Formula, Change::Patch, false, false), Risk::Low);
    assert_eq!(risk_of(Formula, Change::Other, false, false), Risk::Medium, "unknown size of jump is not low");
    // A runtime that other things link against: a minor jump already deserves attention.
    assert_eq!(risk_of(Formula, Change::Minor, true, false), Risk::High);
    assert_eq!(risk_of(Formula, Change::Patch, true, false), Risk::Low);
    // Apps (casks) are never high: no project links against them.
    assert_eq!(risk_of(Cask, Change::Major, false, false), Risk::Medium);
    assert_eq!(risk_of(Cask, Change::Minor, false, false), Risk::Low);
    // Pinned wins over everything: `brew upgrade` will not touch it.
    assert_eq!(risk_of(Formula, Change::Major, true, true), Risk::Pinned);
    assert!(Risk::High > Risk::Medium && Risk::Medium > Risk::Low && Risk::Low > Risk::Pinned);
    for n in ["node", "node@20", "python@3.11", "postgresql@14", "go", "rustup"] {
        assert!(is_runtime(n), "{n}");
    }
    for n in ["git", "wget", "ffmpeg", "nodemon"] {
        assert!(!is_runtime(n), "{n}");
    }
}

#[test]
fn parses_the_documented_output_and_orders_by_risk() {
    let list = parse_outdated(JSON).unwrap();
    let names: Vec<_> = list.iter().map(|o| o.name.as_str()).collect();
    // High: node (major), postgresql@14 (runtime minor). Medium: git, figma (cask major). Low: ... Pinned last.
    assert_eq!(&names[..2], ["node", "postgresql@14"]);
    assert_eq!(*names.last().unwrap(), "ffmpeg");
    let get = |n: &str| list.iter().find(|o| o.name == n).unwrap();
    assert_eq!(get("node").upgrade_command, "brew upgrade node");
    assert_eq!(get("docker").upgrade_command, "brew upgrade --cask docker");
    assert_eq!(get("docker").installed, ["4.27.0"], "a cask's version is a string, not a list");
    assert_eq!((get("ffmpeg").pinned, get("ffmpeg").risk), (true, Risk::Pinned));
    assert_eq!(get("wget").change, Change::Patch);
    assert_eq!(get("weirdapp").change, Change::Other);
    assert!(get("python@3.11").runtime && !get("git").runtime);
    assert!(parse_outdated("not json").is_err());
    assert_eq!(parse_outdated(r#"{"formulae":[],"casks":[]}"#).unwrap().len(), 0);
    assert_eq!(parse_outdated("{}").unwrap().len(), 0, "missing sections mean nothing outdated");
}

#[test]
fn the_only_command_ever_run_is_outdated_with_updates_and_analytics_off() {
    let fake = FakeRunner::new().route("outdated", JSON);
    let report = analyze(&fake, Some(BREW), &[]);
    assert!(report.installed && report.error.is_none() && report.packages.len() == 9);
    assert_eq!(fake.calls(), [format!("{BREW} outdated --json=v2")]);
    let env: std::collections::HashMap<_, _> = fake.envs()[0].iter().cloned().collect();
    assert_eq!(env.get("HOMEBREW_NO_AUTO_UPDATE").map(String::as_str), Some("1"));
    assert_eq!(env.get("HOMEBREW_NO_ANALYTICS").map(String::as_str), Some("1"));
}

#[test]
fn the_module_cannot_run_anything_that_changes_homebrew() {
    // A guard on the source: every argument list in the module is `outdated`, and no mutating verb appears.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/brew");
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let text = fs::read_to_string(entry.path()).unwrap();
        let code: String = text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        for verb in ["\"install\"", "\"uninstall\"", "\"upgrade\"", "\"update\"", "\"cleanup\"", "\"remove\"", "\"reinstall\"", "\"pin\"", "\"unpin\"", "\"services\"", "\"link\"", "\"tap\""] {
            assert!(!code.contains(verb), "{} runs or mentions {verb}", entry.path().display());
        }
        assert_eq!(code.matches("Cmd::new(").count(), usize::from(entry.file_name() == "mod.rs"), "{}", entry.path().display());
    }
}

#[test]
fn not_installed_and_failures_are_reported_plainly() {
    let none = analyze(&FakeRunner::new(), None, &[]);
    assert!(!none.installed && none.packages.is_empty() && none.error.is_none());

    let failing = FakeRunner::new().route_failure("outdated", 1, "Error: something broke\nmore detail");
    let r = analyze(&failing, Some(BREW), &[]);
    assert!(r.installed && r.packages.is_empty());
    assert_eq!(r.error.as_deref(), Some("Error: something broke"));

    let garbage = FakeRunner::new().route("outdated", "<html>");
    assert!(analyze(&garbage, Some(BREW), &[]).error.unwrap().contains("resposta inesperada"));
    assert!(matches!(read_outdated(&FakeRunner::new().route_failure("outdated", 2, ""), BREW), Err(BrewError::Failed { .. })));
}

// ---- which projects use a package

#[test]
fn a_brewfile_is_exact_and_tap_prefixes_are_dropped() {
    let text = "# my tools\ntap \"homebrew/cask\"\nbrew \"git\"\nbrew 'node'\nbrew \"hashicorp/tap/terraform\", args: [\"HEAD\"]\ncask \"docker\"\n  brew \"jq\"\nbrewish \"nope\"\nbrew git-without-quotes\n";
    let parsed = parse_brewfile(text);
    let names: Vec<_> = parsed.iter().map(|(n, k)| (n.as_str(), *k)).collect();
    assert_eq!(names, [("git", PackageKind::Formula), ("node", PackageKind::Formula), ("terraform", PackageKind::Formula), ("docker", PackageKind::Cask), ("jq", PackageKind::Formula)]);
}

fn touch(root: &std::path::Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

#[test]
fn projects_are_matched_by_toolchain_files_and_brewfile_entries() {
    let (_t, root) = canonical_tmp();
    let web = root.join("web");
    touch(&web, "package.json", "{}");
    touch(&web, "apps/api/Cargo.toml", "");
    touch(&web, "Brewfile", "brew \"node\"\ncask \"docker\"\n");
    let py = root.join("py");
    touch(&py, "requirements.txt", "");
    touch(&py, ".tool-versions", "nodejs 20.1.0\npython 3.11.7\nunknown 1\n");
    let none = root.join("none");
    touch(&none, "README.md", "");
    let idx = scan(&[web.clone(), py.clone(), none.clone()]);
    use PackageKind::*;

    let node = idx.uses("node", Formula);
    assert_eq!(node.len(), 2, "{node:?}");
    assert_eq!((node[0].repo.clone(), node[0].exact, node[0].via.as_str()), (web.clone(), true, "Brewfile"), "exact entries come first");
    assert_eq!((node[1].repo.clone(), node[1].exact), (py.clone(), false), "a .tool-versions entry is an estimate");
    // One entry per repository even when several files point at the same package.
    assert_eq!(idx.uses("node", Formula).iter().filter(|u| u.repo == web).count(), 1);
    assert_eq!(idx.uses("rust", Formula)[0].via, "apps/api/Cargo.toml", "a monorepo package counts");
    assert_eq!(idx.uses("python@3.11", Formula).iter().map(|u| u.repo.clone()).collect::<Vec<_>>(), [py.clone()]);
    assert_eq!(idx.uses("docker", Cask)[0].exact, true);
    assert!(idx.uses("docker", Formula).is_empty(), "a cask line does not name the formula");
    // A package nothing points to, and a repo with no signals.
    assert!(idx.uses("imagemagick", Formula).is_empty());
    assert!(idx.uses("node", Formula).iter().all(|u| u.repo != none));
}

#[test]
fn the_project_scan_skips_regenerable_folders_symlinks_and_nested_repos() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("app");
    touch(&repo, "README.md", "");
    touch(&repo, "node_modules/dep/package.json", "{}"); // a dependency, not the project
    touch(&repo, "target/debug/Cargo.toml", "");
    touch(&repo, "deep/a/b/c/d/Cargo.toml", ""); // too deep to be the project's own
    let elsewhere = root.join("elsewhere");
    touch(&elsewhere, "go.mod", "");
    symlink(&elsewhere, repo.join("linked")).unwrap();
    touch(&repo, "inner/.git/HEAD", "");
    touch(&repo, "inner/Gemfile", ""); // belongs to the nested repo
    let idx = scan(&[repo.clone()]);
    use PackageKind::*;
    for (name, why) in [("node", "node_modules"), ("rust", "target and depth"), ("go", "symlink"), ("ruby", "nested repo")] {
        assert!(idx.uses(name, Formula).is_empty(), "{name} must not be inferred from {why}: {:?}", idx.uses(name, Formula));
    }
}

#[test]
fn the_full_analysis_joins_packages_and_projects() {
    let (_t, root) = canonical_tmp();
    let web = root.join("web");
    touch(&web, "package.json", "{}");
    let fake = FakeRunner::new().route("outdated", JSON);
    let r = analyze(&fake, Some(BREW), &[web.clone()]);
    assert_eq!(r.repos_scanned, 1);
    let node = r.packages.iter().find(|p| p.outdated.name == "node").unwrap();
    assert_eq!(node.projects.len(), 1);
    assert_eq!(node.projects[0].repo, web);
    assert!(r.packages.iter().find(|p| p.outdated.name == "git").unwrap().projects.is_empty());
    let json = serde_json::to_value(node).unwrap();
    assert_eq!(json["name"], "node");
    assert_eq!(json["risk"], "high");
    assert_eq!(json["projects"][0]["exact"], false);
    let _ = PathBuf::new();
}
