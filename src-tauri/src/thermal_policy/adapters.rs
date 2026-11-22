use super::state::FanActuation;
use crate::fan_actuation::client;

#[derive(Default)]
pub(crate) struct ProductionFanActuation;

impl FanActuation for ProductionFanActuation {
    fn set_target(&mut self, fan_id: usize, rpm: i32) -> Result<(), String> {
        client::set_target(fan_id, rpm)
    }

    fn system_auto(&mut self, fan_id: usize) -> Result<(), String> {
        client::system_auto(fan_id)
    }

    fn restore_all(&mut self) -> Result<(), String> {
        client::restore_all()
    }

    fn heartbeat(&mut self) -> Result<(), String> {
        client::heartbeat()
    }
}
