//! Checks against the recorded CSVs (docs/architecture/testing.md). They need IWBTG_RESEARCH
//! set to the research repo and skip with a note when it is unset.
#![allow(
    clippy::disallowed_methods,
    reason = "tests read the research path and trace files; the sim itself never does"
)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use iwbtg_sim::def::RoomDef;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::motion::{Motion, apply_auto_motion};
use iwbtg_sim::num::{self, Real};
use iwbtg_sim::rng::Rng;
use iwbtg_sim::rules::{MotionRules, NumRules};

fn r(x: f64) -> Real {
    Real::new(x)
}

/// One recorded CSV: the header and the rows, split on commas outside quotes.
struct Csv {
    name: String,
    header: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Csv {
    fn col(&self, name: &str) -> usize {
        self.header
            .iter()
            .position(|h| h == name)
            .unwrap_or_else(|| panic!("{} has no column {name}", self.name))
    }

    fn real(&self, row: &[String], col: usize) -> Real {
        r(row[col]
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("{}: {:?} is not a real", self.name, row[col])))
    }
}

fn split(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    out.push(field);
    out
}

/// The trace `game/case.csv`, or None (with a note) when IWBTG_RESEARCH is unset.
fn trace(game: &str, case: &str) -> Option<Csv> {
    let Some(root) = std::env::var_os("IWBTG_RESEARCH") else {
        eprintln!("IWBTG_RESEARCH is unset; skipping {game}/{case}");
        return None;
    };
    let path = PathBuf::from(root)
        .join("specs/engines/gm8/traces")
        .join(game)
        .join(case);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("IWBTG_RESEARCH is set but {} is unreadable: {e}", path.display()));
    let mut lines = text.lines();
    let header = split(lines.next().expect("header"));
    let rows = lines.filter(|l| !l.is_empty()).map(split).collect();
    Some(Csv {
        name: format!("{game}/{case}"),
        header,
        rows,
    })
}

fn one_room_game() -> iwbtg_sim::GameDef {
    let mut b = GameDefBuilder::new("wp2");
    b.room(RoomDef::new("room", 800, 608));
    b.build().expect("one-room game builds")
}

/// showdown/t-mot-01.csv: a plain fall from (113, 215). Step resets hspeed and clamps
/// `abs(vspeed) > 9`; automatic motion adds gravity and moves. Every falling row's y, hspeed
/// and vspeed match bit for bit.
#[test]
fn fall_table_matches_t_mot_01() {
    let Some(csv) = trace("showdown", "t-mot-01.csv") else {
        return;
    };
    let (y, hs, vs) = (csv.col("y"), csv.col("hspeed"), csv.col("vspeed"));
    let def = one_room_game();
    let n = NumRules::default();
    let mut m = Motion::new_at(r(113.0), r(215.0));
    let mut checked = 0;
    for row in &csv.rows {
        if csv.real(row, vs) == r(0.0) {
            break; // landed
        }
        m.set_gravity(r(0.4)).unwrap();
        m.set_hspeed(r(0.0)).unwrap();
        let v = m.vspeed();
        if num::gm_gt(num::abs(v), r(9.0), &n) {
            m.set_vspeed(num::sign(v, &n) * r(9.0)).unwrap();
        }
        apply_auto_motion(&mut m, None, &def, &MotionRules::default()).unwrap();
        let frame = &row[0];
        assert_eq!(m.y(), csv.real(row, y), "y on frame {frame}");
        assert_eq!(m.hspeed(), csv.real(row, hs), "hspeed on frame {frame}");
        assert_eq!(m.vspeed(), csv.real(row, vs), "vspeed on frame {frame}");
        checked += 1;
    }
    assert!(checked > 40, "only {checked} falling rows");
}

/// little-trap/num-eps-slip.csv: every row's x and hspeed under the default rules (the same
/// model as slip.rs, checked on all 200 rows).
#[test]
fn slip_run_matches_num_eps_slip() {
    let Some(csv) = trace("little-trap", "num-eps-slip.csv") else {
        return;
    };
    let (x, hs) = (csv.col("x"), csv.col("hspeed"));
    let def = one_room_game();
    let n = NumRules::default();
    let mut m = Motion::new_at(r(145.0), r(567.4));
    for row in &csv.rows {
        let frame: u32 = row[0].parse().expect("frame number");
        let d: i8 = if (12..=26).contains(&frame) { -1 } else { 0 };
        if frame > 2 {
            let h = m.hspeed();
            let next = match d {
                -1 if num::gm_gt(h, r(-3.0), &n) => h - r(0.2),
                0 if num::gm_ne(h, r(0.0), &n) => h - r(0.2) * num::sign(h, &n),
                _ => h,
            };
            m.set_hspeed(next).unwrap();
        }
        m.set_gravity(r(0.4)).unwrap();
        apply_auto_motion(&mut m, None, &def, &MotionRules::default()).unwrap();
        m.set_y(r(567.4)).unwrap();
        m.set_vspeed(r(0.0)).unwrap();
        assert_eq!(m.x(), csv.real(row, x), "x on frame {frame}");
        assert_eq!(m.hspeed(), csv.real(row, hs), "hspeed on frame {frame}");
    }
    assert_eq!(csv.rows.len(), 200);
}

/// num-trig-a.csv: on every row of every shot with a nonzero vector, writing hspeed and
/// vspeed gives the recorded speed and direction (M5).
#[test]
fn polar_readback_matches_num_trig() {
    let Some(csv) = trace("little-runner", "num-trig-a.csv") else {
        return;
    };
    let cols = ["hspeed", "vspeed", "speed", "direction"].map(|c| csv.col(c));
    let mut checked = 0;
    for row in &csv.rows {
        let [h, v, s, d] = cols.map(|c| csv.real(row, c));
        if h == r(0.0) && v == r(0.0) {
            continue; // a shot before it aims: the untouched defaults
        }
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.set_hspeed(h).unwrap();
        m.set_vspeed(v).unwrap();
        assert_eq!(
            (m.speed(), m.direction()),
            (s, d),
            "frame {} id {}",
            row[0],
            row[3]
        );
        checked += 1;
    }
    assert!(checked > 3000, "only {checked} rows");
}

/// animus/bounce-snap.csv and bounce-snap-b.csv (research#193): every probe's hspeed and
/// vspeed after its speed and direction writes, in the recorded write order, bit for bit
/// (sign of zero included): the integer snap of `num::snap_to_int`.
#[test]
fn speed_and_direction_writes_snap_like_bounce_snap() {
    for case in ["bounce-snap.csv", "bounce-snap-b.csv"] {
        let Some(csv) = trace("animus", case) else {
            return;
        };
        let (hs, vs, sp) = (csv.col("hspeed"), csv.col("vspeed"), csv.col("speed"));
        let dd = csv.header.iter().position(|h| h == "dd");
        let via = csv.header.iter().position(|h| h == "via");
        for row in &csv.rows {
            let speed = csv.real(row, sp);
            let mut m = Motion::new_at(r(0.0), r(0.0));
            match (dd, via) {
                (Some(dd), Some(via)) if row[via] == "1.0" => {
                    m.set_direction(csv.real(row, dd)).unwrap();
                    m.set_speed(speed).unwrap();
                }
                (Some(dd), _) => {
                    m.set_speed(speed).unwrap();
                    m.set_direction(csv.real(row, dd)).unwrap();
                }
                (None, _) => m.set_speed(speed).unwrap(),
            }
            let bits = |x: Real| x.get().to_bits();
            assert_eq!(
                (bits(m.hspeed()), bits(m.vspeed())),
                (bits(csv.real(row, hs)), bits(csv.real(row, vs))),
                "{case} id {}: speed {speed:?}",
                row[3]
            );
        }
        assert_eq!(csv.rows.len(), if dd.is_some() { 1024 } else { 800 }, "{case}");
    }
}

/// animus/motion-snap-*.csv (research#199, CLONE_SPEC "Where the snap applies"): every probe's
/// hspeed, vspeed, speed and direction bit for bit, sign of zero included. Speed and direction
/// writes, motion_set (the D&D Move Free action is the same call) and friction snap; motion_add
/// and hspeed/vspeed writes do not; the direction read-back snaps. Allowed: the M6 libm gaps
/// (510 friction rows at direction 45 one or two ulps off in h or v, one atan2 read-back in
/// motion-snap-add one ulp off).
#[test]
fn motion_snap_probes_match() {
    const COLS: [&str; 4] = ["hspeed", "vspeed", "speed", "direction"];
    let bits = |x: Real| x.get().to_bits();
    let state = |m: &Motion| [m.hspeed(), m.vspeed(), m.speed(), m.direction()].map(bits);
    let mut misses: Vec<String> = Vec::new();
    let (mut total, mut m6) = (0, 0);
    let mut check = |csv: &Csv, row: &[String], m: &Motion| {
        total += 1;
        let got = state(m);
        let exp = COLS.map(|c| bits(csv.real(row, csv.col(c))));
        let ulps = [0, 1, 2, 3].map(|i| got[i].abs_diff(exp[i]));
        // M6 (num/trig.rs, research#154): our libm is one ulp from the recorder's at sin 45,
        // cos 220 and some atan2 values; times a speed that is up to two ulps in h or v.
        let gap_dir = [45.0f64, 220.0].iter().any(|a| got[3] == a.to_bits());
        if got == exp {
        } else if (gap_dir && ulps[0] + ulps[1] <= 2 && ulps[2] + ulps[3] == 0)
            || (ulps[0] + ulps[1] + ulps[2] == 0 && ulps[3] == 1)
        {
            m6 += 1;
        } else {
            misses.push(format!(
                "{} id {} frame {}: {:?} vs {:?}",
                csv.name,
                row[3],
                row[0],
                got.map(f64::from_bits),
                exp.map(f64::from_bits)
            ));
        }
    };

    let Some(set) = trace("animus", "motion-snap-set.csv") else {
        return;
    };
    let (mc, sp, dd) = (set.col("m"), set.col("sp"), set.col("dd"));
    for row in &set.rows {
        let (s, d) = (set.real(row, sp), set.real(row, dd));
        let mut m = Motion::new_at(r(0.0), r(0.0));
        if row[mc] == "0.0" {
            m.set_direction(d).unwrap();
            m.set_speed(s).unwrap();
        } else {
            m.motion_set(d, s).unwrap();
        }
        check(&set, row, &m);
    }

    let Some(add) = trace("animus", "motion-snap-add.csv") else {
        return;
    };
    let [st, sp, dd, h0, v0, s0, d0] = ["st", "sp", "dd", "h0", "v0", "s0", "d0"].map(|c| add.col(c));
    for row in &add.rows {
        let mut m = Motion::new_at(r(0.0), r(0.0));
        match row[st].as_str() {
            "0.0" => {}
            // hspeed and vspeed writes; the recorded h0 and v0, because the recorder parses
            // the GML literal -0.000002 one ulp from the nearest double (N14).
            "1.0" | "2.0" => {
                m.set_hspeed(add.real(row, h0)).unwrap();
                m.set_vspeed(add.real(row, v0)).unwrap();
            }
            _ => {
                m.set_speed(r(8.0)).unwrap();
                m.set_direction(r(60.0)).unwrap();
            }
        }
        let start = [h0, v0, s0, d0].map(|c| bits(add.real(row, c)));
        assert_eq!(state(&m), start, "motion-snap-add id {}: start state", row[3]);
        m.motion_add(add.real(row, dd), add.real(row, sp)).unwrap();
        check(&add, row, &m);
    }

    let Some(hv) = trace("animus", "motion-snap-hv.csv") else {
        return;
    };
    let (hw, vw) = (hv.col("hw"), hv.col("vw"));
    for row in &hv.rows {
        let mut m = Motion::new_at(r(0.0), r(0.0));
        m.set_hspeed(hv.real(row, hw)).unwrap();
        m.set_vspeed(hv.real(row, vw)).unwrap();
        check(&hv, row, &m);
    }

    // Friction: two rows per probe (frames 3 and 4), one tick of automatic motion each.
    let Some(fric) = trace("animus", "motion-snap-fric.csv") else {
        return;
    };
    let [fc, sp, dd, id] = ["f", "sp", "dd", "id"].map(|c| fric.col(c));
    let def = one_room_game();
    let mut probes: BTreeMap<String, Motion> = BTreeMap::new();
    for row in &fric.rows {
        let m = probes.entry(row[id].clone()).or_insert_with(|| {
            let mut m = Motion::new_at(r(0.0), r(0.0));
            m.set_direction(fric.real(row, dd)).unwrap();
            m.set_speed(fric.real(row, sp)).unwrap();
            m.set_friction(fric.real(row, fc)).unwrap();
            m
        });
        apply_auto_motion(m, None, &def, &MotionRules::default()).unwrap();
        check(&fric, row, m);
    }

    eprintln!("motion-snap: {total} rows, {m6} at the M6 libm gaps");
    assert_eq!(total, 3456 + 4608 + 512 + 4096);
    assert!(
        misses.is_empty(),
        "{} rows differ:\n{}",
        misses.len(),
        misses.join("\n")
    );
}

/// num-rng-a.csv: every change of random_get_seed() between rows is a whole number of LCG
/// steps (N9).
#[test]
fn seeds_in_num_rng_a_are_lcg_steps() {
    let Some(csv) = trace("rely-the-needle-4-2", "num-rng-a.csv") else {
        return;
    };
    let col = csv.col("random_get_seed()");
    let seeds: Vec<u32> = csv
        .rows
        .iter()
        .map(|row| {
            let signed = csv.real(row, col).get();
            // Signed 32-bit seeds are integers; reinterpret as unsigned.
            let i = num::to_int_trunc(r(signed)).expect("integral seed");
            u32::try_from(i.rem_euclid(1 << 32)).expect("32-bit seed")
        })
        .collect();
    for pair in seeds.windows(2) {
        let mut rng = Rng::new(pair[0]);
        let reached = (0..=2000).any(|_| {
            let hit = rng.seed == pair[1];
            rng.next_unit();
            hit
        });
        assert!(
            reached,
            "seed {} does not reach {} within 2000 steps",
            pair[0], pair[1]
        );
    }
    assert!(seeds.len() > 40);
}

/// num-rng-b.csv: all 40 blood of frame 15, drawn per blood in the order choose(0,1,2),
/// gravity 0.1 + random(0.2), direction irandom(35) * 10, speed random(6), from the frame-14
/// seed -1390856633, in id order.
#[test]
fn blood_of_num_rng_b() {
    let Some(csv) = trace("rely-the-needle-4-2", "num-rng-b.csv") else {
        return;
    };
    let (id, img, grav, dir, spd) = (
        csv.col("id"),
        csv.col("cr_img"),
        csv.col("cr_grav"),
        csv.col("cr_dir"),
        csv.col("cr_spd"),
    );
    let mut first: BTreeMap<u64, &Vec<String>> = BTreeMap::new();
    for row in csv.rows.iter().filter(|row| row[0] == "15") {
        first.entry(row[id].parse().expect("id")).or_insert(row);
    }
    assert_eq!(first.len(), 40);
    let mut rng = Rng::new(u32::try_from((-1_390_856_633i64).rem_euclid(1 << 32)).unwrap());
    for (inst, row) in first {
        let image = rng.choose(3).unwrap();
        let gravity = r(0.1) + rng.random(r(0.2));
        let direction = rng.irandom(r(35.0)).unwrap() * r(10.0);
        let speed = rng.random(r(6.0));
        assert_eq!(r(f64::from(image)), csv.real(row, img), "image of {inst}");
        assert_eq!(gravity, csv.real(row, grav), "gravity of {inst}");
        assert_eq!(direction, csv.real(row, dir), "direction of {inst}");
        assert_eq!(speed, csv.real(row, spd), "speed of {inst}");
    }
}

/// go-the-jun-g-world/num-eps-alpha.csv: g peaks at 0.8000000000000004 and ends at
/// -6.245004513516506e-17 (the sequence itself is in num.rs).
#[test]
fn eps_alpha_values_are_in_the_trace() {
    let Some(csv) = trace("go-the-jun-g-world", "num-eps-alpha.csv") else {
        return;
    };
    let g = csv.col("g");
    let values: Vec<Real> = csv.rows.iter().map(|row| csv.real(row, g)).collect();
    assert!(values.contains(&r(0.8000000000000004)));
    assert_eq!(values.last(), Some(&r(-6.245004513516506e-17)));
}
