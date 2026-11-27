cask "statpulse" do
  version "1.6.0"
  sha256 "a05cd21de84094ca90d84dd1a99cd3ffd77083ef0072291a9888986710de374f"

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
