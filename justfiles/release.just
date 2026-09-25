# FilePath: justfiles/release.just
# Releasing, entirely from this Mac: build, tag, push, GitHub release and Homebrew cask.

# Bump every version field, build, commit, tag vVERSION, push, and publish the cask.
[group('release')]
release VERSION:
    scripts/release/release.sh {{VERSION}}

# Build the already pushed tag vVERSION and publish it (retry after a failed publish step).
[group('release')]
publish VERSION:
    scripts/release/publish-tag.sh {{VERSION}}

# Render the Homebrew cask (ad-hoc and notarized variants) and run brew style and brew audit.
[group('release')]
cask-check:
    scripts/release/check-cask.sh
