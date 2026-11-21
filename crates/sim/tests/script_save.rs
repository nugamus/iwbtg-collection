//! The save/script boundary (#1202 C2): a save payload is data, never script.
//!
//! The original ports saved GML text and loaded it with `execute_file` (CLONE_SPEC 862, 1049).
//! The modern decoder only returns a value or a data error; there is no path from save bytes to
//! the script host. The strict-decode and shared client/replay validator work is #1325.

use iwbtg_sim::def::RoomDef;
use iwbtg_sim::def::testing::GameDefBuilder;
use iwbtg_sim::num::Real;
use iwbtg_sim::save::{Checkpoint, CheckpointError, ProfileId, RoomKey, RulesetId, SavePrecision};

#[test]
fn a_gml_or_script_payload_is_a_data_error() {
    for payload in [
        "global.save = 1;\nexecute_file(\"save.sav\");\n",
        "// machinary: the original wrote its save as GML text\nshow_message(\"saved\");\n",
        "\u{0}\u{1}not a checkpoint",
    ] {
        match Checkpoint::decode(payload.as_bytes()) {
            Err(CheckpointError::Corrupt(_) | CheckpointError::Format(_)) => {}
            other => panic!("{payload:?} decoded as {other:?}, not a data error"),
        }
    }
}

#[test]
fn a_text_field_is_a_room_name_not_a_script() {
    // A payload that does decode is still data: a room field holding GML text is one room name
    // and is refused because no such room exists. Nothing parses or runs it.
    let text = "execute_file(\"save.sav\")";
    let checkpoint = Checkpoint {
        format: Checkpoint::FORMAT,
        ruleset: RulesetId("gm8@0".into()),
        profile: ProfileId("gm8/a1@1".into()),
        room: RoomKey(text.into()),
        point: None,
        x: Real::new(32.0),
        y: Real::new(64.0),
        precision: SavePrecision::XyBase100Bytes,
        flipped: false,
        facing: 1,
        difficulty: 0,
        deaths: 0,
        time: 0,
        bosses: 0,
        items: 0,
        clear: false,
    };
    let bytes = checkpoint.encode().expect("encode");
    let decoded = Checkpoint::decode(&bytes).expect("it is valid data");
    assert_eq!(decoded.room.0, text);

    let mut b = GameDefBuilder::new("save-data");
    let room = b.room(RoomDef::new("play", 320, 240));
    b.start_room(room);
    let def = b.build().expect("def");
    assert_eq!(
        decoded.to_slot(&def),
        Err(CheckpointError::NoRoom(text.into())),
        "the text is a room name that does not exist"
    );
}
