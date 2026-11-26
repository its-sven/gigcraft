//! Band on stage: one device leads the show, the others follow.
//!
//! The leader sends the running order position (a [`Snapshot`]) to every
//! follower; followers show it with their own chart, key and presets, say when
//! they are ready, and anyone can send a short cue ("last chorus").
//!
//! Like `gigcraft-rig`, this crate has no I/O: transports (WebSocket on the
//! desktop, the web viewer, later MultipeerConnectivity on iOS) move the JSON
//! text this crate produces and hand received text back in. Messages are one
//! JSON object each, tagged by `type`; unknown fields are ignored so newer
//! peers can add some.

use gigcraft_model::{SetlistId, SongId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Protocol version; a follower with another version is turned away.
pub const PROTOCOL: u32 = 1;
/// Largest message accepted (a snapshot carries a chart).
pub const MAX_MESSAGE_BYTES: usize = 4 << 20;
/// Longest cue text, in characters.
pub const MAX_CUE_CHARS: usize = 200;
/// Longest member name or instrument, in characters.
pub const MAX_NAME_CHARS: usize = 40;
/// Cues kept for late joiners and the UI.
pub const CUE_HISTORY: usize = 20;

/// The port a leader listens on first (transports use another if it is taken).
pub const DEFAULT_PORT: u16 = 8787;

/// `ws://host:port/ws` for what a person types or scans: `http://host:port/`,
/// `host:port`, or a `ws://` address.
pub fn ws_url(address: &str) -> Option<String> {
    let a = address.trim();
    let rest = a
        .strip_prefix("http://")
        .or_else(|| a.strip_prefix("ws://"))
        .unwrap_or(a);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").trim();
    if host.is_empty() || host.contains(' ') || a.starts_with("https://") || a.starts_with("wss://")
    {
        return None;
    }
    let has_port = if host.starts_with('[') {
        host.contains("]:")
    } else {
        host.matches(':').count() == 1
    };
    let host = if has_port {
        host.to_string()
    } else {
        format!("{host}:{DEFAULT_PORT}")
    };
    Some(format!("ws://{host}/ws"))
}

/// A transport's id for one connection.
pub type ConnId = u64;

/// Where the show is: what every follower needs to show the current song.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub setlist: SetlistId,
    pub setlist_name: String,
    /// Position in the running order, from 0.
    pub index: usize,
    /// Items in the running order.
    pub count: usize,
    /// Current part of the current song, from 0.
    pub part: usize,
    pub current: Option<Slot>,
    pub next: Option<Slot>,
}

impl Snapshot {
    /// The current song, if the current item is one.
    pub fn song(&self) -> Option<&SongSlot> {
        match &self.current {
            Some(Slot::Song(s)) => Some(s),
            _ => None,
        }
    }
}

/// One item of the running order as followers see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "built once per show change, not in a hot path"
)]
pub enum Slot {
    Song(SongSlot),
    Break {
        #[serde(default)]
        label: String,
        #[serde(default)]
        minutes: u32,
    },
    /// The leader's library no longer has this song.
    Missing,
}

/// A song in the running order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SongSlot {
    pub song: SongId,
    pub title: String,
    pub artist: String,
    /// Key for this gig.
    pub key: Option<String>,
    pub tempo_bpm: Option<f64>,
    pub time_signature: Option<String>,
    /// The leader's note for this gig.
    pub gig_notes: String,
    pub part_names: Vec<String>,
    /// Number within its set, from 1.
    pub number: usize,
    /// The leader's ChordPro text, for followers without this song.
    pub chart_source: Option<String>,
    /// Semitones from the song's key to the gig key.
    pub semitones: i8,
    /// The chart already parsed and transposed (`gigcraft_chordpro::Chart` as
    /// JSON), for viewers that cannot parse ChordPro (the web viewer).
    pub chart: Option<Value>,
}

/// A band member as everyone sees them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub instrument: String,
    #[serde(default)]
    pub ready: bool,
}

/// A place in a chart that means the same on every screen: a section
/// (chart block, from 0) and how far into it (0.0 = its top, 1.0 = the next).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Spot {
    pub block: usize,
    #[serde(default)]
    pub fraction: f32,
}

impl Spot {
    /// The spot at `offset` in a chart whose blocks start at `tops` (points,
    /// ascending; `end` is the chart's height).
    #[must_use]
    pub fn at(offset: f32, tops: &[f32], end: f32) -> Self {
        let Some(block) = tops.iter().rposition(|t| *t <= offset) else {
            return Self::default();
        };
        let next = tops.get(block + 1).copied().unwrap_or(end);
        let span = (next - tops[block]).max(1.0);
        Self {
            block,
            fraction: ((offset - tops[block]) / span).clamp(0.0, 1.0),
        }
    }

    /// The very top: the start of the first section, or above it (the song's title).
    #[must_use]
    pub fn is_top(&self) -> bool {
        self.block == 0 && self.fraction <= 0.0
    }

    /// The offset of this spot in a chart whose blocks start at `tops`.
    #[must_use]
    pub fn offset(&self, tops: &[f32], end: f32) -> Option<f32> {
        let top = *tops.get(self.block)?;
        let next = tops.get(self.block + 1).copied().unwrap_or(end);
        Some(top + (next - top).max(0.0) * self.fraction.clamp(0.0, 1.0))
    }

    fn valid(&self) -> bool {
        self.fraction.is_finite() && self.block < 10_000
    }
}

/// A place marked for the whole band ("from here"), e.g. in rehearsal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    pub song: SongId,
    pub spot: Spot,
    /// Who set it.
    pub from: String,
    #[serde(default)]
    pub text: String,
}

/// A short message to the whole band.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cue {
    pub from: String,
    pub text: String,
    /// Leader's clock, milliseconds.
    #[serde(default)]
    pub at_ms: u64,
}

/// Leader → follower.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "built once per show change, not in a hot path"
)]
pub enum ToFollower {
    Welcome {
        protocol: u32,
        session: String,
        leader: String,
        /// The follower's own member id.
        member: u32,
        #[serde(default)]
        cues: Vec<Cue>,
        #[serde(default)]
        mark: Option<Mark>,
    },
    State {
        seq: u64,
        snapshot: Option<Snapshot>,
    },
    Roster {
        members: Vec<Member>,
    },
    Cue(Cue),
    /// Where the leader's chart is (for "follow the leader's scrolling").
    Scroll {
        song: SongId,
        spot: Spot,
    },
    /// The band's mark was set or cleared.
    Mark {
        mark: Option<Mark>,
    },
    Bye {
        reason: String,
    },
}

/// Follower → leader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToLeader {
    Join {
        protocol: u32,
        name: String,
        #[serde(default)]
        instrument: String,
    },
    Ready {
        ready: bool,
    },
    Cue {
        text: String,
    },
    /// Mark a place for everyone.
    Mark {
        song: SongId,
        spot: Spot,
        #[serde(default)]
        text: String,
    },
    ClearMark,
    Ping,
}

/// JSON text of a message.
pub fn encode<T: Serialize>(msg: &T) -> String {
    serde_json::to_string(msg).unwrap_or_default()
}

/// Trim, drop control characters, shorten to `max` characters.
fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(max)
        .collect()
}

/// What the transport should do, or what the UI should know.
#[derive(Debug, Clone, PartialEq)]
pub enum LeaderOut {
    /// Send this text to one connection.
    Send {
        conn: ConnId,
        text: String,
    },
    /// Close the connection (after sending what came before).
    Close {
        conn: ConnId,
    },
    Event(LeaderEvent),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LeaderEvent {
    Joined(Member),
    Left(Member),
    ReadyChanged(Member),
    Cue(Cue),
    /// The band's mark was set (`Some`) or cleared.
    Mark(Option<Mark>),
    /// A message that was not understood (kept for the log).
    Rejected {
        conn: ConnId,
        reason: String,
    },
}

/// The leading device's side of a live session.
#[derive(Debug, Clone)]
pub struct Leader {
    session: String,
    name: String,
    next_id: u32,
    members: BTreeMap<ConnId, Member>,
    seq: u64,
    snapshot: Option<Snapshot>,
    cues: Vec<Cue>,
    mark: Option<Mark>,
    /// The last scroll position sent.
    scroll: Option<(SongId, Spot)>,
}

impl Leader {
    /// A new session; `session` should be unique per start (followers see it
    /// change when the leader restarts).
    pub fn new(session: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            name: clean(&name.into(), MAX_NAME_CHARS),
            next_id: 1,
            members: BTreeMap::new(),
            seq: 0,
            snapshot: None,
            cues: Vec::new(),
            mark: None,
            scroll: None,
        }
    }

    pub fn session(&self) -> &str {
        &self.session
    }

    /// Joined members, in join order.
    pub fn members(&self) -> Vec<Member> {
        let mut m: Vec<Member> = self.members.values().cloned().collect();
        m.sort_by_key(|m| m.id);
        m
    }

    pub fn ready_count(&self) -> usize {
        self.members.values().filter(|m| m.ready).count()
    }

    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }

    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    /// The band's mark, if one is set.
    pub fn mark(&self) -> Option<&Mark> {
        self.mark.as_ref()
    }

    /// The leader's chart is at `spot` of `song`: tell the band (only when it
    /// moved noticeably, so calling it every frame is fine).
    pub fn scroll(&mut self, song: SongId, spot: Spot) -> Vec<LeaderOut> {
        if !spot.valid() {
            return Vec::new();
        }
        if let Some((s, last)) = self.scroll
            && s == song
            && last.block == spot.block
            && (last.fraction - spot.fraction).abs() < 0.02
        {
            return Vec::new();
        }
        self.scroll = Some((song, spot));
        self.to_all(&ToFollower::Scroll { song, spot })
    }

    /// Set (or with `None`, clear) the band's mark.
    pub fn set_mark(&mut self, mark: Option<(SongId, Spot, &str)>) -> Vec<LeaderOut> {
        let from = self.name.clone();
        self.put_mark(mark.map(|(song, spot, text)| (song, spot, text.to_string(), from)))
    }

    fn put_mark(&mut self, mark: Option<(SongId, Spot, String, String)>) -> Vec<LeaderOut> {
        let mark = match mark {
            Some((_, spot, _, _)) if !spot.valid() => return Vec::new(),
            Some((song, spot, text, from)) => Some(Mark {
                song,
                spot,
                from,
                text: clean(&text, MAX_CUE_CHARS),
            }),
            None => None,
        };
        if mark == self.mark {
            return Vec::new();
        }
        self.mark = mark.clone();
        let mut out = self.to_all(&ToFollower::Mark { mark: mark.clone() });
        out.push(LeaderOut::Event(LeaderEvent::Mark(mark)));
        out
    }

    fn to_all(&self, msg: &ToFollower) -> Vec<LeaderOut> {
        let text = encode(msg);
        self.members
            .keys()
            .map(|&conn| LeaderOut::Send {
                conn,
                text: text.clone(),
            })
            .collect()
    }

    fn roster(&self) -> Vec<LeaderOut> {
        self.to_all(&ToFollower::Roster {
            members: self.members(),
        })
    }

    /// Text received from a connection.
    pub fn on_text(&mut self, conn: ConnId, text: &str, now_ms: u64) -> Vec<LeaderOut> {
        if text.len() > MAX_MESSAGE_BYTES {
            return vec![LeaderOut::Event(LeaderEvent::Rejected {
                conn,
                reason: "message too large".into(),
            })];
        }
        match serde_json::from_str::<ToLeader>(text) {
            Ok(msg) => self.on_message(conn, msg, now_ms),
            Err(e) => vec![LeaderOut::Event(LeaderEvent::Rejected {
                conn,
                reason: e.to_string(),
            })],
        }
    }

    pub fn on_message(&mut self, conn: ConnId, msg: ToLeader, now_ms: u64) -> Vec<LeaderOut> {
        let mut out = Vec::new();
        match msg {
            ToLeader::Join {
                protocol,
                name,
                instrument,
            } => {
                if protocol != PROTOCOL {
                    out.push(LeaderOut::Send {
                        conn,
                        text: encode(&ToFollower::Bye {
                            reason: format!(
                                "protocol {protocol} not supported; this leader speaks {PROTOCOL}"
                            ),
                        }),
                    });
                    out.push(LeaderOut::Close { conn });
                    return out;
                }
                let name = match clean(&name, MAX_NAME_CHARS) {
                    n if n.is_empty() => "Guest".to_string(),
                    n => n,
                };
                let instrument = clean(&instrument, MAX_NAME_CHARS);
                let id = match self.members.get(&conn) {
                    Some(m) => m.id,
                    None => {
                        let id = self.next_id;
                        self.next_id += 1;
                        id
                    }
                };
                let member = Member {
                    id,
                    name,
                    instrument,
                    ready: false,
                };
                self.members.insert(conn, member.clone());
                out.push(LeaderOut::Send {
                    conn,
                    text: encode(&ToFollower::Welcome {
                        protocol: PROTOCOL,
                        session: self.session.clone(),
                        leader: self.name.clone(),
                        member: id,
                        cues: self.cues.clone(),
                        mark: self.mark.clone(),
                    }),
                });
                out.push(LeaderOut::Send {
                    conn,
                    text: encode(&ToFollower::State {
                        seq: self.seq,
                        snapshot: self.snapshot.clone(),
                    }),
                });
                if let Some((song, spot)) = self.scroll {
                    out.push(LeaderOut::Send {
                        conn,
                        text: encode(&ToFollower::Scroll { song, spot }),
                    });
                }
                out.extend(self.roster());
                out.push(LeaderOut::Event(LeaderEvent::Joined(member)));
            }
            ToLeader::Ready { ready } => {
                let Some(m) = self.members.get_mut(&conn) else {
                    return out;
                };
                if m.ready != ready {
                    m.ready = ready;
                    let m = m.clone();
                    out.extend(self.roster());
                    out.push(LeaderOut::Event(LeaderEvent::ReadyChanged(m)));
                }
            }
            ToLeader::Cue { text } => {
                let Some(m) = self.members.get(&conn) else {
                    return out;
                };
                let from = m.name.clone();
                out.extend(self.push_cue(from, &text, now_ms));
            }
            ToLeader::Mark { song, spot, text } => {
                let Some(m) = self.members.get(&conn) else {
                    return out;
                };
                let from = m.name.clone();
                out.extend(self.put_mark(Some((song, spot, text, from))));
            }
            ToLeader::ClearMark => {
                if self.members.contains_key(&conn) {
                    out.extend(self.put_mark(None));
                }
            }
            ToLeader::Ping => {}
        }
        out
    }

    fn push_cue(&mut self, from: String, text: &str, now_ms: u64) -> Vec<LeaderOut> {
        let text = clean(text, MAX_CUE_CHARS);
        if text.is_empty() {
            return Vec::new();
        }
        let cue = Cue {
            from,
            text,
            at_ms: now_ms,
        };
        self.cues.push(cue.clone());
        if self.cues.len() > CUE_HISTORY {
            self.cues.remove(0);
        }
        let mut out = self.to_all(&ToFollower::Cue(cue.clone()));
        out.push(LeaderOut::Event(LeaderEvent::Cue(cue)));
        out
    }

    /// The leader's own cue to the band.
    pub fn cue(&mut self, text: &str, now_ms: u64) -> Vec<LeaderOut> {
        let from = self.name.clone();
        self.push_cue(from, text, now_ms)
    }

    /// A connection went away.
    pub fn disconnected(&mut self, conn: ConnId) -> Vec<LeaderOut> {
        let Some(m) = self.members.remove(&conn) else {
            return Vec::new();
        };
        let mut out = self.roster();
        out.push(LeaderOut::Event(LeaderEvent::Left(m)));
        out
    }

    /// The show moved (or stopped: `None`). Ready flags reset when the song changes:
    /// "ready" always means ready for the current song.
    pub fn set_snapshot(&mut self, snapshot: Option<Snapshot>) -> Vec<LeaderOut> {
        if snapshot == self.snapshot {
            return Vec::new();
        }
        let moved = |a: &Option<Snapshot>, b: &Option<Snapshot>| match (a, b) {
            (Some(a), Some(b)) => a.setlist != b.setlist || a.index != b.index,
            (None, None) => false,
            _ => true,
        };
        let reset = moved(&self.snapshot, &snapshot);
        self.snapshot = snapshot;
        self.seq += 1;
        let mut out = self.to_all(&ToFollower::State {
            seq: self.seq,
            snapshot: self.snapshot.clone(),
        });
        if reset && self.members.values().any(|m| m.ready) {
            for m in self.members.values_mut() {
                m.ready = false;
            }
            out.extend(self.roster());
        }
        out
    }

    /// End the session: tell everyone and close.
    pub fn stop(&mut self, reason: &str) -> Vec<LeaderOut> {
        let mut out = self.to_all(&ToFollower::Bye {
            reason: reason.to_string(),
        });
        out.extend(self.members.keys().map(|&conn| LeaderOut::Close { conn }));
        self.members.clear();
        out
    }
}

/// What changed for a follower.
#[derive(Debug, Clone, PartialEq)]
pub enum FollowerEvent {
    Joined {
        leader: String,
    },
    /// New position; `song_changed` when the current item is another one.
    Moved {
        song_changed: bool,
        part_changed: bool,
    },
    Roster,
    Cue(Cue),
    /// The leader's chart moved.
    Scrolled,
    /// The band's mark changed.
    Marked,
    Ended {
        reason: String,
    },
}

/// A following device's side of a live session.
#[derive(Debug, Clone, Default)]
pub struct Follower {
    pub name: String,
    pub instrument: String,
    session: Option<String>,
    leader: String,
    member: Option<u32>,
    seq: u64,
    snapshot: Option<Snapshot>,
    roster: Vec<Member>,
    cues: Vec<Cue>,
    ready: bool,
    mark: Option<Mark>,
    scroll: Option<(SongId, Spot)>,
}

impl Follower {
    pub fn new(name: &str, instrument: &str) -> Self {
        Self {
            name: clean(name, MAX_NAME_CHARS),
            instrument: clean(instrument, MAX_NAME_CHARS),
            ..Default::default()
        }
    }

    /// The first message to send after connecting.
    pub fn join(&self) -> String {
        encode(&ToLeader::Join {
            protocol: PROTOCOL,
            name: self.name.clone(),
            instrument: self.instrument.clone(),
        })
    }

    /// Say ready / not ready; returns the message to send.
    pub fn set_ready(&mut self, ready: bool) -> String {
        self.ready = ready;
        encode(&ToLeader::Ready { ready })
    }

    /// A cue to the band; returns the message to send.
    pub fn cue(&self, text: &str) -> String {
        encode(&ToLeader::Cue {
            text: clean(text, MAX_CUE_CHARS),
        })
    }

    pub fn joined(&self) -> bool {
        self.member.is_some()
    }

    pub fn leader(&self) -> &str {
        &self.leader
    }

    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    pub fn roster(&self) -> &[Member] {
        &self.roster
    }

    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }

    pub fn ready(&self) -> bool {
        self.ready
    }

    /// The band's mark, if one is set.
    pub fn mark(&self) -> Option<&Mark> {
        self.mark.as_ref()
    }

    /// Where the leader's chart is, if it said so.
    pub fn leader_scroll(&self) -> Option<(SongId, Spot)> {
        self.scroll
    }

    /// Mark a place for everyone; returns the message to send.
    pub fn mark_spot(&self, song: SongId, spot: Spot, text: &str) -> String {
        encode(&ToLeader::Mark {
            song,
            spot,
            text: clean(text, MAX_CUE_CHARS),
        })
    }

    /// Clear the band's mark; returns the message to send.
    pub fn clear_mark(&self) -> String {
        encode(&ToLeader::ClearMark)
    }

    /// Text received from the leader.
    pub fn on_text(&mut self, text: &str) -> Result<Vec<FollowerEvent>, String> {
        if text.len() > MAX_MESSAGE_BYTES {
            return Err("message too large".into());
        }
        let msg: ToFollower = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Ok(self.on_message(msg))
    }

    pub fn on_message(&mut self, msg: ToFollower) -> Vec<FollowerEvent> {
        match msg {
            ToFollower::Welcome {
                session,
                leader,
                member,
                cues,
                mark,
                ..
            } => {
                self.mark = mark;
                if self.session.as_deref() != Some(session.as_str()) {
                    self.seq = 0;
                    self.snapshot = None;
                }
                self.session = Some(session);
                self.leader = leader.clone();
                self.member = Some(member);
                self.cues = cues;
                self.ready = false;
                vec![FollowerEvent::Joined { leader }]
            }
            ToFollower::State { seq, snapshot } => {
                if seq < self.seq {
                    return Vec::new();
                }
                self.seq = seq;
                let before = self.snapshot.take();
                let song_changed = match (&before, &snapshot) {
                    (Some(a), Some(b)) => a.setlist != b.setlist || a.index != b.index,
                    (None, None) => false,
                    _ => true,
                };
                let part_changed = song_changed
                    || before.as_ref().map(|s| s.part) != snapshot.as_ref().map(|s| s.part);
                if song_changed {
                    self.ready = false;
                }
                self.snapshot = snapshot;
                vec![FollowerEvent::Moved {
                    song_changed,
                    part_changed,
                }]
            }
            ToFollower::Roster { members } => {
                if let Some(me) = self.member
                    && let Some(m) = members.iter().find(|m| m.id == me)
                {
                    self.ready = m.ready;
                }
                self.roster = members;
                vec![FollowerEvent::Roster]
            }
            ToFollower::Cue(cue) => {
                self.cues.push(cue.clone());
                if self.cues.len() > CUE_HISTORY {
                    self.cues.remove(0);
                }
                vec![FollowerEvent::Cue(cue)]
            }
            ToFollower::Scroll { song, spot } => {
                if !spot.valid() {
                    return Vec::new();
                }
                self.scroll = Some((song, spot));
                vec![FollowerEvent::Scrolled]
            }
            ToFollower::Mark { mark } => {
                self.mark = mark;
                vec![FollowerEvent::Marked]
            }
            ToFollower::Bye { reason } => {
                self.member = None;
                vec![FollowerEvent::Ended { reason }]
            }
        }
    }
}

#[cfg(test)]
mod tests;
