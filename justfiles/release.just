# FilePath: justfiles/release.just
# Releasing, entirely from this Mac: build, tag, push and GitHub release.

# Bump every version field, build, commit, tag vVERSION, push, and publish the GitHub release.
[group('release')]
release VERSION:
    scripts/release/release.sh {{VERSION}}

# Build the already pushed tag vVERSION and publish it (retry after a failed publish step).
[group('release')]
publish VERSION:
    scripts/release/publish-tag.sh {{VERSION}}

# Manage the release signing certificate: create BACKUP.p12 | restore BACKUP.p12 | show.
[group('release')]
[positional-arguments]
signing-identity *ARGS:
    scripts/release/signing-identity.sh "$@"
