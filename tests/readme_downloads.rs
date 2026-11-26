//! The README's download buttons and table link straight to one release's files, so a version bump
//! that forgets them leaves people downloading an old build. These links must name this version.

const RELEASES: &str = "https://github.com/opopina/fashioncraft/releases/download/";

#[test]
fn readme_downloads_name_this_version() {
    let readme = include_str!("../README.md");
    let version = env!("CARGO_PKG_VERSION");
    let base = format!("{RELEASES}v{version}/fashioncraft-{version}-");
    for file in [
        "windows-x64.msi",
        "windows-x64-portable.zip",
        "macos-universal.dmg",
        "linux-x86_64.AppImage",
        "linux-x86_64.tar.gz",
    ] {
        assert!(
            readme.contains(&format!("{base}{file}")),
            "README.md has no download link to {base}{file}"
        );
    }
    let current = format!("{RELEASES}v{version}/");
    for (at, _) in readme.match_indices(RELEASES) {
        assert!(
            readme[at..].starts_with(&current),
            "README.md links to another version's files: {}",
            readme[at..].lines().next().unwrap_or_default()
        );
    }
    assert!(
        readme.contains(&format!("Version {version} ")),
        "README.md's line under the download buttons does not say version {version}"
    );
}
