cask "statpulse" do
  version "1.3.1"
  sha256 "bc35a4db83204691db32390fa0b15d1c490f79be49426aadff8382c491d53a2d"

  url "https://github.com/minhtri2710/statpulse/releases/download/v#{version}/StatPulse_#{version}_universal.dmg"
  name "StatPulse"
  desc "System, developer, port, AI agent, and thermal fan monitor for macOS"
  homepage "https://github.com/minhtri2710/statpulse"

  livecheck do
    url :url
    strategy :github_latest
  end

  app "StatPulse.app"
  binary "#{appdir}/StatPulse.app/Contents/MacOS/statpulse"

  uninstall quit: "com.statpulse.app"

  zap trash: [
    "~/Library/Application Support/com.statpulse.app",
    "~/Library/Caches/com.statpulse.app",
    "~/Library/Preferences/com.statpulse.app.plist",
  ]
end
