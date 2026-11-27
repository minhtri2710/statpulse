---
status: accepted
---

# Install the Fan actuation helper with administrator authorization

StatPulse will run a privileged launchd service (`com.statpulse.fan-actuation`) over a narrow Unix domain socket authorized for the active console user (`getpeereid(3)`), validate fan targets against hardware RPM envelopes, and restore all fans to System Auto on communication, lease, or service failure. An administrator-authorized AppleScript invokes the bundled installer with its path as an argument; the root installer validates regular non-symlinked resources, stages root-owned files, stops the existing daemon so it restores System Auto, atomically installs the helper in `/Library/PrivilegedHelperTools` and its plist in `/Library/LaunchDaemons`, and rolls back if launchd bootstrap or startup fails. This removes the Apple signing identity requirement while keeping runtime actuation behind one Unix socket seam, though every install or update requires an administrator password prompt.
