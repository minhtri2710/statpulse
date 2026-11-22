use std::os::unix::fs::MetadataExt;

/// Root ownership means no user console session: the actuator never
/// authorizes UID 0 regardless of where the UID came from.
fn authorized_console_owner(uid: u32) -> Result<u32, String> {
    if uid == 0 {
        Err("no active console user is available".into())
    } else {
        Ok(uid)
    }
}

pub fn active_console_uid() -> Result<u32, String> {
    // Apple documents /dev/console as owned by the active console user.
    // Source: https://developer.apple.com/library/archive/technotes/tn2083/_index.html
    let uid = std::fs::metadata("/dev/console")
        .map_err(|error| format!("cannot determine active console user: {error}"))?
        .uid();
    authorized_console_owner(uid)
}

#[cfg(test)]
mod tests {
    use super::authorized_console_owner;

    #[test]
    fn authorized_console_owner_rejects_root_accepts_user() {
        assert!(authorized_console_owner(0).is_err());
        assert_eq!(authorized_console_owner(501), Ok(501));
    }
}
