//! The engines list against the fake UCI engine: adding, choosing the analysis engine, removing, and what play and
//! analysis get from it.

use std::collections::BTreeMap;

use chess_core::{
    engines::EngineKind,
    events::{EventKind, Topic},
};
use chess_hub::{Config, Hub, Next, TopicFilter};
use chess_store::EngineRecord;
use chess_work::{EngineSource, Engines, NewEngineRequest};

const FAKE: &str = env!("CARGO_BIN_EXE_work-fake-uci");

fn hub() -> (std::sync::Arc<Hub>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::new(dir.path().join("data"), "127.0.0.1:0".parse().unwrap(), 2);
    (Hub::open(config).unwrap(), dir)
}

#[tokio::test]
async fn the_builtins_wait_for_setup() {
    let (hub, _dir) = hub();
    let engines = Engines::new(hub);
    let o = engines.overview().await.unwrap();
    assert_eq!(o.analysis, "stockfish");
    assert_eq!(o.engines.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["stockfish", "reckless"]);
    assert!(o.engines.iter().all(|e| e.kind == EngineKind::Builtin && !e.installed));
    assert!(o.engines.iter().all(|e| e.nodes.is_none() && e.rating.is_none() && e.multipv));
    assert!(o.engines[0].analysis && !o.engines[0].levels, "no levels until it is installed");
    assert!(!o.engines[1].analysis);
    let e = engines.ready(None).await.unwrap_err();
    assert_eq!((e.status(), e.message().contains("chess setup")), (409, true));
    assert_eq!(engines.ready(Some("nope")).await.unwrap_err().status(), 404);
}

#[tokio::test]
async fn engines_are_added_chosen_and_removed() {
    let (hub, _dir) = hub();
    let mut events = hub.bus.subscribe(0, TopicFilter::of(["engines"]));
    let engines = Engines::new(hub.clone());
    let added = engines
        .add(NewEngineRequest {
            name: "Fake Engine".into(),
            path: FAKE.into(),
            options: BTreeMap::from([("Hash".into(), "32".into())]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!((added.id.as_str(), added.id_name.as_deref()), ("fake-engine", Some("Fake 1.0")));
    assert!(added.installed && !added.analysis);
    // Every engine on the clock has levels, the analysis engine or not.
    assert!(added.levels);
    let Some(Next::Event(event)) = events.recv().await else { panic!("no event") };
    assert_eq!((event.kind, event.topic.as_str()), (EventKind::EnginesChanged, "engines"));
    assert_eq!(Topic::Engines.to_string(), "engines");

    // The same id again, a built-in's id, a file that is not an engine and a missing one are refused.
    let again = NewEngineRequest { name: "Fake Engine".into(), path: FAKE.into(), ..Default::default() };
    assert_eq!(engines.add(again.clone()).await.unwrap_err().status(), 409);
    let sf = NewEngineRequest { id: Some("stockfish".into()), ..again.clone() };
    assert_eq!(engines.add(sf).await.unwrap_err().status(), 409);
    let reckless = NewEngineRequest { id: Some("reckless".into()), ..again.clone() };
    assert_eq!(engines.add(reckless).await.unwrap_err().status(), 409, "Reckless is a built-in");
    let missing = NewEngineRequest { name: "Gone".into(), path: "/no/such/engine".into(), ..Default::default() };
    assert_eq!(engines.add(missing).await.unwrap_err().status(), 400);

    // It becomes the analysis engine, with the ladder; play and analysis start it with its options.
    let o = engines.set_analysis("fake-engine").await.unwrap();
    assert_eq!(o.analysis, "fake-engine");
    let fake = o.engines.iter().find(|e| e.id == "fake-engine").unwrap();
    assert!(fake.analysis && fake.levels);
    let ready = engines.ready(None).await.unwrap();
    assert_eq!(ready.engine.cache_key(), "fake-engine|Fake 1.0");
    assert_eq!(ready.options, [("Hash".to_owned(), "32".to_owned())]);
    assert_eq!(ready.path.to_str(), Some(FAKE));

    // A built-in installed by setup is asked what it calls itself.
    let installed =
        engines.install(EngineRecord::new("reckless", "Reckless", EngineKind::Builtin, FAKE)).await.unwrap();
    assert_eq!(installed.id_name.as_deref(), Some("Fake 1.0"));
    assert!(installed.installed && installed.levels);
    assert_eq!(engines.set_analysis("reckless").await.unwrap().analysis, "reckless");
    engines.set_analysis("fake-engine").await.unwrap();

    // An engine with a fixed budget cannot analyse and has no levels.
    let fixed = NewEngineRequest {
        name: "Fixed".into(),
        path: FAKE.into(),
        nodes: Some(1),
        rating: Some(1100),
        ..Default::default()
    };
    let fixed = engines.add(fixed).await.unwrap();
    assert!(!fixed.levels && fixed.multipv);
    // An engine without MultiPV keeps its levels; they play the node budget only.
    let plain = NewEngineRequest {
        name: "Plain".into(),
        path: FAKE.into(),
        args: vec!["--no-multipv".into()],
        ..Default::default()
    };
    let plain = engines.add(plain).await.unwrap();
    assert!(plain.levels && !plain.multipv);
    assert_eq!(engines.set_analysis("fixed").await.unwrap_err().status(), 400);

    // Removing the analysis engine goes back to Stockfish.
    let o = engines.remove("fake-engine").await.unwrap();
    assert_eq!(o.analysis, "stockfish");
    assert!(!o.engines.iter().any(|e| e.id == "fake-engine"));
    assert_eq!(engines.remove("fake-engine").await.unwrap_err().status(), 404);
}
