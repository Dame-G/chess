//! Pure domain: types, chess, statistics, review rules, referee rules, SPSA and scaling maths, events. No I/O: no
//! tokio, no filesystem, no network, no clock reads. Timestamps, durations and randomness are passed in, so every
//! rule here is a plain function and is unit tested.
//!
//! `docs/design.md` is the contract for every type here.

pub mod chess;
pub mod drills;
pub mod error;
pub mod events;
pub mod game;
pub mod harness;
pub mod ideas;
pub mod ids;
pub mod levels;
pub mod lichess;
pub mod prep;
pub mod profile;
pub mod progress;
pub mod referee;
pub mod regression;
pub mod repertoire;
pub mod review;
pub mod scaling;
pub mod score;
pub mod spsa;
pub mod stats;
pub mod themes;
pub mod time;
pub mod training;
pub mod upstream;
pub mod version;
pub mod work;

pub use error::{Error, ErrorCode, Result};
pub use ids::{
    BuildId, Color, ConnectionKey, GameId, IdeaId, PlayerKey, ReviewId, Side, SidePair, Timestamp, TrialId, VersionId,
    WorkId,
};
pub use score::Score;
pub use time::{Speed, TimeControl};

#[cfg(test)]
mod schema_tests {
    use utoipa::OpenApi;

    use crate::*;

    /// Every type the API carries, so a schema that does not build fails here rather than in the server.
    #[derive(OpenApi)]
    #[openapi(components(schemas(
        game::Game,
        game::GameSummary,
        game::GameEnd,
        ideas::Idea,
        ideas::Trial,
        ideas::StagePlan,
        version::Version,
        version::Build,
        work::Work,
        events::Event,
        events::GameMoved,
        events::GameChanged,
        events::GameThinking,
        events::TrialProgress,
        events::TuneProgress,
        events::ReviewUpserted,
        events::EvalInfo,
        events::Policy,
        error::ErrorBody,
        lichess::ConnectionStatus,
        lichess::ChallengeEvent,
        lichess::seek::OnlineBot,
        review::Review,
        profile::Comparison,
        spsa::TuneSpec,
        spsa::TuneStep,
        scaling::ScalingSettings,
        stats::EloEstimate,
        levels::Level,
        levels::EntrantRating,
        levels::PlannedMatch,
        drills::Drill,
        drills::DrillAnswer,
        drills::HitRatePoint,
        repertoire::Repertoire,
        progress::Progress,
        training::Net,
        training::Dataset,
        training::TrainSpec,
        training::TrainStep,
        events::TrainProgress,
    )))]
    struct Doc;

    #[test]
    fn every_api_type_has_a_schema() {
        let json = Doc::openapi().to_pretty_json().unwrap();
        for name in ["GameSummary", "PlayerKey", "Score", "TimeControl", "Comparison", "ConnectionStatus"] {
            assert!(json.contains(&format!("\"{name}\"")), "{name} is missing");
        }
    }
}
