on run argv
	set actionName to item 1 of argv
	set appPath to item 2 of argv
	tell application "System Events"
		set matchesForPath to every login item whose path is appPath
		if actionName is "enable" then
			if (count of matchesForPath) is 0 then make new login item at end with properties {path:appPath, hidden:false}
		else if actionName is "disable" then
			repeat with matchedItem in matchesForPath
				delete matchedItem
			end repeat
		else if actionName is not "status" then
			error "Unknown login item action"
		end if
		return ((count of (every login item whose path is appPath)) > 0)
	end tell
end run
