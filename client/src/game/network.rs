use anyhow::Result;
use logic::snapshot::RestoreConfig;
use protocol::{EventKind, GameOver};

impl super::Game {
    pub(super) fn poll_connection(&mut self) -> Result<Option<GameOver>> {
        while let Some(event) = self.connection.poll_event()? {
            match event.kind {
                EventKind::Snapshot(snapshot) => {
                    let config = RestoreConfig {
                        active_player: Some(self.player.entity),
                    };
                    self.snapshots
                        .restore_snapshot(&mut self.world, &snapshot, &config);
                }
                EventKind::GameOver(game_over) => {
                    return Ok(Some(game_over));
                }
            }
        }

        Ok(None)
    }
}
