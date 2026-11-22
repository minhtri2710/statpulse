cask "statpulse" do
  version "1.3.0"
  sha256 "e97494cf7ba9aabec9b181926e51bd27455ec361b6d9edeef4095991df5a41f8"

  url "https://github.com/minhtri2710/statpulse/releases/download/v#{version}/StatPulse_#{version}_universal.dmg"
  name "StatPulse"
  desc "System, developer, port, AI agent, and thermal fan monitor for macOS"
  homepage "https://github.com/minhtri2710/statpulse"

  livecheck do
    url :url
    strategy :github_latest
  end

  app "StatPulse.app"
  binary "#{appdir}/StatPulse.app/Contents/MacOS/superfan", target: "statpulse"

  uninstall quit: "com.superfan.app"

  zap trash: [
    "~/Library/Application Support/com.superfan.app",
    "~/Library/Caches/com.superfan.app",
    "~/Library/Preferences/com.superfan.app.plist",
  ]
end
