//! The one event stream: what happened, on which topic, with a typed payload. The hub numbers and keeps events; this
//! module is their vocabulary. Also the fair-play policy, which the hub computes and every assisting service obeys.

use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    drills::{Drill, DrillId, DrillOutcome},
    error::FAIR_PLAY_REASON,
    game::{GameSummary, Move, Offers, SeatAction},
    ideas::{Idea, Trial},
    ids::{ConnectionKey, GameId, IdeaId, PlayerKey, Side, SidePair, Timestamp, TrialId, WorkId},
    levels::EntrantRating,
    lichess::{ChallengeEvent, ConnectionStatus},
    score::Score,
    stats::Pentanomial,
    training::{Dataset, Net},
    version::{Estimate, Llr, Version},
    work::Work,
};

/// What happened. A closed set: each kind has one payload type, named beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
pub enum EventKind {
    /// [`GameSummary`].
    #[serde(rename = "game.upserted")]
    GameUpserted,
    /// [`GameMoved`].
    #[serde(rename = "game.moved")]
    GameMoved,
    /// [`GameUndone`].
    #[serde(rename = "game.undone")]
    GameUndone,
    /// [`GameChanged`].
    #[serde(rename = "game.changed")]
    GameChanged,
    /// [`GameThinking`], at most five a second, never replayed.
    #[serde(rename = "game.thinking")]
    GameThinking,
    /// [`Idea`].
    #[serde(rename = "idea.upserted")]
    IdeaUpserted,
    /// [`Trial`].
    #[serde(rename = "trial.upserted")]
    TrialUpserted,
    /// [`TrialProgress`], at most once a second.
    #[serde(rename = "trial.progress")]
    TrialProgress,
    /// [`TuneProgress`].
    #[serde(rename = "tune.progress")]
    TuneProgress,
    /// [`Work`].
    #[serde(rename = "work.upserted")]
    WorkUpserted,
    /// [`WorkLog`], never replayed.
    #[serde(rename = "work.log")]
    WorkLog,
    /// [`Version`].
    #[serde(rename = "version.upserted")]
    VersionUpserted,
    /// [`ReviewUpserted`].
    #[serde(rename = "review.upserted")]
    ReviewUpserted,
    /// [`ConnectionStatus`].
    #[serde(rename = "connection.upserted")]
    ConnectionUpserted,
    /// [`ChallengeEvent`].
    #[serde(rename = "challenge")]
    Challenge,
    /// [`Policy`].
    #[serde(rename = "policy.changed")]
    PolicyChanged,
    /// [`EvalInfo`], never replayed.
    #[serde(rename = "eval.info")]
    EvalInfo,
    /// [`Drill`].
    #[serde(rename = "drill.upserted")]
    DrillUpserted,
    /// [`DrillAnswered`].
    #[serde(rename = "drill.answered")]
    DrillAnswered,
    /// [`DrillDeleted`].
    #[serde(rename = "drill.deleted")]
    DrillDeleted,
    /// [`LevelsUpdated`].
    #[serde(rename = "levels.updated")]
    LevelsUpdated,
    /// [`TrainProgress`], at most once a second.
    #[serde(rename = "train.progress")]
    TrainProgress,
    /// [`Dataset`].
    #[serde(rename = "dataset.upserted")]
    DatasetUpserted,
    /// [`Net`].
    #[serde(rename = "net.upserted")]
    NetUpserted,
}

impl EventKind {
    pub const ALL: [Self; 24] = [
        Self::GameUpserted,
        Self::GameMoved,
        Self::GameUndone,
        Self::GameChanged,
        Self::GameThinking,
        Self::IdeaUpserted,
        Self::TrialUpserted,
        Self::TrialProgress,
        Self::TuneProgress,
        Self::WorkUpserted,
        Self::WorkLog,
        Self::VersionUpserted,
        Self::ReviewUpserted,
        Self::ConnectionUpserted,
        Self::Challenge,
        Self::PolicyChanged,
        Self::EvalInfo,
        Self::DrillUpserted,
        Self::DrillAnswered,
        Self::DrillDeleted,
        Self::LevelsUpdated,
        Self::TrainProgress,
        Self::DatasetUpserted,
        Self::NetUpserted,
    ];

    /// The name on the wire: `game.upserted`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GameUpserted => "game.upserted",
            Self::GameMoved => "game.moved",
            Self::GameUndone => "game.undone",
            Self::GameChanged => "game.changed",
            Self::GameThinking => "game.thinking",
            Self::IdeaUpserted => "idea.upserted",
            Self::TrialUpserted => "trial.upserted",
            Self::TrialProgress => "trial.progress",
            Self::TuneProgress => "tune.progress",
            Self::WorkUpserted => "work.upserted",
            Self::WorkLog => "work.log",
            Self::VersionUpserted => "version.upserted",
            Self::ReviewUpserted => "review.upserted",
            Self::ConnectionUpserted => "connection.upserted",
            Self::Challenge => "challenge",
            Self::PolicyChanged => "policy.changed",
            Self::EvalInfo => "eval.info",
            Self::DrillUpserted => "drill.upserted",
            Self::DrillAnswered => "drill.answered",
            Self::DrillDeleted => "drill.deleted",
            Self::LevelsUpdated => "levels.updated",
            Self::TrainProgress => "train.progress",
            Self::DatasetUpserted => "dataset.upserted",
            Self::NetUpserted => "net.upserted",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == name)
    }

    /// Whether the hub keeps it for clients that reconnect. A search's progress and log lines are sent live only:
    /// the next one replaces them, and several a second would push everything else out of the replay.
    pub fn kept_for_replay(self) -> bool {
        !matches!(self, Self::GameThinking | Self::WorkLog | Self::EvalInfo)
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One event as the stream sends it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Event {
    /// Monotonic; pass the last one seen to resume.
    pub id: u64,
    pub topic: String,
    pub kind: EventKind,
    pub data: serde_json::Value,
}

/// What an event is about, which is what clients subscribe to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Topic {
    /// One game in detail: moves, offers, thinking.
    Game(GameId),
    /// Summaries of every game.
    Games,
    Idea(IdeaId),
    Ideas,
    Work,
    Player(PlayerKey),
    Connection(ConnectionKey),
    Versions,
    Policy,
    /// One live evaluation session.
    Eval(String),
    /// Nets and datasets.
    Training,
}

impl Topic {
    /// Whether a client that asked for no topics gets it: everything but evaluation sessions and game detail.
    pub fn in_default(&self) -> bool {
        !matches!(self, Self::Eval(_) | Self::Game(_))
    }

    /// Whether a subscription to `pattern` (a topic, or `game:*`, `eval:*`, `idea:*`, `player:*` for every one of
    /// a kind) covers this topic.
    pub fn matches(&self, pattern: &str) -> bool {
        let text = self.to_string();
        match pattern.strip_suffix('*') {
            Some(prefix) => text.starts_with(prefix),
            None => text == pattern,
        }
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Game(id) => write!(f, "game:{id}"),
            Self::Games => f.write_str("games"),
            Self::Idea(id) => write!(f, "idea:{id}"),
            Self::Ideas => f.write_str("ideas"),
            Self::Work => f.write_str("work"),
            Self::Player(key) => write!(f, "player:{key}"),
            Self::Connection(key) => write!(f, "connection:{key}"),
            Self::Versions => f.write_str("versions"),
            Self::Policy => f.write_str("policy"),
            Self::Eval(session) => write!(f, "eval:{session}"),
            Self::Training => f.write_str("training"),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Payloads
// ---------------------------------------------------------------------------------------------------------------------

/// `game.moved`: a move was played.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GameMoved {
    pub id: GameId,
    /// The ply it was, from 1.
    pub ply: u32,
    #[serde(rename = "move")]
    pub mv: Move,
    pub clocks: Option<SidePair<u64>>,
    /// The position after it.
    pub fen: String,
}

/// `game.undone`: moves were taken back; `plies` is how many are left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GameUndone {
    pub id: GameId,
    pub plies: u32,
}

/// The opponent left a Lichess game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OpponentGone {
    /// Seconds until a claim may be made.
    pub claim_in_seconds: u32,
}

/// `game.changed`: offers, what You may do, the clocks or the opponent's presence changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GameChanged {
    pub id: GameId,
    pub offers: Offers,
    pub actions: Vec<SeatAction>,
    pub clocks: Option<SidePair<u64>>,
    pub opponent_gone: Option<OpponentGone>,
}

/// `game.thinking`: the engine to move's progress. `score` is White's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct GameThinking {
    pub id: GameId,
    /// The ply being chosen, so a late report for a move already played can be dropped.
    pub ply: u32,
    pub side: Side,
    pub depth: u32,
    pub score: Option<Score>,
    pub nodes: u64,
    pub nps: u64,
    pub pv: Vec<String>,
}

/// `trial.progress`: a trial's running totals.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrialProgress {
    pub id: TrialId,
    /// The trial's idea, which is the topic it is published on.
    pub idea: IdeaId,
    pub penta: Pentanomial,
    pub wdl: [u32; 3],
    pub llr: Option<Llr>,
    pub elo: Option<Estimate>,
}

/// `tune.progress`: an SPSA update was applied.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TuneProgress {
    pub idea: IdeaId,
    /// Counted from 1, in the order updates were applied.
    pub iteration: u32,
    /// Every parameter after it, in the tune's order.
    pub values: Vec<f64>,
    /// θ+'s games won minus games lost.
    pub result: i64,
}

/// `work.log`: one line of a work item's log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct WorkLog {
    pub id: WorkId,
    pub line: String,
}

/// `review.upserted`: a game was reviewed; fetch the review for detail.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ReviewUpserted {
    pub game: GameId,
    pub accuracy: SidePair<Option<f32>>,
}

/// One line of a live evaluation. `score` is White's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EvalLine {
    pub score: Score,
    pub pv: Vec<String>,
}

/// `eval.info`: a live evaluation session's progress.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct EvalInfo {
    pub session: String,
    pub depth: u32,
    pub lines: Vec<EvalLine>,
    pub nodes: u64,
    pub nps: u64,
}

/// `drill.answered`: a drill was answered; the rescheduled drill follows as `drill.upserted`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct DrillAnswered {
    pub drill: DrillId,
    pub outcome: DrillOutcome,
    pub at: Timestamp,
}

/// `drill.deleted`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DrillDeleted {
    pub id: DrillId,
}

/// `levels.updated`: the levels were refitted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct LevelsUpdated {
    pub ratings: Vec<EntrantRating>,
    pub updated_at: Timestamp,
}

/// `train.progress`: a training run's latest reading (throttled to one a second). Published on `idea:<id>` when the
/// run is a net idea's, else on `work`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct TrainProgress {
    pub work: WorkId,
    pub idea: Option<IdeaId>,
    pub superbatch: u32,
    pub superbatches: u32,
    pub batch: u32,
    pub batches_per_superbatch: u32,
    pub loss: f64,
    pub lr: f64,
    pub positions_per_sec: f64,
    pub grad_norm: Option<f64>,
    pub elapsed_seconds: f64,
}

/// Every payload, typed, for building events without mixing kinds and payloads up.
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    GameUpserted(Box<GameSummary>),
    GameMoved(GameMoved),
    GameUndone(GameUndone),
    GameChanged(GameChanged),
    GameThinking(GameThinking),
    IdeaUpserted(Box<Idea>),
    TrialUpserted(Box<Trial>),
    TrialProgress(TrialProgress),
    TuneProgress(TuneProgress),
    WorkUpserted(Box<Work>),
    WorkLog(WorkLog),
    VersionUpserted(Box<Version>),
    ReviewUpserted(ReviewUpserted),
    ConnectionUpserted(Box<ConnectionStatus>),
    Challenge(Box<ChallengeEvent>),
    PolicyChanged(Policy),
    EvalInfo(EvalInfo),
    DrillUpserted(Box<Drill>),
    DrillAnswered(DrillAnswered),
    DrillDeleted(DrillDeleted),
    LevelsUpdated(LevelsUpdated),
    TrainProgress(TrainProgress),
    DatasetUpserted(Box<Dataset>),
    NetUpserted(Box<Net>),
}

impl Payload {
    pub fn kind(&self) -> EventKind {
        match self {
            Self::GameUpserted(_) => EventKind::GameUpserted,
            Self::GameMoved(_) => EventKind::GameMoved,
            Self::GameUndone(_) => EventKind::GameUndone,
            Self::GameChanged(_) => EventKind::GameChanged,
            Self::GameThinking(_) => EventKind::GameThinking,
            Self::IdeaUpserted(_) => EventKind::IdeaUpserted,
            Self::TrialUpserted(_) => EventKind::TrialUpserted,
            Self::TrialProgress(_) => EventKind::TrialProgress,
            Self::TuneProgress(_) => EventKind::TuneProgress,
            Self::WorkUpserted(_) => EventKind::WorkUpserted,
            Self::WorkLog(_) => EventKind::WorkLog,
            Self::VersionUpserted(_) => EventKind::VersionUpserted,
            Self::ReviewUpserted(_) => EventKind::ReviewUpserted,
            Self::ConnectionUpserted(_) => EventKind::ConnectionUpserted,
            Self::Challenge(_) => EventKind::Challenge,
            Self::PolicyChanged(_) => EventKind::PolicyChanged,
            Self::EvalInfo(_) => EventKind::EvalInfo,
            Self::DrillUpserted(_) => EventKind::DrillUpserted,
            Self::DrillAnswered(_) => EventKind::DrillAnswered,
            Self::DrillDeleted(_) => EventKind::DrillDeleted,
            Self::LevelsUpdated(_) => EventKind::LevelsUpdated,
            Self::TrainProgress(_) => EventKind::TrainProgress,
            Self::DatasetUpserted(_) => EventKind::DatasetUpserted,
            Self::NetUpserted(_) => EventKind::NetUpserted,
        }
    }

    /// The topic it is published on.
    pub fn topic(&self) -> Topic {
        match self {
            Self::GameUpserted(_) => Topic::Games,
            Self::GameMoved(p) => Topic::Game(p.id),
            Self::GameUndone(p) => Topic::Game(p.id),
            Self::GameChanged(p) => Topic::Game(p.id),
            Self::GameThinking(p) => Topic::Game(p.id),
            Self::IdeaUpserted(_) => Topic::Ideas,
            Self::TrialUpserted(t) => Topic::Idea(t.idea),
            Self::TrialProgress(p) => Topic::Idea(p.idea),
            Self::TuneProgress(p) => Topic::Idea(p.idea),
            Self::WorkUpserted(_) | Self::WorkLog(_) => Topic::Work,
            Self::VersionUpserted(_) => Topic::Versions,
            Self::ReviewUpserted(_) => Topic::Games,
            Self::ConnectionUpserted(p) => Topic::Connection(p.key),
            Self::Challenge(p) => Topic::Connection(p.challenge.connection),
            Self::PolicyChanged(_) => Topic::Policy,
            Self::EvalInfo(p) => Topic::Eval(p.session.clone()),
            Self::DrillUpserted(_) | Self::DrillAnswered(_) | Self::DrillDeleted(_) => Topic::Player(PlayerKey::You),
            Self::LevelsUpdated(_) => Topic::Player(PlayerKey::Dameless),
            Self::TrainProgress(p) => p.idea.map_or(Topic::Work, Topic::Idea),
            Self::DatasetUpserted(_) | Self::NetUpserted(_) => Topic::Training,
        }
    }

    /// The payload as JSON.
    pub fn data(&self) -> serde_json::Value {
        let v = match self {
            Self::GameUpserted(p) => serde_json::to_value(p),
            Self::GameMoved(p) => serde_json::to_value(p),
            Self::GameUndone(p) => serde_json::to_value(p),
            Self::GameChanged(p) => serde_json::to_value(p),
            Self::GameThinking(p) => serde_json::to_value(p),
            Self::IdeaUpserted(p) => serde_json::to_value(p),
            Self::TrialUpserted(p) => serde_json::to_value(p),
            Self::TrialProgress(p) => serde_json::to_value(p),
            Self::TuneProgress(p) => serde_json::to_value(p),
            Self::WorkUpserted(p) => serde_json::to_value(p),
            Self::WorkLog(p) => serde_json::to_value(p),
            Self::VersionUpserted(p) => serde_json::to_value(p),
            Self::ReviewUpserted(p) => serde_json::to_value(p),
            Self::ConnectionUpserted(p) => serde_json::to_value(p),
            Self::Challenge(p) => serde_json::to_value(p),
            Self::PolicyChanged(p) => serde_json::to_value(p),
            Self::EvalInfo(p) => serde_json::to_value(p),
            Self::DrillUpserted(p) => serde_json::to_value(p),
            Self::DrillAnswered(p) => serde_json::to_value(p),
            Self::DrillDeleted(p) => serde_json::to_value(p),
            Self::LevelsUpdated(p) => serde_json::to_value(p),
            Self::TrainProgress(p) => serde_json::to_value(p),
            Self::DatasetUpserted(p) => serde_json::to_value(p),
            Self::NetUpserted(p) => serde_json::to_value(p),
        };
        // Every payload is plain data with string keys and finite numbers, which always serializes.
        v.unwrap_or(serde_json::Value::Null)
    }

    /// The event, numbered `id` by the hub.
    pub fn event(&self, id: u64) -> Event {
        Event { id, topic: self.topic().to_string(), kind: self.kind(), data: self.data() }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Fair play
// ---------------------------------------------------------------------------------------------------------------------

/// How much assistance is allowed now. Lichess forbids engines, books and tablebases during a game; its explorer is
/// allowed in correspondence only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Assist {
    #[default]
    Allowed,
    /// Every live game of yours is correspondence: the explorer only.
    ExplorerOnly,
    Off,
}

/// A kind of assistance a service gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssistKind {
    /// Live engine evaluation.
    Eval,
    Explorer,
    Tablebase,
    /// New reviews, and running ones before their next position.
    Review,
    /// Importing another player's games.
    PlayerImport,
}

/// The fair-play policy, computed from the you connection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Policy {
    pub assist: Assist,
    /// Why, while not `allowed`: "Analysis is off while you're playing on Lichess".
    pub reason: Option<String>,
}

impl Policy {
    /// The policy while You have these live Lichess games, each `true` when it is correspondence (played here or on
    /// lichess.org): none is `allowed`, all correspondence `explorer_only`, else `off`.
    pub fn from_live_games(correspondence: &[bool]) -> Self {
        if correspondence.is_empty() {
            return Self::default();
        }
        let assist = if correspondence.iter().all(|c| *c) { Assist::ExplorerOnly } else { Assist::Off };
        Self { assist, reason: Some(FAIR_PLAY_REASON.into()) }
    }

    /// Whether this kind of assistance may be given now.
    pub fn allows(&self, kind: AssistKind) -> bool {
        match self.assist {
            Assist::Allowed => true,
            Assist::ExplorerOnly => kind == AssistKind::Explorer,
            Assist::Off => false,
        }
    }

    /// `Ok` when allowed, else the fair-play error every service answers with.
    pub fn check(&self, kind: AssistKind) -> crate::error::Result<()> {
        if self.allows(kind) { Ok(()) } else { Err(crate::error::Error::fair_play()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_have_wire_names_and_replay_rules() {
        for k in EventKind::ALL {
            assert_eq!(serde_json::to_value(k).unwrap(), k.as_str());
            assert_eq!(EventKind::parse(k.as_str()), Some(k));
        }
        assert!(!EventKind::GameThinking.kept_for_replay());
        assert!(!EventKind::EvalInfo.kept_for_replay());
        assert!(EventKind::GameMoved.kept_for_replay());
    }

    #[test]
    fn topics_read_and_match() {
        assert_eq!(Topic::Game(GameId(4)).to_string(), "game:4");
        assert_eq!(Topic::Player(PlayerKey::Lichess("bob".into())).to_string(), "player:lichess:bob");
        assert_eq!(Topic::Connection(ConnectionKey::Bot).to_string(), "connection:bot");
        assert!(Topic::Game(GameId(4)).matches("game:*"));
        assert!(Topic::Game(GameId(4)).matches("game:4"));
        assert!(!Topic::Game(GameId(4)).matches("game:5"));
        assert!(!Topic::Eval("s".into()).in_default());
        assert!(Topic::Games.in_default());
    }

    #[test]
    fn payloads_name_their_kind_and_topic() {
        let p = Payload::GameUndone(GameUndone { id: GameId(3), plies: 10 });
        let e = p.event(7);
        assert_eq!((e.id, e.kind, e.topic.as_str()), (7, EventKind::GameUndone, "game:3"));
        assert_eq!(e.data, serde_json::json!({ "id": 3, "plies": 10 }));
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "game.undone");
    }

    #[test]
    fn the_policy_follows_your_live_games() {
        assert!(Policy::from_live_games(&[]).allows(AssistKind::Eval));
        let corr = Policy::from_live_games(&[true, true]);
        assert_eq!(corr.assist, Assist::ExplorerOnly);
        assert!(corr.allows(AssistKind::Explorer) && !corr.allows(AssistKind::Tablebase));
        let off = Policy::from_live_games(&[true, false]);
        assert_eq!(off.assist, Assist::Off);
        assert_eq!(off.check(AssistKind::Explorer).unwrap_err().status(), 409);
        assert_eq!(off.reason.as_deref(), Some(FAIR_PLAY_REASON));
    }
}
