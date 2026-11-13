//! Fixed-capacity table of per-peer detectors.

use core::time::Duration;

use crate::{
    Beat, Config, ConfigError, Phi, Refusal, Statistics, Suspicion, Threshold, nanos,
    window::Window,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Slot<const W: usize> {
    peer: u64,
    incarnation: u64,
    window: Window<W>,
}

/// A peer whose phi has reached the scan threshold.
///
/// `Suspect::default()` is a placeholder for pre-filling a scan buffer; the
/// roster never emits it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Suspect {
    /// Caller-assigned peer identifier.
    pub peer: u64,
    /// Incarnation the statistics belong to.
    pub incarnation: u64,
    /// Suspicion level at the scan instant.
    pub phi: Phi,
}

/// Result of one bounded [`Roster::scan`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scan {
    /// Suspects written to the front of the output buffer.
    pub written: usize,
    /// Cursor to resume from when a suspect did not fit; `None` once every
    /// slot has been examined.
    pub next: Option<usize>,
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
        self.slots.iter().flatten().find(|slot| slot.peer == peer)
    }

    /// Records a heartbeat from `peer` at `now`. An unknown peer takes a free
    /// slot. A higher `incarnation` discards the peer's statistics and starts
    /// a new warm-up.
    ///
    /// # Errors
    ///
    /// [`Refusal::Overflow`], [`Refusal::Stale`] for an older incarnation,
    /// [`Refusal::TimeRegression`], [`Refusal::Duplicate`], or
    /// [`Refusal::Full`] when an unknown peer finds no free slot. The roster
    /// is unchanged.
    pub fn heartbeat(
        &mut self,
        peer: u64,
        incarnation: u64,
        now: Duration,
    ) -> Result<Beat, Refusal> {
        let now = nanos(now).ok_or(Refusal::Overflow)?;
        let tracked = self
            .slots
            .iter_mut()
            .flatten()
            .find(|slot| slot.peer == peer);
        if let Some(slot) = tracked {
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
            slot.window.reset();
            slot.window.observe(now)?;
            return Ok(Beat::Restarted);
        }
        let free = self
            .slots
            .iter_mut()
            .find(|slot| slot.is_none())
            .ok_or(Refusal::Full)?;
        let mut window = Window::new();
        let beat = window.observe(now)?;
        *free = Some(Slot {
            peer,
            incarnation,
            window,
        });
        Ok(beat)
    }

    /// The suspicion for `peer` at `now`.
    ///
    /// # Errors
    ///
    /// [`Refusal::Overflow`], [`Refusal::Untracked`], or
    /// [`Refusal::TimeRegression`] when `now` precedes the peer's last
    /// heartbeat.
    pub fn suspicion(&self, peer: u64, now: Duration) -> Result<Suspicion, Refusal> {
        let now = nanos(now).ok_or(Refusal::Overflow)?;
        let slot = self.find(peer).ok_or(Refusal::Untracked)?;
        slot.window.suspicion(&self.config, now)
    }

    /// Writes peers whose phi has reached `threshold` at `now` into `out`,
    /// examining slots from `cursor` (zero to start). When a suspect does not
    /// fit, the scan stops and [`Scan::next`] names the slot to resume from.
    ///
    /// Peers still warming up have no phi and are never listed; neither are
    /// peers whose last heartbeat is later than `now`. Each call does at most
    /// `P * W` steps of work.
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
        let now = nanos(now).ok_or(Refusal::Overflow)?;
        let mut written = 0;
        for (index, slot) in self.slots.iter().enumerate().skip(cursor) {
            let Some(slot) = slot else { continue };
            let Ok(Suspicion::Measured(phi)) = slot.window.suspicion(&self.config, now) else {
                continue;
            };
            if !phi.reaches(threshold) {
                continue;
            }
            let Some(entry) = out.get_mut(written) else {
                return Ok(Scan {
                    written,
                    next: Some(index),
                });
            };
            *entry = Suspect {
                peer: slot.peer,
                incarnation: slot.incarnation,
                phi,
            };
            written += 1;
        }
        Ok(Scan {
            written,
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

    /// Stops tracking `peer` and frees its slot. Returns whether it was
    /// tracked. A later heartbeat from the same peer starts from nothing, at
    /// any incarnation: the roster keeps no tombstones.
    pub fn forget(&mut self, peer: u64) -> bool {
        let tracked = self
            .slots
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|slot| slot.peer == peer));
        tracked.is_some_and(|slot| slot.take().is_some())
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
