#![allow(clippy::float_cmp)]

use std::{assert_matches, vec::Vec};

use super::*;
use crate::test_support::{
    Expected, MILLI, Model, Rng, SECOND, at, close, exponential, random_config, roster, threshold,
};

#[test]
fn roster_tracks_incarnations_capacity_and_forgetting() {
    let mut roster = roster::<2, 4>(exponential(1));
    assert!(roster.is_empty());
    assert_eq!(roster.suspicion(7, at(0)), Err(Refusal::Untracked));
    assert_eq!(roster.heartbeat(7, 3, at(10)), Ok(Beat::First));
    assert_eq!(roster.heartbeat(8, 0, at(10)), Ok(Beat::First));
    assert_eq!(roster.len(), 2);
    let full = roster.clone();
    assert_eq!(roster.heartbeat(9, 0, at(11)), Err(Refusal::Full));
    assert_eq!(roster.heartbeat(7, 2, at(20)), Err(Refusal::Stale));
    assert_eq!(roster.heartbeat(7, 3, at(9)), Err(Refusal::TimeRegression));
    assert_eq!(roster.heartbeat(7, 4, at(9)), Err(Refusal::TimeRegression));
    assert_eq!(roster.heartbeat(7, 3, at(10)), Err(Refusal::SameInstant));
    assert_eq!(
        roster.heartbeat(7, 3, Duration::MAX),
        Err(Refusal::Overflow)
    );
    assert_eq!(roster.suspicion(7, Duration::MAX), Err(Refusal::Overflow));
    assert_eq!(roster.suspicion(7, at(9)), Err(Refusal::TimeRegression));
    assert_eq!(roster.forget(9), Forgetting::NotTracked);
    assert_eq!(roster, full);

    assert_matches!(
        roster.heartbeat(7, 3, at(20)),
        Ok(Beat::Recorded { samples: 1, .. })
    );
    assert_eq!(roster.statistics(7).map(|s| s.samples()), Some(1));
    assert_eq!(roster.incarnation(7), Some(3));
    assert_eq!(roster.last_heartbeat(7), Some(at(20)));
    assert_matches!(roster.suspicion(7, at(30)), Ok(Suspicion::Measured(_)));
    // A restart at the same instant is accepted and discards the statistics.
    assert_eq!(roster.heartbeat(7, u64::MAX, at(20)), Ok(Beat::Restarted));
    assert_eq!(roster.incarnation(7), Some(u64::MAX));
    assert_eq!(roster.statistics(7), None);
    assert_matches!(
        roster.suspicion(7, at(30)),
        Ok(Suspicion::WarmingUp { samples: 0, .. })
    );
    assert_eq!(roster.heartbeat(7, 3, at(40)), Err(Refusal::Stale));

    assert_eq!(roster.forget(7), Forgetting::Forgotten);
    let forgotten = roster.clone();
    assert_eq!(roster.forget(7), Forgetting::NotTracked);
    assert_eq!(roster, forgotten);
    assert_eq!(roster.len(), 1);
    assert_eq!(roster.incarnation(7), None);
    assert_eq!(roster.statistics(7), None);
    assert_eq!(roster.last_heartbeat(7), None);
    assert_eq!(roster.heartbeat(u64::MAX, 0, at(0)), Ok(Beat::First));
    assert_eq!(roster.heartbeat(7, 0, at(50)), Err(Refusal::Full));
    assert_eq!(roster.config(), exponential(1));
    assert_eq!(Roster::<2, 4>::CAPACITY, 2);
    assert_eq!(Roster::<2, 4>::WINDOW, 4);
    assert_eq!(
        Roster::<0, 4>::new(exponential(1)).map(|mut empty| empty.heartbeat(1, 0, at(0))),
        Ok(Err(Refusal::Full))
    );
}

#[test]
fn scan_is_bounded_resumable_and_skips_peers_without_phi() {
    let mut roster = roster::<6, 4>(exponential(2));
    // Peers 0..4 beat every 10 ns until they fall silent at 10 * (peer + 2).
    for peer in 0..5u64 {
        for beat in 0..=(peer + 2) {
            assert!(roster.heartbeat(peer, peer, at(beat * 10)).is_ok());
        }
    }
    // Peer 5 is still warming up, however long it stays silent.
    assert!(roster.heartbeat(5, 0, at(0)).is_ok());
    assert!(roster.heartbeat(5, 0, at(10)).is_ok());
    let limit = threshold(3.0);
    let now = at(120);
    // phi = silence / (10 * ln 10): at least 3 needs 70 ns of silence.
    let shape = |scan: Result<Scan, Refusal>| scan.map(|scan| (scan.suspects(), scan.next()));
    let mut all = [Suspect::default(); 6];
    assert_eq!(shape(roster.scan(now, limit, 0, &mut all)), Ok((4, None)));
    assert_eq!(all.map(|suspect| suspect.peer()), [0, 1, 2, 3, 0, 0]);
    assert_eq!(all[2].incarnation(), 2);
    let expected = 10.0 * std::f64::consts::LOG10_E;
    assert!(close(all[0].phi().value(), expected, 1e-12));

    let mut page = [Suspect::default(); 3];
    assert_eq!(
        shape(roster.scan(now, limit, 0, &mut page)),
        Ok((3, Some(3)))
    );
    assert_eq!(page.map(|suspect| suspect.peer()), [0, 1, 2]);
    assert_eq!(shape(roster.scan(now, limit, 3, &mut page)), Ok((1, None)));
    assert_eq!(page[0].peer(), 3);

    for (cursor, room, answer) in [(0, 0, (0, Some(0))), (4, 0, (0, None)), (99, 6, (0, None))] {
        let out = &mut all[..room];
        assert_eq!(shape(roster.scan(now, limit, cursor, out)), Ok(answer));
    }
    // Peers 1..4 last beat after 25 ns: skipped, not a refusal of the scan.
    let early = roster.scan(at(25), threshold(1e-9), 0, &mut all);
    assert_eq!(shape(early), Ok((1, None)));
    assert_eq!(all[0].peer(), 0);
    assert_eq!(roster.suspicion(1, at(25)), Err(Refusal::TimeRegression));
    assert_eq!(
        roster.scan(Duration::MAX, limit, 0, &mut all),
        Err(Refusal::Overflow)
    );
}

#[test]
fn roster_matches_naive_model_on_random_schedules() {
    const PEERS: usize = 6;
    const WINDOW: usize = 8;
    let mut rng = Rng::new(0x0B5E_27ED);
    let mut listed = 0;
    for _ in 0..12 {
        let config = random_config(&mut rng, WINDOW);
        let mut roster = roster::<PEERS, WINDOW>(config);
        let mut model: Vec<(u64, u64, Model)> = Vec::new();
        let mut now = 0;
        for _ in 0..400 {
            now += rng.below(300 * MILLI);
            let peer = rng.below(9);
            let known = model.iter().position(|entry| entry.0 == peer);
            if rng.below(40) == 0 {
                let forgotten = roster.forget(peer) == Forgetting::Forgotten;
                assert_eq!(forgotten, known.is_some());
                if let Some(index) = known {
                    model.remove(index);
                }
                continue;
            }
            let tracked = known.map_or(0, |index| model[index].1);
            let incarnation = match rng.below(20) {
                0 => tracked + 1,
                1 => tracked.saturating_sub(1),
                _ => tracked,
            };
            let stamp = now.saturating_sub(rng.below(3) / 2 * rng.below(SECOND));
            let expected = match known {
                Some(index) if incarnation < model[index].1 => Err(Refusal::Stale),
                Some(index) if incarnation == model[index].1 => {
                    model[index].2.observe(stamp, WINDOW)
                }
                Some(index) if model[index].2.last.is_some_and(|last| stamp < last) => {
                    Err(Refusal::TimeRegression)
                }
                Some(index) => {
                    let fresh = Model {
                        last: Some(stamp),
                        ..Model::default()
                    };
                    model[index] = (peer, incarnation, fresh);
                    Ok(Beat::Restarted)
                }
                None if model.len() == PEERS => Err(Refusal::Full),
                None => {
                    let fresh = Model {
                        last: Some(stamp),
                        ..Model::default()
                    };
                    model.push((peer, incarnation, fresh));
                    Ok(Beat::First)
                }
            };
            let before = roster.clone();
            let outcome = roster.heartbeat(peer, incarnation, at(stamp));
            assert_eq!(outcome, expected);
            if outcome.is_err() {
                assert_eq!(roster, before);
            }
            assert_eq!(roster.len(), model.len());

            // Page through a scan two suspects at a time and compare the set.
            let limit = threshold(0.25 + 4.0 * rng.unit());
            let probe = now + rng.below(5 * SECOND);
            let mut found: Vec<(u64, u64, f64)> = Vec::new();
            let mut cursor = Some(0);
            while let Some(position) = cursor {
                let mut page = [Suspect::default(); 2];
                let scan = roster
                    .scan(at(probe), limit, position, &mut page)
                    .expect("an instant that fits u64 nanoseconds");
                found.extend(
                    page.iter().take(scan.suspects()).map(|suspect| {
                        (suspect.peer(), suspect.incarnation(), suspect.phi().value())
                    }),
                );
                assert!(scan.next().is_none_or(|next| next > position));
                cursor = scan.next();
            }
            found.sort_by_key(|entry| entry.0);
            let mut expected_suspects: Vec<(u64, u64, f64)> = model
                .iter()
                .filter_map(|(peer, incarnation, model)| {
                    let Expected::Phi(phi) = model.phi(&config, probe) else {
                        return None;
                    };
                    // Leave a margin so rounding cannot flip membership.
                    assert!(!close(phi, limit.value(), 1e-7), "ambiguous threshold");
                    (phi >= limit.value()).then_some((*peer, *incarnation, phi))
                })
                .collect();
            expected_suspects.sort_by_key(|entry| entry.0);
            assert_eq!(found.len(), expected_suspects.len());
            for (actual, expected) in found.iter().zip(&expected_suspects) {
                assert_eq!((actual.0, actual.1), (expected.0, expected.1));
                assert!(close(actual.2, expected.2, 1e-8));
                listed += 1;
            }
        }
    }
    assert!(listed > 500, "only {listed} suspects compared");
}
