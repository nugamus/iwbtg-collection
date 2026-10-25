//! Replay file (`.iwbr`) encode and decode. See docs/architecture.md, "Replay file (.iwbr)".
//!
//! A replay is a postcard-encoded struct: header, run-length encoded input frames, trailer. The
//! file form is canonical: `decode` accepts only bytes that `encode` would write for the same
//! replay, so identical runs always give identical files. Both sides run the same consistency
//! checks, so `encode` never writes a file that `decode` rejects.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::input::InputFrame;

/// File magic, the first four bytes of every replay.
pub const MAGIC: [u8; 4] = *b"IWBR";

/// The replay layout version this module reads and writes. Bump it on any layout change.
pub const REPLAY_FORMAT: u16 = 1;

/// Hard ceiling on the ticks in one replay: 48 hours at 50 ticks per second. Bounds memory when
/// runs are expanded (6 bytes per tick) before the game's own `max_ticks` is known.
pub const MAX_REPLAY_TICKS: u64 = 8_640_000;

/// Ticks between state-hash checkpoints. Every checkpoint tick is a multiple of this.
pub const CHECKPOINT_INTERVAL: u64 = 500;

/// How a run starts. Only `NewGame` is ranked.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum StartMode {
    /// A fresh game with an empty save slot.
    NewGame,
    /// From a serialized save slot (practice and route testing).
    FromSave(Vec<u8>),
}

/// Everything needed to set up the sim before the first tick.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Header {
    /// Always [`MAGIC`]; `decode` rejects anything else.
    pub magic: [u8; 4],
    /// Always [`REPLAY_FORMAT`] when written by this build.
    pub replay_format: u16,
    /// `SIM_VERSION` of the build that recorded the replay.
    pub sim_version: u32,
    /// Package id from `game.toml`.
    pub package_id: String,
    /// Gameplay hash of the package the replay was recorded on.
    pub gameplay_hash: [u8; 32],
    /// Built-in behavior revision the run used.
    pub engine_rev: u16,
    /// Seed of the sim's random generator, its only randomness.
    pub rng_seed: u64,
    /// New game or a saved state.
    pub start_mode: StartMode,
    /// Client build that recorded the replay, for bug reports only.
    pub client_build_id: String,
}

/// What the recording client claims about the run. The server re-simulates and trusts none of it.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Trailer {
    /// Ticks simulated; equals the number of input frames.
    pub total_ticks: u64,
    /// `(stage index, tick)` for each split, in tick order, none past `total_ticks`. Two splits
    /// may share a tick when several timing triggers fire in the same tick.
    pub splits: Vec<(u16, u64)>,
    /// `(tick, state hash truncated to 16 bytes)` every [`CHECKPOINT_INTERVAL`] ticks, to locate
    /// desyncs. Ticks are strictly increasing multiples of the interval, none past `total_ticks`.
    pub checkpoints: Vec<(u64, [u8; 16])>,
    /// State hash after the last tick.
    pub final_hash: [u8; 32],
}

/// A decoded replay with one input frame per tick.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Replay {
    /// Setup for the sim.
    pub header: Header,
    /// One frame per tick, already expanded from the run-length body.
    pub inputs: Vec<InputFrame>,
    /// Claimed results.
    pub trailer: Trailer,
}

/// Why a replay could not be encoded, or why a replay file was rejected.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum ReplayError {
    /// The file does not start with `IWBR`.
    #[error("not a replay file (wrong magic)")]
    BadMagic,
    /// A layout version this build does not know.
    #[error("unknown replay format {0}")]
    UnknownFormat(u16),
    /// Postcard could not decode the bytes.
    #[error("malformed replay: {0}")]
    Malformed(#[from] postcard::Error),
    /// Bytes left over after the trailer.
    #[error("{0} trailing bytes after the replay")]
    TrailingBytes(u64),
    /// A run of zero ticks, which `encode` never writes.
    #[error("run {0} has length zero")]
    ZeroLengthRun(u64),
    /// A run with the same frame as the run before it, which `encode` would have merged.
    #[error("run {0} repeats the frame of the run before it")]
    RepeatedFrame(u64),
    /// More ticks than [`MAX_REPLAY_TICKS`].
    #[error("replay has more than {MAX_REPLAY_TICKS} ticks")]
    TooManyTicks,
    /// The runs add up to a different tick count than the trailer claims.
    #[error("runs hold {runs} ticks but the trailer says {total_ticks}")]
    TickCountMismatch {
        /// Sum of all run lengths.
        runs: u64,
        /// `trailer.total_ticks`.
        total_ticks: u64,
    },
    /// A split past `total_ticks` or earlier than the split before it.
    #[error("split {0} is past the last tick or out of order")]
    BadSplit(u64),
    /// A checkpoint past `total_ticks`, not on a multiple of [`CHECKPOINT_INTERVAL`], or not later
    /// than the checkpoint before it.
    #[error("checkpoint {0} is past the last tick, off the interval grid or out of order")]
    BadCheckpoint(u64),
    /// The bytes decode but are not what `encode` writes (for example an overlong varint).
    #[error("replay is not in canonical form")]
    NotCanonical,
}

/// The file layout, borrowed for encoding.
#[derive(Serialize)]
struct FileRef<'a> {
    header: &'a Header,
    runs: Vec<(u32, InputFrame)>,
    trailer: &'a Trailer,
}

/// The file layout, owned for decoding. Field order and types must match [`FileRef`].
#[derive(Serialize, Deserialize)]
struct FileOwned {
    header: Header,
    runs: Vec<(u32, InputFrame)>,
    trailer: Trailer,
}

impl Replay {
    /// The canonical file bytes: adjacent equal frames merged into one run.
    ///
    /// Fails, writing nothing, when the replay breaks a rule `decode` checks: wrong magic or
    /// format, more than [`MAX_REPLAY_TICKS`] inputs, `total_ticks` not equal to the number of
    /// inputs, or splits and checkpoints out of order or past the last tick.
    pub fn encode(&self) -> Result<Vec<u8>, ReplayError> {
        let ticks = self.inputs.len() as u64;
        if ticks > MAX_REPLAY_TICKS {
            return Err(ReplayError::TooManyTicks);
        }
        check(&self.header, ticks, &self.trailer)?;

        let file = FileRef {
            header: &self.header,
            runs: runs(&self.inputs),
            trailer: &self.trailer,
        };
        // Serializing plain structs, vectors and strings into a growable Vec cannot fail.
        Ok(postcard::to_allocvec(&file).expect("postcard encoding into a Vec is infallible"))
    }

    /// Reads a replay file, accepting only the canonical bytes `encode` would write.
    pub fn decode(bytes: &[u8]) -> Result<Replay, ReplayError> {
        if bytes.get(..MAGIC.len()) != Some(&MAGIC[..]) {
            return Err(ReplayError::BadMagic);
        }
        // Check the version before decoding the rest: other versions may use another layout.
        let ((_, replay_format), _) = postcard::take_from_bytes::<([u8; 4], u16)>(bytes)?;
        if replay_format != REPLAY_FORMAT {
            return Err(ReplayError::UnknownFormat(replay_format));
        }

        let (file, rest) = postcard::take_from_bytes::<FileOwned>(bytes)?;
        if !rest.is_empty() {
            return Err(ReplayError::TrailingBytes(rest.len() as u64));
        }

        let mut total: u64 = 0;
        let mut previous: Option<InputFrame> = None;
        for (index, &(len, frame)) in (0u64..).zip(&file.runs) {
            if len == 0 {
                return Err(ReplayError::ZeroLengthRun(index));
            }
            if previous == Some(frame) {
                return Err(ReplayError::RepeatedFrame(index));
            }
            previous = Some(frame);
            total += u64::from(len);
            if total > MAX_REPLAY_TICKS {
                return Err(ReplayError::TooManyTicks);
            }
        }
        check(&file.header, total, &file.trailer)?;

        if postcard::to_allocvec(&file)? != bytes {
            return Err(ReplayError::NotCanonical);
        }

        let capacity = usize::try_from(total).map_err(|_| ReplayError::TooManyTicks)?;
        let mut inputs = Vec::with_capacity(capacity);
        for &(len, frame) in &file.runs {
            let len = usize::try_from(len).map_err(|_| ReplayError::TooManyTicks)?;
            inputs.extend(std::iter::repeat_n(frame, len));
        }

        Ok(Replay {
            header: file.header,
            inputs,
            trailer: file.trailer,
        })
    }

    /// Text form for `iwbtg replay dump`: header fields, one run per line, then the trailer.
    pub fn dump(&self) -> String {
        let h = &self.header;
        let t = &self.trailer;
        let mut out = String::new();
        // Writing to a String cannot fail, so the fmt results are ignored.
        let _ = writeln!(out, "magic {:?}", String::from_utf8_lossy(&h.magic));
        let _ = writeln!(out, "replay_format {}", h.replay_format);
        let _ = writeln!(out, "sim_version {}", h.sim_version);
        let _ = writeln!(out, "package_id {:?}", h.package_id);
        let _ = writeln!(out, "gameplay_hash {}", hex(&h.gameplay_hash));
        let _ = writeln!(out, "engine_rev {}", h.engine_rev);
        let _ = writeln!(out, "rng_seed {}", h.rng_seed);
        match &h.start_mode {
            StartMode::NewGame => {
                let _ = writeln!(out, "start_mode new_game");
            }
            StartMode::FromSave(save) => {
                let _ = writeln!(out, "start_mode from_save {}", hex(save));
            }
        }
        let _ = writeln!(out, "client_build_id {:?}", h.client_build_id);

        let runs = runs(&self.inputs);
        let _ = writeln!(out, "runs {}", runs.len());
        let mut tick: u64 = 0;
        for (len, f) in runs {
            let last = tick + u64::from(len) - 1;
            let _ = writeln!(
                out,
                "ticks {tick}-{last} ({len}): held {:#06x} pressed {:#06x} released {:#06x}",
                f.held, f.pressed, f.released
            );
            tick = last + 1;
        }

        let _ = writeln!(out, "total_ticks {}", t.total_ticks);
        for (stage, tick) in &t.splits {
            let _ = writeln!(out, "split stage {stage} tick {tick}");
        }
        for (tick, hash) in &t.checkpoints {
            let _ = writeln!(out, "checkpoint tick {tick} {}", hex(hash));
        }
        let _ = writeln!(out, "final_hash {}", hex(&t.final_hash));
        out
    }
}

/// The rules shared by `encode` and `decode`, given the real tick count (inputs or run lengths).
fn check(header: &Header, ticks: u64, trailer: &Trailer) -> Result<(), ReplayError> {
    if header.magic != MAGIC {
        return Err(ReplayError::BadMagic);
    }
    if header.replay_format != REPLAY_FORMAT {
        return Err(ReplayError::UnknownFormat(header.replay_format));
    }
    if ticks != trailer.total_ticks {
        return Err(ReplayError::TickCountMismatch {
            runs: ticks,
            total_ticks: trailer.total_ticks,
        });
    }

    let mut previous: Option<u64> = None;
    for (index, &(_, tick)) in (0u64..).zip(&trailer.splits) {
        if tick > ticks || previous.is_some_and(|p| tick < p) {
            return Err(ReplayError::BadSplit(index));
        }
        previous = Some(tick);
    }

    let mut previous: Option<u64> = None;
    for (index, &(tick, _)) in (0u64..).zip(&trailer.checkpoints) {
        if tick > ticks || tick % CHECKPOINT_INTERVAL != 0 || previous.is_some_and(|p| tick <= p) {
            return Err(ReplayError::BadCheckpoint(index));
        }
        previous = Some(tick);
    }
    Ok(())
}

/// Run-length encodes frames: maximal runs of equal frames, split only past `u32::MAX` ticks
/// (far beyond [`MAX_REPLAY_TICKS`], so never in a valid replay).
fn runs(inputs: &[InputFrame]) -> Vec<(u32, InputFrame)> {
    let mut out: Vec<(u32, InputFrame)> = Vec::new();
    for &frame in inputs {
        match out.last_mut() {
            Some((len, last)) if *last == frame && *len < u32::MAX => *len += 1,
            _ => out.push((1, frame)),
        }
    }
    out
}

/// Lowercase hex without separators.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed 64-bit LCG (Knuth MMIX constants) for reproducible test data.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 32) as u32
        }

        /// The low byte of the next value.
        fn byte(&mut self) -> u8 {
            self.next().to_le_bytes()[0]
        }

        /// Uniform-ish index in `0..=hi`.
        fn index(&mut self, hi: usize) -> usize {
            let hi = u32::try_from(hi).expect("test buffers are small");
            usize::try_from(self.range(0, hi)).expect("u32 fits in usize")
        }

        /// Uniform-ish value in `lo..=hi`.
        fn range(&mut self, lo: u32, hi: u32) -> u32 {
            lo + self.next() % (hi - lo + 1)
        }
    }

    const LEFT: u16 = 1 << 0;
    const RIGHT: u16 = 1 << 1;
    const JUMP: u16 = 1 << 4;
    const SHOOT: u16 = 1 << 5;

    fn frame(held: u16, pressed: u16, released: u16) -> InputFrame {
        InputFrame {
            held,
            pressed,
            released,
        }
    }

    fn header() -> Header {
        Header {
            magic: MAGIC,
            replay_format: REPLAY_FORMAT,
            sim_version: 1,
            package_id: "rely_the_needle".to_string(),
            gameplay_hash: [0xab; 32],
            engine_rev: 1,
            rng_seed: 0x1234_5678_9abc_def0,
            start_mode: StartMode::NewGame,
            client_build_id: "test-build".to_string(),
        }
    }

    /// A consistent replay: splits at ticks 3 and 7 (those that fit) and a checkpoint on every
    /// interval tick from 0.
    fn replay(inputs: Vec<InputFrame>) -> Replay {
        let ticks = inputs.len() as u64;
        Replay {
            header: header(),
            trailer: Trailer {
                total_ticks: ticks,
                splits: [(0, 3), (1, 7)]
                    .into_iter()
                    .filter(|&(_, tick)| tick <= ticks)
                    .collect(),
                checkpoints: (0..=ticks)
                    .step_by(usize::try_from(CHECKPOINT_INTERVAL).expect("small"))
                    .map(|tick| (tick, [(tick / CHECKPOINT_INTERVAL).to_le_bytes()[0]; 16]))
                    .collect(),
                final_hash: [3; 32],
            },
            inputs,
        }
    }

    fn small() -> Replay {
        let mut inputs = vec![InputFrame::default(); 3];
        inputs.push(frame(JUMP, JUMP, 0));
        inputs.extend([frame(JUMP, 0, 0); 4]);
        inputs.push(frame(0, 0, JUMP));
        inputs.push(frame(0, JUMP, JUMP)); // sub-tick tap
        replay(inputs)
    }

    /// Encodes a replay without the checks in `encode`, as a buggy or hostile writer could.
    fn encode_unchecked(r: &Replay) -> Vec<u8> {
        let file = FileRef {
            header: &r.header,
            runs: runs(&r.inputs),
            trailer: &r.trailer,
        };
        postcard::to_allocvec(&file).unwrap()
    }

    /// Encodes a hand-made run list, bypassing the merging in `encode`.
    fn encode_runs(runs: Vec<(u32, InputFrame)>, total_ticks: u64) -> Vec<u8> {
        let r = small();
        let file = FileOwned {
            header: r.header,
            runs,
            trailer: Trailer {
                total_ticks,
                ..r.trailer
            },
        };
        postcard::to_allocvec(&file).unwrap()
    }

    /// Simulated play: change the held set, one edge tick, then hold for a while.
    fn realistic_inputs(ticks: usize, seed: u64) -> Vec<InputFrame> {
        let mut rng = Lcg(seed);
        let mut inputs = Vec::with_capacity(ticks);
        let mut held = 0u16;
        while inputs.len() < ticks {
            let mut next = 0u16;
            match rng.range(0, 3) {
                0 => next |= LEFT,
                1 => next |= RIGHT,
                _ => {}
            }
            if rng.range(0, 2) == 0 {
                next |= JUMP;
            }
            if rng.range(0, 3) == 0 {
                next |= SHOOT;
            }
            if next != held {
                inputs.push(frame(next, next & !held, held & !next));
                held = next;
            }
            for _ in 0..rng.range(4, 80) {
                inputs.push(frame(held, 0, 0));
            }
        }
        inputs.truncate(ticks);
        inputs
    }

    #[test]
    fn round_trip() {
        for r in [small(), replay(Vec::new()), replay(realistic_inputs(5_000, 7))] {
            let bytes = r.encode().unwrap();
            assert_eq!(&bytes[..4], b"IWBR");
            assert_eq!(Replay::decode(&bytes), Ok(r));
        }
    }

    #[test]
    fn round_trip_keeps_pause_mute_and_reset() {
        use crate::input::Action;
        let (p, m, r) = (Action::Pause.bit(), Action::Mute.bit(), Action::Reset.bit());
        let all = Action::MASK;
        let replay = replay(vec![
            frame(p, p, 0),
            frame(p | m, m, 0),
            frame(p | m | r, r, 0),
            frame(0, 0, p | m | r),
            frame(all, all, all),
        ]);
        let decoded = Replay::decode(&replay.encode().unwrap()).unwrap();
        assert_eq!(decoded, replay);
        assert_eq!(decoded.inputs[4].held, 0x7fff);
    }

    #[test]
    fn round_trip_from_save() {
        let mut r = small();
        r.header.start_mode = StartMode::FromSave(vec![9, 8, 7]);
        assert_eq!(Replay::decode(&r.encode().unwrap()), Ok(r));
    }

    #[test]
    fn identical_inputs_give_identical_bytes() {
        let a = replay(realistic_inputs(2_000, 11));
        let b = replay(realistic_inputs(2_000, 11));
        assert_eq!(a.encode().unwrap(), b.encode().unwrap());
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut bytes = small().encode().unwrap();
        bytes[0] = b'X';
        assert_eq!(Replay::decode(&bytes), Err(ReplayError::BadMagic));
        assert_eq!(Replay::decode(b""), Err(ReplayError::BadMagic));
        assert_eq!(Replay::decode(b"IWB"), Err(ReplayError::BadMagic));
    }

    #[test]
    fn rejects_unknown_format() {
        let mut r = small();
        r.header.replay_format = REPLAY_FORMAT + 1;
        let err = ReplayError::UnknownFormat(REPLAY_FORMAT + 1);
        assert_eq!(r.encode(), Err(err.clone()));
        assert_eq!(Replay::decode(&encode_unchecked(&r)), Err(err));
    }

    #[test]
    fn encode_rejects_wrong_magic() {
        let mut r = small();
        r.header.magic = [0; 4];
        assert_eq!(r.encode(), Err(ReplayError::BadMagic));
    }

    #[test]
    fn encode_rejects_default_trailer() {
        // A recorder that forgets total_ticks gets an error now, not a file decode rejects later.
        let mut r = small();
        r.trailer = Trailer {
            final_hash: [3; 32],
            ..Default::default()
        };
        let err = ReplayError::TickCountMismatch {
            runs: 10,
            total_ticks: 0,
        };
        assert_eq!(r.encode(), Err(err.clone()));
        assert_eq!(Replay::decode(&encode_unchecked(&r)), Err(err));
    }

    #[test]
    fn encode_rejects_too_many_ticks() {
        let ticks = MAX_REPLAY_TICKS + 1;
        let mut r = small();
        r.inputs = vec![InputFrame::default(); usize::try_from(ticks).expect("fits")];
        r.trailer.total_ticks = ticks;
        assert_eq!(r.encode(), Err(ReplayError::TooManyTicks));
    }

    #[test]
    fn rejects_split_past_last_tick() {
        let mut r = small();
        r.trailer.splits = vec![(0, 3), (1, 11)];
        let err = ReplayError::BadSplit(1);
        assert_eq!(r.encode(), Err(err.clone()));
        assert_eq!(Replay::decode(&encode_unchecked(&r)), Err(err));
    }

    #[test]
    fn rejects_splits_out_of_order() {
        let mut r = small();
        r.trailer.splits = vec![(0, 7), (1, 3)];
        let err = ReplayError::BadSplit(1);
        assert_eq!(r.encode(), Err(err.clone()));
        assert_eq!(Replay::decode(&encode_unchecked(&r)), Err(err));
    }

    #[test]
    fn accepts_splits_on_one_tick_and_on_the_last_tick() {
        let mut r = small();
        r.trailer.splits = vec![(0, 4), (1, 4), (2, 10)];
        assert_eq!(Replay::decode(&r.encode().unwrap()), Ok(r));
    }

    #[test]
    fn rejects_checkpoint_past_last_tick() {
        let mut r = replay(realistic_inputs(1_200, 5));
        r.trailer.checkpoints.push((1_500, [0; 16]));
        let err = ReplayError::BadCheckpoint(3);
        assert_eq!(r.encode(), Err(err.clone()));
        assert_eq!(Replay::decode(&encode_unchecked(&r)), Err(err));
    }

    #[test]
    fn rejects_checkpoint_off_the_grid() {
        let mut r = replay(realistic_inputs(1_200, 5));
        r.trailer.checkpoints[1].0 = 499;
        let err = ReplayError::BadCheckpoint(1);
        assert_eq!(r.encode(), Err(err.clone()));
        assert_eq!(Replay::decode(&encode_unchecked(&r)), Err(err));
    }

    #[test]
    fn rejects_repeated_checkpoint() {
        let mut r = replay(realistic_inputs(1_200, 5));
        r.trailer.checkpoints[2].0 = 500;
        let err = ReplayError::BadCheckpoint(2);
        assert_eq!(r.encode(), Err(err.clone()));
        assert_eq!(Replay::decode(&encode_unchecked(&r)), Err(err));
    }

    #[test]
    fn rejects_checkpoints_out_of_order() {
        let mut r = replay(realistic_inputs(1_200, 5));
        r.trailer.checkpoints.swap(1, 2);
        let err = ReplayError::BadCheckpoint(2);
        assert_eq!(r.encode(), Err(err.clone()));
        assert_eq!(Replay::decode(&encode_unchecked(&r)), Err(err));
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut bytes = small().encode().unwrap();
        bytes.extend([0, 0]);
        assert_eq!(Replay::decode(&bytes), Err(ReplayError::TrailingBytes(2)));
    }

    #[test]
    fn rejects_truncation() {
        let bytes = small().encode().unwrap();
        for len in 4..bytes.len() {
            assert!(Replay::decode(&bytes[..len]).is_err(), "length {len}");
        }
    }

    #[test]
    fn rejects_zero_length_run() {
        let a = frame(0, 0, 0);
        let b = frame(JUMP, JUMP, 0);
        let bytes = encode_runs(vec![(2, a), (0, b), (3, a)], 5);
        assert_eq!(Replay::decode(&bytes), Err(ReplayError::ZeroLengthRun(1)));
    }

    #[test]
    fn rejects_repeated_frame() {
        let a = frame(LEFT, 0, 0);
        let bytes = encode_runs(vec![(2, a), (3, a)], 5);
        assert_eq!(Replay::decode(&bytes), Err(ReplayError::RepeatedFrame(1)));
    }

    #[test]
    fn rejects_tick_count_mismatch() {
        let bytes = encode_runs(vec![(2, frame(0, 0, 0))], 3);
        assert_eq!(
            Replay::decode(&bytes),
            Err(ReplayError::TickCountMismatch {
                runs: 2,
                total_ticks: 3
            })
        );
    }

    #[test]
    fn rejects_too_many_ticks() {
        let a = frame(0, 0, 0);
        let b = frame(RIGHT, 0, 0);
        let bytes = encode_runs(vec![(u32::MAX, a), (u32::MAX, b)], 0);
        assert_eq!(Replay::decode(&bytes), Err(ReplayError::TooManyTicks));
    }

    #[test]
    fn rejects_overlong_varint() {
        // replay_format 1 is the single byte 0x01 right after the magic; 0x81 0x00 also reads
        // as 1 but is not what encode writes.
        let bytes = small().encode().unwrap();
        assert_eq!(bytes[4], 0x01);
        let mut long = bytes[..4].to_vec();
        long.extend([0x81, 0x00]);
        long.extend(&bytes[5..]);
        assert_eq!(Replay::decode(&long), Err(ReplayError::NotCanonical));
    }

    #[test]
    fn hour_long_replay_stays_small() {
        let r = replay(realistic_inputs(180_000, 42));
        let bytes = r.encode().unwrap();
        assert!(bytes.len() < 64 * 1024, "{} bytes", bytes.len());
        assert_eq!(Replay::decode(&bytes), Ok(r));
    }

    #[test]
    fn random_bytes_never_panic() {
        let mut rng = Lcg(0x5eed);
        let valid = replay(realistic_inputs(3_000, 3)).encode().unwrap();
        for i in 0..20_000 {
            let bytes: Vec<u8> = match i % 3 {
                // Pure noise.
                0 => (0..rng.range(0, 256)).map(|_| rng.byte()).collect(),
                // Valid prefix, noise after it.
                1 => {
                    let mut b = vec![b'I', b'W', b'B', b'R', 0x01];
                    b.extend((0..rng.range(0, 256)).map(|_| rng.byte()));
                    b
                }
                // A valid file with a few bytes flipped and maybe truncated.
                _ => {
                    let mut b = valid.clone();
                    for _ in 0..rng.range(1, 4) {
                        let at = rng.index(b.len() - 1);
                        b[at] = rng.byte();
                    }
                    if rng.range(0, 1) == 0 {
                        b.truncate(rng.index(b.len()));
                    }
                    b
                }
            };
            if let Ok(r) = Replay::decode(&bytes) {
                assert_eq!(r.encode(), Ok(bytes), "accepted bytes must be canonical");
            }
        }
    }

    #[test]
    fn dump_prints_one_run_per_line() {
        let text = small().dump();
        let runs: Vec<&str> = text.lines().filter(|l| l.starts_with("ticks ")).collect();
        assert_eq!(
            runs,
            [
                "ticks 0-2 (3): held 0x0000 pressed 0x0000 released 0x0000",
                "ticks 3-3 (1): held 0x0010 pressed 0x0010 released 0x0000",
                "ticks 4-7 (4): held 0x0010 pressed 0x0000 released 0x0000",
                "ticks 8-8 (1): held 0x0000 pressed 0x0000 released 0x0010",
                "ticks 9-9 (1): held 0x0000 pressed 0x0010 released 0x0010",
            ]
        );
        assert!(text.contains("magic \"IWBR\"\n"));
        assert!(text.contains("package_id \"rely_the_needle\"\n"));
        assert!(text.contains("start_mode new_game\n"));
        assert!(text.contains("total_ticks 10\n"));
        assert!(text.contains("split stage 1 tick 7\n"));
        assert!(text.contains(&format!("final_hash {}\n", "03".repeat(32))));
    }
}
