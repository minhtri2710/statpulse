cask "statpulse" do
  version "1.5.0"
  sha256 "a7e7774b2dfc8d0e2e9f68b16312c055fbf0d290c8b93ad27c01c0dedb100783"

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
