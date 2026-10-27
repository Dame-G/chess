mod common;

use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::Path,
};

use common::random_games;
use data::{
    convert::{ConvertOptions, convert},
    filter::{GameTransform, PositionFilter},
    formats::{Format, GameReader, GameWriter, bullet, pgn::ScoreUnits, rbinpack, viri},
    game::{self, Game},
    shuffle, stats,
};
use shakmaty::{Color, Position};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/sample.vf");

fn write_all(path: &Path, format: Format, games: &[Game]) {
    let mut w = GameWriter::create(path, format, PositionFilter::NONE).unwrap();
    for g in games {
        w.write_game(g).unwrap();
    }
    w.finish().unwrap();
}

fn read_all(path: &Path, format: Format) -> Vec<Game> {
    GameReader::open(path, format, ScoreUnits::Cp).unwrap().map(|g| g.unwrap()).collect()
}

#[test]
fn viriformat_round_trip() {
    let games = random_games(60, 1, 200, false);
    for g in &games {
        g.validate().unwrap();
        let bytes = viri::encode_game(g);
        assert_eq!(bytes.len(), 32 + 4 * (g.len() + 1));
        assert_eq!(&viri::decode_game(&bytes).unwrap(), g);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("games.vf");
    write_all(&path, Format::Viriformat, &games);
    assert_eq!(read_all(&path, Format::Viriformat), games);
}

/// Viridithas's own reader (what bullet uses) reads our games, checks every move is legal (in debug builds) and
/// sees the same boards and White-relative evals.
#[test]
fn viriformat_matches_viridithas() {
    let games = random_games(40, 2, 150, false);
    for g in &games {
        let bytes = viri::encode_game(g);
        let theirs = viriformat::dataformat::Game::deserialise_from(&mut &bytes[..], Vec::new()).unwrap();
        assert_eq!(theirs.len(), g.len());
        let mut ours = Vec::new();
        g.for_each_position(|pos, e, _| {
            let white = if pos.turn() == Color::White { e.score } else { -e.score };
            let fen = game::fen(pos);
            let mut fields = fen.split(' ');
            ours.push((fields.next().unwrap().to_string(), fields.next().unwrap().to_string(), i32::from(white)));
        });
        let mut i = 0;
        theirs.visit_positions(|board, eval| {
            let fen = board.to_string();
            let mut fields = fen.split(' ');
            let placement = fields.next().unwrap().to_string();
            let stm = fields.next().unwrap().to_string();
            assert_eq!((placement, stm, eval), ours[i], "position {i}");
            i += 1;
        });
        let wdl = match theirs.outcome() {
            viriformat::dataformat::WDL::Win => 2,
            viriformat::dataformat::WDL::Draw => 1,
            viriformat::dataformat::WDL::Loss => 0,
        };
        assert_eq!(wdl, g.result.as_u8());
    }
}

#[test]
fn rbinpack_round_trip() {
    let games = random_games(60, 3, 200, false);
    for g in &games {
        let bytes = rbinpack::encode_game(g);
        assert_eq!(bytes.len(), 4 + 72 + 4 * g.len());
        assert_eq!(&rbinpack::decode_game(&bytes).unwrap(), g);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("games.rbinpack");
    write_all(&path, Format::Rbinpack, &games);
    assert_eq!(read_all(&path, Format::Rbinpack), games);
}

#[test]
fn rbinpack_moves_are_reckless_moves() {
    // White castles king side: Reckless writes king e1 to g1 with kind 2.
    let pos = game::parse_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1").unwrap();
    let mv = game::parse_uci(&pos, "e1g1").unwrap();
    assert_eq!(rbinpack::encode_move(mv), 4 | (6 << 6) | (2 << 12));
    let pos = game::parse_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1").unwrap();
    let mv = game::parse_uci(&pos, "e2e4").unwrap();
    assert_eq!(rbinpack::encode_move(mv), 12 | (28 << 6) | (1 << 12));
    let pos = game::parse_fen("1n6/P7/8/8/8/8/8/k6K w - - 0 1").unwrap();
    let mv = game::parse_uci(&pos, "a7b8q").unwrap();
    assert_eq!(rbinpack::encode_move(mv), 48 | (57 << 6) | (15 << 12));
}

#[test]
fn binpack_round_trip() {
    let games = random_games(40, 4, 200, true);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("games.binpack");
    write_all(&path, Format::Binpack, &games);
    let back = read_all(&path, Format::Binpack);
    assert_eq!(back.len(), games.len());
    for (a, b) in games.iter().zip(&back) {
        assert_eq!(a.entries, b.entries);
        assert_eq!(a.result, b.result);
        assert_eq!(game::fen(&a.start), game::fen(&b.start));
    }
    // And on to viriformat, as `data convert x.binpack -o x.vf` does.
    let vf = dir.path().join("games.vf");
    let options = ConvertOptions {
        input_format: None,
        output_format: Format::Viriformat,
        units: ScoreUnits::Cp,
        transform: GameTransform::default(),
        filter: PositionFilter::NONE,
    };
    let report = convert(&[path], &vf, &options).unwrap();
    assert_eq!(report.games_out, games.len() as u64);
    assert_eq!(read_all(&vf, Format::Viriformat), games);
}

#[test]
fn binpack_refuses_chess960() {
    let games = random_games(2, 5, 20, false);
    let dir = tempfile::tempdir().unwrap();
    let mut w = GameWriter::create(&dir.path().join("x.binpack"), Format::Binpack, PositionFilter::NONE).unwrap();
    assert!(w.write_game(&games[1]).is_err());
}

#[test]
fn bulletformat_positions() {
    let games = random_games(20, 6, 100, false);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.bullet");
    write_all(&path, Format::Bulletformat, &games);
    let total: usize = games.iter().map(|g| g.len()).sum();
    assert_eq!(std::fs::metadata(&path).unwrap().len(), (total * bullet::SIZE) as u64);
    let mut input = BufReader::new(File::open(&path).unwrap());
    for g in &games {
        g.for_each_position(|pos, e, _| {
            let d = bullet::decode(&bullet::read(&mut input).unwrap().unwrap());
            assert_eq!(d.score, e.score);
            assert_eq!(d.pieces, pos.board().occupied().count() as u32);
            assert_eq!(i8::try_from(d.result).unwrap() - 1, g.result.for_color(pos.turn()));
        });
    }
    // With Reckless's filter, fewer positions.
    let filtered = dir.path().join("f.bullet");
    let mut w = GameWriter::create(&filtered, Format::Bulletformat, PositionFilter::RECKLESS).unwrap();
    let written: u64 = games.iter().map(|g| w.write_game(g).unwrap()).sum();
    w.finish().unwrap();
    assert!(written < total as u64);
    assert_eq!(std::fs::metadata(&filtered).unwrap().len(), written * bullet::SIZE as u64);
}

#[test]
fn pgn_to_viriformat_and_rbinpack() {
    let pgn = r#"[Event "trial"]
[White "dev"]
[Black "base"]
[Result "1-0"]
[FEN "nrbqkbrn/pppppppp/8/8/8/8/PPPPPPPP/NRBQKBRN w GBgb - 0 1"]
[Variant "Chess960"]

1. e4 {+0.21/12 0.10s, n=1200} e5 {-0.18/11 0.09s} 2. Ng3 {+0.30/12 0.08s} Ng6 {-0.25/10 0.07s}
3. Be2 {+0.35/12 0.05s} Be7 {-0.30/11 0.05s} 4. O-O {+0.40/12 0.05s} 1-0
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.pgn");
    std::fs::write(&path, pgn).unwrap();
    for (name, format) in [("t.vf", Format::Viriformat), ("t.rbinpack", Format::Rbinpack)] {
        let out = dir.path().join(name);
        let options = ConvertOptions {
            input_format: None,
            output_format: format,
            units: ScoreUnits::Cp,
            transform: GameTransform::default(),
            filter: PositionFilter::NONE,
        };
        let r = convert(std::slice::from_ref(&path), &out, &options).unwrap();
        assert_eq!((r.games_out, r.positions_out), (1, 7));
        let g = &read_all(&out, format)[0];
        assert_eq!(g.entries.iter().map(|e| e.score).collect::<Vec<_>>(), vec![21, -18, 30, -25, 35, -30, 40]);
        assert!(g.entries[6].mv.is_castle());
        assert_eq!(g.result.as_u8(), 2);
    }
}

#[test]
fn shuffle_and_interleave_keep_every_record() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.vf");
    let b = dir.path().join("b.vf");
    write_all(&a, Format::Viriformat, &random_games(80, 7, 60, false));
    write_all(&b, Format::Viriformat, &random_games(40, 8, 60, false));
    let records = |paths: &[&Path]| {
        let mut all = Vec::new();
        for p in paths {
            let mut r = BufReader::new(File::open(p).unwrap());
            while let Some(rec) = shuffle::read_record(&mut r, Format::Viriformat).unwrap() {
                all.push(rec);
            }
        }
        all
    };
    let original = records(&[&a, &b]);
    let mut sorted = original.clone();
    sorted.sort();

    // In memory.
    let out = dir.path().join("s.vf");
    let r = shuffle::shuffle(&[a.clone(), b.clone()], &out, Format::Viriformat, 1 << 30, 1).unwrap();
    assert_eq!(r.buckets, 1);
    let shuffled = records(&[&out]);
    assert_ne!(shuffled, original);
    let mut s = shuffled.clone();
    s.sort();
    assert_eq!(s, sorted);

    // Through buckets: a budget far below the data.
    let out2 = dir.path().join("s2.vf");
    let r = shuffle::shuffle(&[a.clone(), b.clone()], &out2, Format::Viriformat, 4096, 2).unwrap();
    assert!(r.buckets > 1, "{r:?}");
    let mut s = records(&[&out2]);
    s.sort();
    assert_eq!(s, sorted);
    assert!(!dir.path().join("s2.shuffle-tmp").exists());

    let out3 = dir.path().join("i.vf");
    shuffle::interleave(&[a, b], &out3, Format::Viriformat, 3).unwrap();
    let mut s = records(&[&out3]);
    s.sort();
    assert_eq!(s, sorted);
}

#[test]
fn transforms_through_convert() {
    let games = random_games(30, 9, 120, false);
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.vf");
    write_all(&input, Format::Viriformat, &games);
    let out = dir.path().join("out.vf");
    let options = ConvertOptions {
        input_format: None,
        output_format: Format::Viriformat,
        units: ScoreUnits::Cp,
        transform: GameTransform { skip_plies: 4, cut_tail_at: 1900, min_positions: 3 },
        filter: PositionFilter::NONE,
    };
    let r = convert(&[input], &out, &options).unwrap();
    let back = read_all(&out, Format::Viriformat);
    assert_eq!(back.len() as u64, r.games_out);
    assert_eq!(r.games_out + r.dropped, games.len() as u64);
    for g in &back {
        assert!(g.len() >= 3);
        assert!(g.entries.iter().all(|e| e.score.unsigned_abs() < 1900));
        g.validate().unwrap();
    }
}

#[test]
fn checked_in_sample() {
    let path = Path::new(SAMPLE);
    let s = stats::stats(path, Format::Viriformat, &PositionFilter::RECKLESS).unwrap();
    assert!(s.positions >= 200 && s.positions <= 1000, "{}", s.positions);
    assert_eq!(s.wdl.iter().sum::<u64>(), s.games);
    assert_eq!(s.pieces.values().sum::<u64>(), s.positions);
    assert_eq!(s.scores.values().sum::<u64>(), s.positions);
    assert!(s.filtered_positions < s.positions);
    // Through every writable format and back.
    let games = read_all(path, Format::Viriformat);
    let dir = tempfile::tempdir().unwrap();
    for format in [Format::Viriformat, Format::Rbinpack] {
        let out = dir.path().join(format!("x.{}", format.extension()));
        write_all(&out, format, &games);
        assert_eq!(read_all(&out, format), games);
    }
    // Byte for byte back to the file.
    let mut bytes = Vec::new();
    for g in &games {
        bytes.extend(viri::encode_game(g));
    }
    assert_eq!(bytes, std::fs::read(path).unwrap());
}

/// Rewrites `tests/data/sample.vf` from the first games (about 300 positions) of a viriformat file from real datagen
/// (`SAMPLE_SOURCE`, e.g. kept by `real_reckless_datagen` with `DATAGEN_KEEP`), or from random games without one:
/// `cargo test -p data --test formats -- --ignored write_sample`.
#[test]
#[ignore]
fn write_sample() {
    let games = match std::env::var_os("SAMPLE_SOURCE") {
        Some(source) => {
            let mut games = Vec::new();
            let mut positions = 0;
            for g in GameReader::open(Path::new(&source), Format::Viriformat, ScoreUnits::Cp).unwrap() {
                let g = g.unwrap();
                positions += g.len();
                games.push(g);
                if positions >= 300 {
                    break;
                }
            }
            games
        }
        None => random_games(8, 2026, 80, false),
    };
    let mut w = BufWriter::new(File::create(SAMPLE).unwrap());
    for g in &games {
        viri::write_game(&mut w, g).unwrap();
    }
    w.flush().unwrap();
}
