//! The engines list against the fake UCI engine: adding, choosing the analysis engine, removing, and what play and
//! analysis get from it.

use std::collections::BTreeMap;

use dameless_core::{
    engines::EngineKind,
    events::{EventKind, Topic},
};
use dameless_hub::{Config, Hub, Next, TopicFilter};
use dameless_store::EngineRecord;
use dameless_work::{EngineSource, Engines, NewEngineRequest};

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
    assert_eq!(o.engines.len(), 10);
    assert!(o.engines.iter().all(|e| e.kind == EngineKind::Builtin && !e.installed));
    assert_eq!(o.engines[5].id, "maia-1500");
    assert_eq!((o.engines[5].nodes, o.engines[5].rating), (Some(1), Some(1500)));
    assert!(o.engines[0].analysis && !o.engines[0].levels);
    let e = engines.ready(None).await.unwrap_err();
    assert_eq!((e.status(), e.message().contains("dameless setup")), (409, true));
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
    let Some(Next::Event(event)) = events.recv().await else { panic!("no event") };
    assert_eq!((event.kind, event.topic.as_str()), (EventKind::EnginesChanged, "engines"));
    assert_eq!(Topic::Engines.to_string(), "engines");

    // The same id again, a built-in's id, a file that is not an engine and a missing one are refused.
    let again = NewEngineRequest { name: "Fake Engine".into(), path: FAKE.into(), ..Default::default() };
    assert_eq!(engines.add(again.clone()).await.unwrap_err().status(), 409);
    let sf = NewEngineRequest { id: Some("stockfish".into()), ..again.clone() };
    assert_eq!(engines.add(sf).await.unwrap_err().status(), 409);
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

    // An engine with a fixed budget cannot analyse.
    let mut maia = EngineRecord::new("maia-1100", "Maia 1100", EngineKind::Builtin, FAKE);
    maia.nodes = Some(1);
    maia.rating = Some(1100);
    let installed = engines.install(maia).await.unwrap();
    assert_eq!(installed.id_name.as_deref(), Some("Fake 1.0"));
    assert_eq!(engines.set_analysis("maia-1100").await.unwrap_err().status(), 400);

    // Removing the analysis engine goes back to Stockfish.
    let o = engines.remove("fake-engine").await.unwrap();
    assert_eq!(o.analysis, "stockfish");
    assert!(!o.engines.iter().any(|e| e.id == "fake-engine"));
    assert_eq!(engines.remove("fake-engine").await.unwrap_err().status(), 404);
}
