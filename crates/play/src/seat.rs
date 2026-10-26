//! Seats: how each side of a game chooses its moves. The runtime asks the seat whose turn it is for a move and acts on
//! the answer; a seat never touches the record itself.
//!
//! - [`crate::EngineSeat`]: a UCI engine searches (local games).
//! - [`LocalSeat`]: you, at the board: the move comes from the API.
//! - The Lichess seats (`LichessYou`, `LichessBot`, `LichessOpponent`, in the Lichess game runtime): Lichess is the
//!   referee there, and a seat's move counts once Lichess's game stream shows it.

use async_trait::async_trait;
use dameless_core::{
    Error,
    chess::Position,
    game::{SeatAction, SeatMove, Turn},
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use utoipa::ToSchema;

/// What plays a seat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SeatKind {
    Engine,
    Local,
    LichessYou,
    LichessBot,
    LichessOpponent,
}

/// One seat of a game being played.
#[async_trait]
pub trait SeatController: Send {
    /// Asks for this seat's next move in `turn`. Engine seats search; the local seat waits for the board; Lichess
    /// seats wait for the stream (the opponent) or forward ours (you, the bot) and answer once Lichess has it.
    ///
    /// The runtime may drop the future (an action ended the game or took moves back) and ask again; a seat must cope.
    async fn next_move(&mut self, turn: Turn<'_>) -> Result<SeatMove, Error>;

    /// A seat action reaching this seat's side: draw offer, takeback request, resign, abort, claim… Local seats leave
    /// the rules to the Referee; Lichess seats send it to Lichess.
    async fn action(&mut self, action: SeatAction) -> Result<(), Error>;

    fn kind(&self) -> SeatKind;
}

/// A move you made on the board, on its way to your seat, and where to say whether it was taken.
pub(crate) struct MoveRequest {
    pub uci: String,
    pub ack: oneshot::Sender<Result<(), Error>>,
}

/// Where your moves go: the sending half of a [`LocalSeat`] (or of your Lichess seat).
pub(crate) type MoveSender = mpsc::UnboundedSender<MoveRequest>;

/// You, at the board, in a local game: the move comes from the API.
pub struct LocalSeat {
    moves: mpsc::UnboundedReceiver<MoveRequest>,
}

impl LocalSeat {
    pub(crate) fn new() -> (Self, MoveSender) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { moves: rx }, tx)
    }
}

/// Reads a move in UCI or SAN in the position `fen`, as UCI: 400 when it is not legal there.
pub(crate) fn legal_uci(fen: &str, text: &str) -> Result<String, Error> {
    let pos = Position::from_fen(fen)?;
    Ok(pos.to_uci(text)?)
}

#[async_trait]
impl SeatController for LocalSeat {
    async fn next_move(&mut self, turn: Turn<'_>) -> Result<SeatMove, Error> {
        loop {
            let Some(req) = self.moves.recv().await else {
                return Err(Error::conflict("nobody is at the board"));
            };
            match legal_uci(turn.fen, &req.uci) {
                Ok(uci) => {
                    let _ = req.ack.send(Ok(()));
                    return Ok(SeatMove::bare(uci));
                }
                Err(e) => {
                    let _ = req.ack.send(Err(e));
                }
            }
        }
    }

    async fn action(&mut self, _action: SeatAction) -> Result<(), Error> {
        Ok(())
    }

    fn kind(&self) -> SeatKind {
        SeatKind::Local
    }
}
