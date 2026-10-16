//! The file names the Rust updater looks for must be the ones `release.yml` publishes
//! (Python had `test_the_build_names_match_the_updater`). A rename on either side would stop
//! every installed copy from finding its update, so the workflow text is checked against
//! `clawd_appupdate::version::asset_name`.

use std::path::PathBuf;

use clawd_appupdate::version::asset_name;

fn workflow() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/release.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The name the workflow builds with the shell variable `$VERSION` in the version's place.
fn in_workflow(updater_name: &str, version: &str) -> String {
    updater_name.replace(version, "$VERSION")
}

#[test]
fn every_updater_asset_is_published_by_the_release_workflow() {
    let text = workflow();
    let version = "9.8.7";
    for (system, arch) in [
        ("mac", "arm64"),
        ("mac", "x86_64"),
        ("windows", "x86_64"),
        ("windows", "arm64"),
    ] {
        let name = asset_name(version, system, arch).expect("an update file for this platform");
        let wanted = in_workflow(&name, version);
        assert!(
            text.contains(&wanted),
            "release.yml never names {wanted} ({system}/{arch})"
        );
    }
}

#[test]
fn the_workflow_ships_a_dmg_beside_each_mac_zip_and_signs_the_sums() {
    let text = workflow();
    for arch in ["arm64", "x86_64"] {
        assert!(
            text.contains(&format!("Clawd-$VERSION-mac-{arch}.dmg")),
            "{arch} dmg"
        );
    }
    assert!(
        text.contains("SHA256SUMS.minisig"),
        "the sums must be signed"
    );
}

#[test]
fn no_update_file_exists_where_there_is_no_self_update() {
    assert_eq!(asset_name("1.2.3", "linux", "x86_64"), None);
    assert_eq!(asset_name("1.2.3", "mac", "ppc"), None);
}
