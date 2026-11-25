use super::protocol::{handle_command, CommandError, FanCommand, FanHardware, FanResponse};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

const RESTORE_RETRY_DELAY: Duration = Duration::from_millis(250);

pub struct ActuationLease {
    timeout: Duration,
    last_heartbeat: Instant,
    manual_fans: BTreeSet<usize>,
    retry_at: Option<Instant>,
}

impl ActuationLease {
    pub fn new(timeout: Duration, now: Instant) -> Self {
        Self {
            timeout,
            last_heartbeat: now,
            manual_fans: BTreeSet::new(),
            retry_at: None,
        }
    }

    pub fn execute(
        &mut self,
        command: FanCommand,
        hardware: &mut impl FanHardware,
        completed_at: impl FnOnce() -> Instant,
    ) -> FanResponse {
        if self.retry_at.is_some()
            && matches!(
                command,
                FanCommand::SetTarget { .. } | FanCommand::Heartbeat | FanCommand::Status
            )
        {
            return FanResponse::Error {
                message: "System Auto recovery is pending".into(),
            };
        }
        let outcome = handle_command(&command, hardware);
        // Retry and expiry windows start when the hardware operation finished,
        // not when the request arrived.
        let now = completed_at();
        match (&command, &outcome) {
            (FanCommand::RestoreAll, Ok(FanResponse::Ok)) => {
                self.retry_at = None;
                self.restored_all(now);
            }
            (FanCommand::RestoreAll, Err(_)) => {
                self.retry_at = Some(now + RESTORE_RETRY_DELAY);
            }
            (_, Err(CommandError::Unconfirmed(_))) => {
                self.retry_at = Some(now + RESTORE_RETRY_DELAY);
            }
            (FanCommand::Heartbeat, Ok(FanResponse::Ok)) => self.heartbeat(now),
            (FanCommand::SetTarget { fan_id, .. }, Ok(FanResponse::Ok)) => {
                self.set_manual(*fan_id, now)
            }
            (FanCommand::SystemAuto { fan_id }, Ok(FanResponse::Ok)) => {
                self.set_system_auto(*fan_id, now)
            }
            _ => (),
        }
        match outcome {
            Ok(response) => response,
            Err(error) => FanResponse::Error {
                message: error.message(),
            },
        }
    }

    pub fn should_restore(&self, now: Instant) -> bool {
        if let Some(retry_at) = self.retry_at {
            return now >= retry_at;
        }
        !self.manual_fans.is_empty()
            && now.saturating_duration_since(self.last_heartbeat) >= self.timeout
    }

    fn heartbeat(&mut self, now: Instant) {
        self.last_heartbeat = now;
    }

    fn set_manual(&mut self, fan_id: usize, now: Instant) {
        self.manual_fans.insert(fan_id);
        self.heartbeat(now);
    }

    fn set_system_auto(&mut self, fan_id: usize, now: Instant) {
        self.manual_fans.remove(&fan_id);
        self.heartbeat(now);
    }

    fn restored_all(&mut self, now: Instant) {
        self.manual_fans.clear();
        self.heartbeat(now);
    }
}

#[cfg(test)]
mod tests {
    use super::ActuationLease;
    use crate::fan_actuation::protocol::{FanCommand, FanEnvelope, FanHardware, FanResponse};
    use std::cell::Cell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct FixtureHardware {
        fans: BTreeMap<usize, (i32, i32)>,
        targets: BTreeMap<usize, i32>,
        write_count: Rc<Cell<usize>>,
        restored: Vec<usize>,
        auto_failures: BTreeSet<usize>,
        target_failures: BTreeSet<usize>,
    }

    impl FixtureHardware {
        fn with_fan(mut self, id: usize, min_rpm: i32, max_rpm: i32) -> Self {
            self.fans.insert(id, (min_rpm, max_rpm));
            self
        }

        fn failing_auto(mut self, id: usize) -> Self {
            self.auto_failures.insert(id);
            self
        }

        fn failing_target(mut self, id: usize) -> Self {
            self.target_failures.insert(id);
            self
        }
    }

    impl FanHardware for FixtureHardware {
        fn envelopes(&self) -> Result<Vec<FanEnvelope>, String> {
            Ok(self
                .fans
                .iter()
                .map(|(id, (min_rpm, max_rpm))| FanEnvelope {
                    id: *id,
                    min_rpm: *min_rpm,
                    max_rpm: *max_rpm,
                })
                .collect())
        }

        fn set_target(&mut self, fan_id: usize, rpm: i32) -> Result<(), String> {
            if self.target_failures.remove(&fan_id) {
                return Err(format!("fan {fan_id} target write failed"));
            }
            self.targets.insert(fan_id, rpm);
            self.write_count.set(self.write_count.get() + 1);
            Ok(())
        }

        fn system_auto(&mut self, fan_id: usize) -> Result<(), String> {
            self.restored.push(fan_id);
            if self.auto_failures.remove(&fan_id) {
                return Err(format!("fan {fan_id} restore failed"));
            }
            Ok(())
        }
    }

    #[test]
    fn expires_manual_control_without_heartbeat() {
        let started = Instant::now();
        let mut lease = ActuationLease::new(Duration::from_secs(5), started);
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);
        lease.execute(
            FanCommand::SetTarget {
                fan_id: 0,
                rpm: 3200,
            },
            &mut hardware,
            || started,
        );

        assert!(lease.should_restore(started + Duration::from_secs(5)));
    }

    #[test]
    fn heartbeat_keeps_manual_control_active() {
        let started = Instant::now();
        let mut lease = ActuationLease::new(Duration::from_secs(5), started);
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);
        lease.execute(
            FanCommand::SetTarget {
                fan_id: 0,
                rpm: 3200,
            },
            &mut hardware,
            || started,
        );
        lease.execute(FanCommand::Heartbeat, &mut hardware, || {
            started + Duration::from_secs(4)
        });

        assert!(!lease.should_restore(started + Duration::from_secs(8)));
    }

    #[test]
    fn system_auto_clears_the_manual_lease() {
        let started = Instant::now();
        let mut lease = ActuationLease::new(Duration::from_secs(5), started);
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);
        lease.execute(
            FanCommand::SetTarget {
                fan_id: 0,
                rpm: 3200,
            },
            &mut hardware,
            || started,
        );
        lease.execute(FanCommand::SystemAuto { fan_id: 0 }, &mut hardware, || {
            started + Duration::from_secs(1)
        });

        assert!(!lease.should_restore(started + Duration::from_secs(10)));
    }

    #[test]
    fn failed_restore_attempts_every_fan_and_retries_until_confirmed() {
        let started = Instant::now();
        let mut lease = ActuationLease::new(Duration::from_secs(5), started);
        let mut hardware = FixtureHardware::default()
            .with_fan(0, 1200, 6000)
            .with_fan(1, 1200, 6000)
            .failing_auto(0);
        lease.execute(
            FanCommand::SetTarget {
                fan_id: 0,
                rpm: 3200,
            },
            &mut hardware,
            || started,
        );

        let response = lease.execute(FanCommand::RestoreAll, &mut hardware, || {
            started + Duration::from_secs(6)
        });

        assert!(matches!(response, FanResponse::Error { .. }));
        assert_eq!(hardware.restored, vec![0, 1]);
        assert!(!lease.should_restore(started + Duration::from_secs(6)));

        let retry = started + Duration::from_secs(6) + Duration::from_millis(250);
        assert!(lease.should_restore(retry));
        assert!(matches!(
            lease.execute(
                FanCommand::SetTarget {
                    fan_id: 1,
                    rpm: 3200,
                },
                &mut hardware,
                || retry,
            ),
            FanResponse::Error { .. }
        ));
        assert!(matches!(
            lease.execute(FanCommand::Heartbeat, &mut hardware, || retry),
            FanResponse::Error { .. }
        ));
        assert!(!hardware.targets.contains_key(&1));

        let response = lease.execute(FanCommand::RestoreAll, &mut hardware, || retry);
        assert_eq!(response, FanResponse::Ok);
        assert_eq!(hardware.restored, vec![0, 1, 0, 1]);
        assert!(!lease.should_restore(retry + Duration::from_secs(60)));
    }

    #[test]
    fn failed_target_arms_recovery_until_restore_confirms() {
        // setFanSpeed unlocks manual control before the RPM write, so a failed
        // write must arm recovery even with no prior manual lease.
        let started = Instant::now();
        let mut lease = ActuationLease::new(Duration::from_secs(5), started);
        let mut hardware = FixtureHardware::default()
            .with_fan(0, 1200, 6000)
            .failing_target(0);

        let response = lease.execute(
            FanCommand::SetTarget {
                fan_id: 0,
                rpm: 3200,
            },
            &mut hardware,
            || started,
        );
        assert!(matches!(response, FanResponse::Error { .. }));
        assert!(lease.should_restore(started + Duration::from_millis(250)));

        assert!(matches!(
            lease.execute(FanCommand::Heartbeat, &mut hardware, || started
                + Duration::from_millis(250),),
            FanResponse::Error { .. }
        ));

        let response = lease.execute(FanCommand::RestoreAll, &mut hardware, || {
            started + Duration::from_millis(250)
        });
        assert_eq!(response, FanResponse::Ok);
        assert_eq!(hardware.restored, vec![0]);

        assert!(matches!(
            lease.execute(FanCommand::Heartbeat, &mut hardware, || started
                + Duration::from_millis(500),),
            FanResponse::Ok
        ));
    }

    #[test]
    fn rejected_target_does_not_arm_recovery() {
        let started = Instant::now();
        let mut lease = ActuationLease::new(Duration::from_secs(5), started);
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);

        let response = lease.execute(
            FanCommand::SetTarget {
                fan_id: 0,
                rpm: 6100,
            },
            &mut hardware,
            || started,
        );
        assert!(matches!(response, FanResponse::Error { .. }));
        assert!(!lease.should_restore(started + Duration::from_secs(60)));
    }

    #[test]
    fn failed_system_auto_and_status_do_not_clear_or_extend_manual_lease() {
        let started = Instant::now();
        let mut lease = ActuationLease::new(Duration::from_secs(5), started);
        let mut hardware = FixtureHardware::default()
            .with_fan(0, 1200, 6000)
            .failing_auto(0);
        lease.execute(
            FanCommand::SetTarget {
                fan_id: 0,
                rpm: 3200,
            },
            &mut hardware,
            || started,
        );

        let response = lease.execute(FanCommand::SystemAuto { fan_id: 0 }, &mut hardware, || {
            started + Duration::from_secs(4)
        });
        assert!(matches!(response, FanResponse::Error { .. }));
        // While restore is pending, Status must not report Ready.
        let response = lease.execute(FanCommand::Status, &mut hardware, || {
            started + Duration::from_secs(4)
        });
        assert!(matches!(response, FanResponse::Error { .. }));

        assert!(lease.should_restore(started + Duration::from_secs(5)));

        // A confirmed restore clears the gate; Status then reports Ready and
        // still never extends the lease.
        let confirmed = started + Duration::from_secs(4) + Duration::from_millis(250);
        let response = lease.execute(FanCommand::RestoreAll, &mut hardware, || confirmed);
        assert_eq!(response, FanResponse::Ok);
        let response = lease.execute(FanCommand::Status, &mut hardware, || {
            started + Duration::from_secs(5)
        });
        assert!(matches!(response, FanResponse::Ready { .. }));
        assert!(!lease.should_restore(started + Duration::from_secs(60)));
    }

    #[test]
    fn lease_expiry_starts_when_the_write_completes() {
        let started = Instant::now();
        let mut lease = ActuationLease::new(Duration::from_secs(5), started);
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);
        let writes = hardware.write_count.clone();

        lease.execute(
            FanCommand::SetTarget {
                fan_id: 0,
                rpm: 3200,
            },
            &mut hardware,
            move || {
                // The clock is sampled only after the hardware write lands.
                assert_eq!(writes.get(), 1);
                started + Duration::from_secs(6)
            },
        );

        assert!(!lease.should_restore(started + Duration::from_secs(10)));
        assert!(lease.should_restore(started + Duration::from_secs(11)));
    }
}
