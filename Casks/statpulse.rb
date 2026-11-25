cask "statpulse" do
  version "1.5.1"
  sha256 "0c9139d09bf774561df7b3b86c86fab5cc6a0df8cb5de0c7af68688fb1884615"

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
