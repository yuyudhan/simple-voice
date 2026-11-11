# FilePath: justfiles/release.just
# Releasing: the Mac builds, tags, pushes and publishes; a Linux machine then builds the tag's
# .deb, which the Mac adds to the same GitHub release (docs/internal/releasing.md).

# Bump every version field, build, commit, tag vVERSION, push, and publish the GitHub release.
[group('release')]
release VERSION:
    scripts/release/release.sh {{VERSION}}

# Build the already pushed tag vVERSION and publish it (retry after a failed publish step).
[group('release')]
publish VERSION:
    scripts/release/publish-tag.sh {{VERSION}}

# On x86-64 Linux: build and check the .deb of the pushed, checked-out tag vVERSION.
[group('release')]
build-linux VERSION:
    scripts/release/build-linux.sh {{VERSION}}

# On the publishing Mac: add the Linux .deb copied into target/release-assets/VERSION/.
[group('release')]
publish-linux VERSION:
    scripts/release/publish-linux.sh {{VERSION}}

# Rewrite and upload the SHA256SUMS of release vVERSION (after uploading a changed installer).
[group('release')]
checksums VERSION:
    scripts/release/checksums.sh {{VERSION}}

# Manage the release signing certificate: create BACKUP.p12 | restore BACKUP.p12 | show.
[group('release')]
[positional-arguments]
signing-identity *ARGS:
    scripts/release/signing-identity.sh "$@"
