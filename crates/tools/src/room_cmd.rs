//! `iwbtg room show <pkg> <room>`: an ASCII map of a room, so an agent or a reviewer can see it
//! without the client (docs/decisions/0002-data-syntax.md, Grafted in).
//!
//! One character stands for a `--cell` pixel square (32 by default, the grid the rooms are built
//! on). Each placed instance goes into the cell of its position; where several share a cell the
//! one earlier in [`GLYPHS`] shows. The first line names the room and the scale, the last is the
//! legend of the characters that appear.

use std::path::Path;

use iwbtg_format::{LoadMode, load_game};
use iwbtg_sim::def::{Builtin, GameDef, KindId, ParamValue, Params, RoomDef};

use crate::trace_cmd::{ToolError, load_package};

/// What a character means, in drawing priority (first wins a shared cell).
const GLYPHS: [(char, &str); 17] = [
    ('@', "kid start"),
    ('S', "save"),
    ('W', "warp"),
    ('D', "difficulty warp"),
    ('E', "edge warp"),
    ('^', "spike up"),
    ('v', "spike down"),
    ('<', "spike left"),
    ('>', "spike right"),
    ('*', "killer"),
    ('#', "block"),
    ('=', "platform"),
    ('/', "slope"),
    ('~', "water"),
    ('|', "vine"),
    ('o', "other built-in or object"),
    ('.', "empty"),
];

/// The glyph of one placed instance.
fn glyph(def: &GameDef, kind: KindId, params: &Params) -> char {
    let Some(k) = usize::try_from(kind.0).ok().and_then(|i| def.kinds.get(i)) else {
        return 'o';
    };
    match k.base {
        Builtin::KidStart => '@',
        Builtin::Save => 'S',
        Builtin::Warp => 'W',
        Builtin::DifficultyWarp => 'D',
        Builtin::EdgeWarp => 'E',
        Builtin::Spike => {
            let dir = params.0.get("dir").or_else(|| k.params.0.get("dir"));
            match dir {
                Some(ParamValue::Str(d)) if d == "down" => 'v',
                Some(ParamValue::Str(d)) if d == "left" => '<',
                Some(ParamValue::Str(d)) if d == "right" => '>',
                _ => '^',
            }
        }
        Builtin::Killer => '*',
        Builtin::Block => '#',
        Builtin::Platform => '=',
        Builtin::Slope => '/',
        Builtin::Water => '~',
        Builtin::Vine => '|',
        _ => 'o',
    }
}

fn priority(c: char) -> usize {
    GLYPHS.iter().position(|(g, _)| *g == c).unwrap_or(GLYPHS.len())
}

/// The map of `room`: the header, one line per row of cells, the legend.
pub fn room_map(def: &GameDef, room: &RoomDef, cell: u32) -> String {
    let cell = cell.max(1);
    let cols = room.width.div_ceil(cell).max(1) as usize;
    let rows = room.height.div_ceil(cell).max(1) as usize;
    let mut grid = vec![vec!['.'; cols]; rows];
    let mut outside = 0usize;
    for inst in &room.instances {
        let (cx, cy) = (
            (inst.x.get() / f64::from(cell)).floor(),
            (inst.y.get() / f64::from(cell)).floor(),
        );
        let inside = cx >= 0.0 && cy >= 0.0 && cx < cols as f64 && cy < rows as f64;
        if !inside {
            outside += 1;
            continue;
        }
        let g = glyph(def, inst.kind, &inst.params);
        let slot = &mut grid[cy as usize][cx as usize];
        if priority(g) < priority(*slot) {
            *slot = g;
        }
    }
    let mut out = format!(
        "room {} ({} x {} px, {} instances), 1 character = {cell} x {cell} px\n",
        room.name,
        room.width,
        room.height,
        room.instances.len()
    );
    let used: Vec<char> = grid.iter().flatten().copied().collect();
    for row in &grid {
        out.extend(row.iter());
        out.push('\n');
    }
    let legend: Vec<String> = GLYPHS
        .iter()
        .filter(|(g, _)| *g != '.' && used.contains(g))
        .map(|(g, what)| format!("{g} {what}"))
        .collect();
    out.push_str(&format!("legend: {}, . empty", legend.join(", ")));
    if outside > 0 {
        out.push_str(&format!(" ({outside} outside the room)"));
    }
    out.push('\n');
    out
}

/// `iwbtg room show <pkg> <room> [--cell N] [--trace-mode]`.
pub fn show_main(dir: &Path, room: &str, cell: u32, trace_mode: bool) -> Result<bool, ToolError> {
    let files = load_package(dir)?;
    let mode = if trace_mode {
        LoadMode::Trace {
            unmapped_kinds_inert: true,
        }
    } else {
        LoadMode::Strict
    };
    let def = load_game(&files, mode).map_err(|diags| {
        ToolError::Message(format!(
            "the package does not load:\n  {}",
            diags
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n  ")
        ))
    })?;
    let Some(r) = def.rooms.iter().find(|r| r.name == room) else {
        let names: Vec<&str> = def.rooms.iter().map(|r| r.name.as_str()).collect();
        return Err(ToolError::Message(format!(
            "no room `{room}`; the package has: {}",
            names.join(", ")
        )));
    };
    print!("{}", room_map(&def, r, cell));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iwbtg_sim::def::testing::GameDefBuilder;
    use iwbtg_sim::def::{KindDef, RoomDef};

    /// A 128 x 96 room (4 x 3 cells) with these instances: `(kind name, base, x, y, spike dir)`.
    fn def_with(instances: &[(&str, Builtin, f64, f64, Option<&str>)]) -> GameDef {
        let mut b = GameDefBuilder::new("map");
        let room = b.room(RoomDef::new("r", 128, 96));
        let mut ids = std::collections::BTreeMap::new();
        for (name, base, ..) in instances {
            ids.entry(*name)
                .or_insert_with(|| b.kind(KindDef::new(*name, *base)));
        }
        for (name, _, x, y, _) in instances {
            b.place(room, ids[name], *x, *y).unwrap();
        }
        let mut def = b.build().unwrap();
        for (i, (_, _, _, _, dir)) in instances.iter().enumerate() {
            if let Some(d) = dir {
                def.rooms[0].instances[i]
                    .params
                    .0
                    .insert("dir".to_owned(), ParamValue::Str((*d).to_owned()));
            }
        }
        def
    }

    #[test]
    fn blocks_spikes_saves_warps_and_the_kid_start_are_characters() {
        let def = def_with(&[
            ("kid_start", Builtin::KidStart, 32.0, 64.0, None),
            ("block", Builtin::Block, 0.0, 64.0, None),
            ("block", Builtin::Block, 0.0, 96.0, None),
            ("spike", Builtin::Spike, 64.0, 64.0, Some("down")),
            ("spike", Builtin::Spike, 96.0, 64.0, None),
            ("save", Builtin::Save, 96.0, 32.0, None),
            ("warp", Builtin::Warp, 64.0, 0.0, None),
        ]);
        let map = room_map(&def, &def.rooms[0], 32);
        let lines: Vec<&str> = map.lines().collect();
        assert!(
            lines[0].starts_with("room r (128 x 96 px, 7 instances)"),
            "{}",
            lines[0]
        );
        assert_eq!(lines[1], "..W.");
        assert_eq!(lines[2], "...S");
        assert_eq!(lines[3], "#@v^");
        assert_eq!(
            lines[4],
            "legend: @ kid start, S save, W warp, ^ spike up, v spike down, # block, . empty (1 outside the room)"
        );
    }

    #[test]
    fn the_stronger_glyph_wins_a_shared_cell() {
        let def = def_with(&[
            ("block", Builtin::Block, 0.0, 0.0, None),
            ("save", Builtin::Save, 5.0, 5.0, None),
        ]);
        let map = room_map(&def, &def.rooms[0], 32);
        assert_eq!(map.lines().nth(1), Some("S..."));
    }
}
