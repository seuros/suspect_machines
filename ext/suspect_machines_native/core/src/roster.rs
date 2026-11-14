//! Fixed-capacity table of per-peer detectors.

use core::time::Duration;

use crate::{
    Beat, Config, ConfigError, Phi, Refusal, Statistics, Suspicion, Threshold, instant,
    window::Window,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Slot<const W: usize> {
    peer: u64,
    incarnation: u64,
    window: Window<W>,
}

/// Whether `entry` is the occupied slot of `peer`.
fn holds<const W: usize>(entry: Option<&Slot<W>>, peer: u64) -> bool {
    entry.is_some_and(|slot| slot.peer == peer)
}

/// A peer whose phi has reached the scan threshold.
///
/// `Suspect::default()` is a placeholder for pre-filling a scan buffer; the
/// roster never emits it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Suspect {
    peer: u64,
    incarnation: u64,
    phi: Phi,
}

impl Suspect {
    /// A suspect: any peer and incarnation, and its level.
    #[must_use]
    pub const fn new(peer: u64, incarnation: u64, phi: Phi) -> Self {
        Self {
            peer,
            incarnation,
            phi,
        }
    }

    /// Caller-assigned peer identifier.
    #[must_use]
    pub const fn peer(&self) -> u64 {
        self.peer
    }

    /// Incarnation the statistics belong to.
    #[must_use]
    pub const fn incarnation(&self) -> u64 {
        self.incarnation
    }

    /// Suspicion level at the scan instant.
    #[must_use]
    pub const fn phi(&self) -> Phi {
        self.phi
    }
}

/// Result of one bounded [`Roster::scan`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scan {
    suspects: usize,
    next: Option<usize>,
}

impl Scan {
    /// A scan a roster of `P` slots can answer: `None` when more than `P`
    /// suspects are written or the resume cursor names no slot (`next >= P`).
    #[must_use]
    pub const fn new<const P: usize>(suspects: usize, next: Option<usize>) -> Option<Self> {
        let resumable = match next {
            Some(index) => index < P,
            None => true,
        };
        if suspects > P || !resumable {
            return None;
        }
        Some(Self { suspects, next })
    }

    /// How many suspects were written to the front of the output buffer.
    #[must_use]
    pub const fn suspects(&self) -> usize {
        self.suspects
    }

    /// Cursor to resume from when a suspect did not fit; `None` once every
    /// slot has been examined.
    #[must_use]
    pub const fn next(&self) -> Option<usize> {
        self.next
    }
}

/// What an honoured [`Roster::forget`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forgetting {
    /// The peer was tracked; its slot is now free.
    Forgotten,
    /// The peer was not tracked: an idempotent repeat that changed nothing.
    NotTracked,
}

/// Up to `P` peers, each with a window of `W` intervals, sharing one policy.
///
/// Peers are identified by a caller-assigned `u64` and an incarnation number
/// that the caller raises when a peer restarts. The roster never removes a
/// peer on its own: membership belongs to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roster<const P: usize, const W: usize> {
    config: Config,
    slots: [Option<Slot<W>>; P],
}

impl<const P: usize, const W: usize> Roster<P, W> {
    /// Peers the roster can track.
    pub const CAPACITY: usize = P;
    /// Intervals kept per peer.
    pub const WINDOW: usize = W;

    /// An empty roster.
    ///
    /// # Errors
    ///
    /// [`ConfigError::ZeroWindow`] when `W` is zero and
    /// [`ConfigError::WarmUpExceedsWindow`] when the warm-up could never
    /// finish.
    pub fn new(config: Config) -> Result<Self, ConfigError> {
        Ok(Self {
            config: config.fit(W)?,
            slots: core::array::from_fn(|_| None),
        })
    }

    fn find(&self, peer: u64) -> Option<&Slot<W>> {
        self.slots
            .iter()
            .find(|entry| holds(entry.as_ref(), peer))?
            .as_ref()
    }

    /// The slot entry holding `peer`, to update or free.
    fn entry(&mut self, peer: u64) -> Option<&mut Option<Slot<W>>> {
        self.slots
            .iter_mut()
            .find(|entry| holds(entry.as_ref(), peer))
    }

    /// Records a heartbeat from `peer` at `now`. An unknown peer takes a free
    /// slot. A higher `incarnation` discards the peer's statistics and starts
    /// a new warm-up.
    ///
    /// # Errors
    ///
    /// [`Refusal::Overflow`], [`Refusal::Stale`] for an older incarnation,
    /// [`Refusal::TimeRegression`], [`Refusal::SameInstant`], or
    /// [`Refusal::Full`] when an unknown peer finds no free slot. The roster
    /// is unchanged.
    pub fn heartbeat(
        &mut self,
        peer: u64,
        incarnation: u64,
        now: Duration,
    ) -> Result<Beat, Refusal> {
        let now = instant(now)?;
        if let Some(Some(slot)) = self.entry(peer) {
            if incarnation < slot.incarnation {
                return Err(Refusal::Stale);
            }
            if incarnation == slot.incarnation {
                return slot.window.observe(now);
            }
            if slot.window.last().is_some_and(|last| now < last) {
                return Err(Refusal::TimeRegression);
            }
            slot.incarnation = incarnation;
            slot.window = Window::started(now);
            return Ok(Beat::Restarted);
        }
        let free = self
            .slots
            .iter_mut()
            .find(|slot| slot.is_none())
            .ok_or(Refusal::Full)?;
        *free = Some(Slot {
            peer,
            incarnation,
            window: Window::started(now),
        });
        Ok(Beat::First)
    }

    /// The suspicion for `peer` at `now`.
    ///
    /// # Errors
    ///
    /// [`Refusal::Overflow`], [`Refusal::Untracked`], or
    /// [`Refusal::TimeRegression`] when `now` precedes the peer's last
    /// heartbeat.
    pub fn suspicion(&self, peer: u64, now: Duration) -> Result<Suspicion, Refusal> {
        let now = instant(now)?;
        let slot = self.find(peer).ok_or(Refusal::Untracked)?;
        slot.window
            .suspicion(&self.config, now)
            .ok_or(Refusal::TimeRegression)
    }

    /// Writes peers whose phi has reached `threshold` at `now` into `out`,
    /// examining slots from `cursor` (zero to start). When a suspect does not
    /// fit, the scan stops and [`Scan::next`] names the slot to resume from.
    ///
    /// Peers still warming up have no phi and are never listed. Neither are
    /// peers whose last heartbeat is later than `now`: [`Roster::suspicion`]
    /// refuses them with [`Refusal::TimeRegression`], but one peer's later
    /// heartbeat says nothing about the others, so the scan skips that peer
    /// and goes on. Each call does at most `P * W` steps of work.
    ///
    /// # Errors
    ///
    /// [`Refusal::Overflow`] when `now` exceeds `u64` nanoseconds.
    pub fn scan(
        &self,
        now: Duration,
        threshold: Threshold,
        cursor: usize,
        out: &mut [Suspect],
    ) -> Result<Scan, Refusal> {
        let now = instant(now)?;
        let mut suspects = 0;
        for (index, slot) in self.slots.iter().enumerate().skip(cursor) {
            let Some(slot) = slot else { continue };
            let phi = match slot.window.suspicion(&self.config, now) {
                Some(Suspicion::Measured(phi)) if phi.reaches(threshold) => phi,
                // Below the threshold, or no phi yet (a tracked peer always
                // has a heartbeat, so this is a warm-up).
                Some(Suspicion::Measured(_) | Suspicion::WarmingUp { .. } | Suspicion::NoHeartbeat)
                // The peer's last heartbeat is later than `now`: skipped,
                // as documented above.
                | None => continue,
            };
            let Some(entry) = out.get_mut(suspects) else {
                return Ok(Scan {
                    suspects,
                    next: Some(index),
                });
            };
            *entry = Suspect::new(slot.peer, slot.incarnation, phi);
            suspects += 1;
        }
        Ok(Scan {
            suspects,
            next: None,
        })
    }

    /// Statistics of `peer`'s stored intervals; `None` when untracked or
    /// before its second heartbeat.
    #[must_use]
    pub fn statistics(&self, peer: u64) -> Option<Statistics> {
        self.find(peer)?.window.statistics()
    }

    /// The incarnation tracked for `peer`.
    #[must_use]
    pub fn incarnation(&self, peer: u64) -> Option<u64> {
        self.find(peer).map(|slot| slot.incarnation)
    }

    /// Time of `peer`'s last recorded heartbeat.
    #[must_use]
    pub fn last_heartbeat(&self, peer: u64) -> Option<Duration> {
        self.find(peer)?.window.last().map(Duration::from_nanos)
    }

    /// Stops tracking `peer` and frees its slot. A later heartbeat from the
    /// same peer starts from nothing, at any incarnation: the roster keeps no
    /// tombstones.
    ///
    /// Never refuses. [`Forgetting::NotTracked`] is an idempotent repeat: the
    /// peer was already absent and nothing changed.
    pub fn forget(&mut self, peer: u64) -> Forgetting {
        self.entry(peer).map_or(Forgetting::NotTracked, |entry| {
            *entry = None;
            Forgetting::Forgotten
        })
    }

    /// Peers currently tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    /// Whether no peer is tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The policy shared by every peer.
    #[must_use]
    pub const fn config(&self) -> Config {
        self.config
    }
}

#[cfg(test)]
mod tests;
