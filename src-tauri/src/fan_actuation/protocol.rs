use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum FanCommand {
    Status,
    Heartbeat,
    SetTarget { fan_id: usize, rpm: i32 },
    SystemAuto { fan_id: usize },
    RestoreAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanEnvelope {
    pub id: usize,
    pub min_rpm: i32,
    pub max_rpm: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FanResponse {
    Ok,
    Ready { fans: Vec<FanEnvelope> },
    Error { message: String },
}

// Rejected means input validation failed before any actuation was attempted;
// Unconfirmed means hardware work was started but its outcome failed, so a
// fan may have been left under manual control and recovery must be armed.
#[derive(Debug)]
pub enum CommandError {
    Rejected(String),
    Unconfirmed(String),
}

impl CommandError {
    pub(crate) fn message(self) -> String {
        match self {
            CommandError::Rejected(message) | CommandError::Unconfirmed(message) => message,
        }
    }
}

pub trait FanHardware {
    fn envelopes(&self) -> Result<Vec<FanEnvelope>, String>;
    // Fan IDs used by RestoreAll. RPM envelopes are intentionally not required:
    // a fan reporting invalid readings must still be restored to system auto.
    fn fan_ids(&self) -> Result<Vec<usize>, String> {
        Ok(self.envelopes()?.into_iter().map(|fan| fan.id).collect())
    }
    fn set_target(&mut self, fan_id: usize, rpm: i32) -> Result<(), String>;
    fn system_auto(&mut self, fan_id: usize) -> Result<(), String>;
    // Restore every fan in one pass, retaining the first failure. The default
    // services fans individually; hardware adapters may override with a single
    // bounded firmware round-trip.
    fn restore_all(&mut self) -> Result<(), String> {
        let mut failed = None;
        for fan_id in self.fan_ids()? {
            if let Err(error) = self.system_auto(fan_id) {
                failed.get_or_insert(error);
            }
        }
        match failed {
            None => Ok(()),
            Some(message) => Err(message),
        }
    }
}

pub fn handle_command(
    command: &FanCommand,
    hardware: &mut impl FanHardware,
) -> Result<FanResponse, CommandError> {
    match command {
        FanCommand::Status => hardware
            .envelopes()
            .map(|fans| FanResponse::Ready { fans })
            .map_err(CommandError::Rejected),
        FanCommand::Heartbeat => Ok(FanResponse::Ok),
        FanCommand::SetTarget { fan_id, rpm } => {
            let fans = hardware.envelopes().map_err(CommandError::Rejected)?;
            let Some(fan) = fans.iter().find(|fan| fan.id == *fan_id) else {
                return Err(CommandError::Rejected(format!(
                    "fan {fan_id} is unavailable"
                )));
            };
            if *rpm < fan.min_rpm || *rpm > fan.max_rpm {
                return Err(CommandError::Rejected(format!(
                    "target {rpm} RPM is outside fan {fan_id} range {}-{} RPM",
                    fan.min_rpm, fan.max_rpm
                )));
            }
            hardware
                .set_target(*fan_id, *rpm)
                .map_err(CommandError::Unconfirmed)?;
            Ok(FanResponse::Ok)
        }
        FanCommand::SystemAuto { fan_id } => {
            let fans = hardware.fan_ids().map_err(CommandError::Rejected)?;
            if !fans.iter().any(|fan| fan == fan_id) {
                return Err(CommandError::Rejected(format!(
                    "fan {fan_id} is unavailable"
                )));
            }
            hardware
                .system_auto(*fan_id)
                .map_err(CommandError::Unconfirmed)?;
            Ok(FanResponse::Ok)
        }
        FanCommand::RestoreAll => hardware
            .restore_all()
            .map(|()| FanResponse::Ok)
            .map_err(CommandError::Unconfirmed),
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_command, CommandError, FanCommand, FanEnvelope, FanHardware, FanResponse};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct FixtureHardware {
        fans: BTreeMap<usize, (i32, i32)>,
        targets: BTreeMap<usize, i32>,
        restored: Vec<usize>,
    }

    impl FixtureHardware {
        fn with_fan(mut self, id: usize, min_rpm: i32, max_rpm: i32) -> Self {
            self.fans.insert(id, (min_rpm, max_rpm));
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
            self.targets.insert(fan_id, rpm);
            Ok(())
        }

        fn system_auto(&mut self, fan_id: usize) -> Result<(), String> {
            self.restored.push(fan_id);
            Ok(())
        }
    }

    #[test]
    fn rejects_target_outside_hardware_envelope() {
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);

        let error = handle_command(
            &FanCommand::SetTarget {
                fan_id: 0,
                rpm: 6100,
            },
            &mut hardware,
        )
        .unwrap_err();

        assert!(matches!(error, CommandError::Rejected(_)));
        assert!(hardware.targets.is_empty());
    }

    #[test]
    fn applies_valid_target() {
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);

        let response = handle_command(
            &FanCommand::SetTarget {
                fan_id: 0,
                rpm: 3200,
            },
            &mut hardware,
        )
        .unwrap();

        assert_eq!(response, FanResponse::Ok);
        assert_eq!(hardware.targets.get(&0), Some(&3200));
    }

    #[test]
    fn rejects_system_auto_for_unknown_fan() {
        let mut hardware = FixtureHardware::default().with_fan(0, 1200, 6000);

        let error =
            handle_command(&FanCommand::SystemAuto { fan_id: 99 }, &mut hardware).unwrap_err();

        assert!(matches!(error, CommandError::Rejected(_)));
        assert!(hardware.restored.is_empty());
    }

    #[test]
    fn restore_all_returns_every_fan_to_system_auto() {
        let mut hardware = FixtureHardware::default()
            .with_fan(0, 1200, 6000)
            .with_fan(1, 1200, 6000);

        let response = handle_command(&FanCommand::RestoreAll, &mut hardware).unwrap();

        assert_eq!(response, FanResponse::Ok);
        assert_eq!(hardware.restored, vec![0, 1]);
    }
}
