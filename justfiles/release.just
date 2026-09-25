# FilePath: justfiles/release.just
# Cutting a release. Pushing the tag starts .github/workflows/release.yml, which builds the DMG,
# publishes the GitHub release and updates the Homebrew cask in yuyudhan/homebrew-tap.

# Bump package.json, tauri.conf.json and Cargo.toml to VERSION, commit, and tag vVERSION.
[group('release')]
release VERSION:
    scripts/release/bump-version.sh {{VERSION}}
