use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Shared sleep signal: the OS callback flips the flag, policy decisions read
/// it inside their own lock. The restore it triggers is supplied by the
/// composition root, so this module never imports policy types.
struct PowerState {
    sleeping: AtomicBool,
}

impl PowerState {
    const fn new() -> Self {
        Self {
            sleeping: AtomicBool::new(false),
        }
    }

    fn is_sleeping(&self) -> bool {
        self.sleeping.load(Ordering::Relaxed)
    }

    fn on_will_sleep(&self, restore: impl FnOnce()) {
        self.sleeping.store(true, Ordering::Relaxed);
        restore();
    }

    fn on_did_wake(&self) {
        self.sleeping.store(false, Ordering::Relaxed);
    }
}

static POWER: PowerState = PowerState::new();
type RestoreCallback = Box<dyn Fn() -> Result<(), String> + Send + Sync>;
static RESTORE_FN: Mutex<Option<RestoreCallback>> = Mutex::new(None);

pub(crate) fn is_sleeping() -> bool {
    POWER.is_sleeping()
}

pub(crate) fn restore_system_auto() {
    if let Ok(guard) = RESTORE_FN.lock() {
        if let Some(ref restore) = *guard {
            let _ = restore();
        }
    }
}

extern "C" fn on_will_sleep() {
    POWER.on_will_sleep(restore_system_auto);
}

extern "C" fn on_did_wake() {
    POWER.on_did_wake();
}

#[cfg(target_os = "macos")]
extern "C" {
    fn statpulse_register_sleep_wake_callbacks(
        will_sleep: extern "C" fn(),
        did_wake: extern "C" fn(),
    );
}

/// The caller owns what "restore" means; this module only wires the OS
/// callback to it.
pub(crate) fn register_power_events(
    restore: impl Fn() -> Result<(), String> + Send + Sync + 'static,
) {
    if let Ok(mut guard) = RESTORE_FN.lock() {
        *guard = Some(Box::new(restore));
    }

    #[cfg(target_os = "macos")]
    {
        unsafe {
            statpulse_register_sleep_wake_callbacks(on_will_sleep, on_did_wake);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn sleep_state_toggles_and_restores() {
        let state = PowerState::new();
        assert!(!state.is_sleeping());

        let restores = Cell::new(0);
        let restore = || {
            assert!(state.is_sleeping());
            restores.set(restores.get() + 1);
        };
        state.on_will_sleep(restore);
        assert!(state.is_sleeping());
        assert_eq!(restores.get(), 1);

        state.on_did_wake();
        assert!(!state.is_sleeping());
    }
}
