# FilePath: packaging/homebrew/Casks/simple-voice.rb
# Template for the cask published to yuyudhan/homebrew-tap. scripts/release/update-cask.sh
# renders it for each release: it pins `version` and the real `sha256` of the uploaded DMG, and
# drops every block between unsigned-build markers when the release was Developer ID signed and
# notarized. Until a release renders it, the checksum is not pinned. Indented with two spaces,
# as `brew style` requires; `just cask-check` runs Homebrew's style and audit checks on it.
cask "simple-voice" do
  version "0.1.0"
  sha256 :no_check

  url "https://github.com/yuyudhan/simple-voice/releases/download/v#{version}/Simple-Voice_#{version}_aarch64.dmg"
  name "Simple Voice"
  desc "System-wide dictation: speak, and polished text lands in the focused app"
  homepage "https://github.com/yuyudhan/simple-voice"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on arch: :arm64
  depends_on macos: :sonoma

  app "Simple Voice.app"

  # unsigned-build:begin
  # Ad-hoc signed builds are not notarized, so Gatekeeper would refuse to open the app after a
  # download. Clearing the quarantine flag is the same as approving it once in System Settings.
  postflight_steps do
    run "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "{{appdir}}/Simple Voice.app"]
  end
  # unsigned-build:end

  uninstall quit: "dev.yuyudhan.simplevoice"

  zap trash: [
    "~/.simplevoice",
    "~/Library/Application Support/dev.yuyudhan.simplevoice",
    "~/Library/Caches/dev.yuyudhan.simplevoice",
    "~/Library/LaunchAgents/Simple Voice.plist",
    "~/Library/Preferences/dev.yuyudhan.simplevoice.plist",
    "~/Library/WebKit/dev.yuyudhan.simplevoice",
  ]

  # unsigned-build:begin
  caveats <<~EOS
    This build of Simple Voice is ad-hoc signed, not notarized by Apple. Homebrew cleared the
    download quarantine so it opens normally.

    macOS ties the Microphone, Accessibility and Speech Recognition permissions to the app's
    signature. After an upgrade, if dictation stops pasting or recording, open
    System Settings > Privacy & Security, remove Simple Voice from the affected list, and grant
    it again from the app's Settings > Permissions.
  EOS
  # unsigned-build:end
end
