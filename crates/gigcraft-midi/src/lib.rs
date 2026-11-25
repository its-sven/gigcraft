//! GigCraft's MIDI logic, without I/O.
//!
//! Everything here is plain Rust that never touches a port or a clock: the
//! platform shells (CoreMIDI on Apple, `midir` on desktop, Web MIDI) move
//! the bytes and pass the time in, so all of it is deterministic and tested.
//!
//! - [`parser`]: MIDI 1.0 byte streams to complete messages and [`Event`]s.
//! - [`ump`]: MIDI 1.0 bytes ↔ Universal MIDI Packets (CoreMIDI).
//! - [`identity`]: the universal identity request and reply.
//! - [`profile`]: device profiles (TOML in `device-profiles/`).
//! - [`play_acoustic`]: the TC-Helicon Play Acoustic / Play Electric driver
//!   and a simulated pedal.
//! - [`send`]: a song part's MIDI actions as a timed schedule.
//! - [`learn`]: triggers for MIDI control inputs and MIDI Learn.

pub mod identity;
pub mod learn;
pub mod parser;
pub mod play_acoustic;
pub mod profile;
pub mod send;
pub mod ump;

pub use identity::{IDENTITY_REQUEST, IdentityReply};
pub use learn::{Trigger, ValueRange};
pub use parser::{Event, StreamParser};
pub use profile::{DeviceProfile, Numbering, ProfileError, Switching};
pub use send::{Schedule, ScheduleOptions, Scheduled, schedule};
