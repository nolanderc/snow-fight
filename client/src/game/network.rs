use anyhow::Result;

use protocol::EventKind;

impl super::Game {
    pub(super) fn poll_connection(&mut self) -> Result<()> {
        while let Some(event) = self.connection.poll_event()? {
            match event.kind {
                EventKind::Chat(chat) => {
                    println!("{} said: '{}'", chat.player, chat.message);
                }
            }
        }

        Ok(())
    }
}
