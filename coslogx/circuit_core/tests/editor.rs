use circuit_core::{editor::*, kind::Kind, params, project::*};

fn ed() -> Editor { Editor::new(Project::empty()) }
fn place(e: &mut Editor, k: Kind, x: f64, y: f64) { e.set_tool(Tool::Place(k)); e.pointer_down((x, y), false, 1.0); e.pointer_up(); e.set_tool(Tool::Select); }
fn click(e: &mut Editor, x: f64, y: f64) { e.pointer_down((x, y), false, 1.0); e.pointer_up(); }
fn netlist_nodes(e: &Editor) -> usize { circuit_core::netlist::build(&e.proj, 1).n_nodes }

#[test]
fn placement_snaps_names_and_applies_defaults() {
    let mut e = ed();
    place(&mut e, Kind::R, 103.0, 58.0);
    place(&mut e, Kind::R, 303.0, 58.0);
    place(&mut e, Kind::Npn, 500.0, 100.0);
    place(&mut e, Kind::Gnd, 600.0, 100.0);
    assert_eq!((e.proj.c[0].x, e.proj.c[0].y), (100.0, 60.0));
    assert_eq!(e.proj.c[0].n.as_deref(), Some("R1"));
    assert_eq!(e.proj.c[1].n.as_deref(), Some("R2"));
    assert_eq!(e.proj.c[2].p.as_deref(), Some("100")); // BJT gain default
    assert_eq!(e.proj.c[0].q.as_deref(), Some("0.25")); // resistor power rating default
    assert!(e.proj.c[3].n.is_none()); // ground has no designator
    e.proj.c.remove(0); e.sc.remove(0); // free R1: next R gets R1 again
    assert_eq!(e.next_name(Kind::R).as_deref(), Some("R1"));
    assert_eq!(e.recent[0], Kind::Gnd);
    assert_eq!(e.recent.len(), 3);
}

#[test]
fn placement_rotation_and_mirror_preview_apply() {
    let mut e = ed();
    e.set_tool(Tool::Place(Kind::Npn));
    e.rotate_sel(false); e.rotate_sel(false); e.mirror_sel(false);
    e.pointer_down((0.0, 0.0), false, 1.0); e.pointer_up();
    assert_eq!((e.proj.c[0].r, e.proj.c[0].m), (2, 1));
    e.set_tool(Tool::Place(Kind::Os)); e.rotate_sel(false); e.mirror_sel(false);
    e.pointer_down((200.0, 0.0), false, 1.0); e.pointer_up();
    assert_eq!(e.proj.c[1].m, 0); // scope never mirrors
}

#[test]
fn manual_wire_makes_manhattan_l_shape_and_chains() {
    let mut e = ed();
    e.set_tool(Tool::Wire);
    click(&mut e, 0.0, 0.0);
    click(&mut e, 100.0, 60.0);
    assert_eq!(e.proj.w.len(), 2);
    assert_eq!(e.proj.w[0], Wire { x1: 0.0, y1: 0.0, x2: 100.0, y2: 0.0 });
    assert_eq!(e.proj.w[1], Wire { x1: 100.0, y1: 0.0, x2: 100.0, y2: 60.0 });
    assert_eq!(e.wire_start, Some((100.0, 60.0)));      // chain continues
    click(&mut e, 100.0, 120.0);                         // straight segment, single wire
    assert_eq!(e.proj.w.len(), 3);
    e.right_click();                                     // first right click cancels the chain
    assert!(e.wire_start.is_none() && e.tool == Tool::Wire);
    e.right_click();
    assert_eq!(e.tool, Tool::Select);
}

#[test]
fn clicking_a_pin_autowires_and_ends_on_connection() {
    let mut e = ed();
    place(&mut e, Kind::R, 100.0, 100.0);   // pins (60,100) (140,100)
    place(&mut e, Kind::R, 300.0, 100.0);   // pins (260,100) (340,100)
    e.pointer_down((141.0, 101.0), false, 1.0); e.pointer_up();
    assert_eq!(e.tool, Tool::Wire); assert!(e.auto_wire);
    assert_eq!(e.wire_start, Some((140.0, 100.0)));
    click(&mut e, 200.0, 160.0);                 // free point: keep wiring
    assert_eq!(e.tool, Tool::Wire);
    click(&mut e, 260.0, 100.0);                 // onto the second resistor's pin: done
    assert_eq!(e.tool, Tool::Select);
    assert!(!e.auto_wire);
    // all four pins are separate nodes except the two joined by the new wires
    let n = netlist_nodes(&e);
    assert_eq!(n, 1 + 3); // resistor pin, joined pair, other pin (no ground -> node 0 unused)
}

#[test]
fn window_vs_crossing_selection() {
    let mut e = ed();
    place(&mut e, Kind::R, 100.0, 100.0);   // bbox x 46..154
    place(&mut e, Kind::R, 300.0, 100.0);
    // window (left -> right) must fully enclose
    e.pointer_down((20.0, 60.0), false, 1.0); e.pointer_move((200.0, 140.0)); e.pointer_up();
    assert_eq!(e.sc, vec![true, false]);
    // window that only touches R2 does not select it...
    e.clear_selection();
    e.pointer_down((250.0, 60.0), false, 1.0); e.pointer_move((320.0, 140.0)); e.pointer_up();
    assert_eq!(e.sc, vec![false, false]);
    // ...but the same area dragged right -> left (crossing) does
    e.pointer_down((320.0, 60.0), false, 1.0); e.pointer_move((250.0, 140.0)); e.pointer_up();
    assert_eq!(e.sc, vec![false, true]);
    // shift adds
    e.pointer_down((20.0, 60.0), true, 1.0); e.pointer_move((200.0, 140.0)); e.pointer_up();
    assert_eq!(e.sc, vec![true, true]);
    // tiny drags select nothing and clear
    e.pointer_down((700.0, 700.0), false, 1.0); e.pointer_move((701.0, 701.0)); e.pointer_up();
    assert_eq!(e.n_selected(), 0);
}

#[test]
fn click_select_shift_toggle_and_wires() {
    let mut e = ed();
    place(&mut e, Kind::R, 100.0, 100.0);
    e.proj.w.push(Wire { x1: 0.0, y1: 300.0, x2: 200.0, y2: 300.0 }); e.sw.push(false);
    click(&mut e, 100.0, 100.0); assert_eq!(e.sc, vec![true]);
    click(&mut e, 100.0, 303.0); assert_eq!((e.sc[0], e.sw[0]), (false, true)); // wire hit within 6px
    e.pointer_down((100.0, 100.0), true, 1.0); e.pointer_up(); assert_eq!((e.sc[0], e.sw[0]), (true, true));
    e.pointer_down((100.0, 100.0), true, 1.0); e.pointer_up(); assert_eq!((e.sc[0], e.sw[0]), (false, true)); // shift toggles off
}

#[test]
fn moving_parts_rubber_bands_attached_wires() {
    let mut e = ed();
    place(&mut e, Kind::R, 200.0, 100.0);   // pins (160,100) (240,100)
    e.set_tool(Tool::Wire); click(&mut e, 160.0, 100.0); click(&mut e, 60.0, 100.0); e.set_tool(Tool::Select);
    let far = Wire { x1: 500.0, y1: 500.0, x2: 600.0, y2: 500.0 }; e.proj.w.push(far); e.sw.push(false);
    click(&mut e, 200.0, 100.0);                       // select resistor
    e.pointer_down((200.0, 100.0), false, 1.0);        // grab
    e.pointer_move((220.0, 140.0)); e.pointer_up();
    assert_eq!((e.proj.c[0].x, e.proj.c[0].y), (220.0, 140.0));
    assert_eq!((e.proj.w[0].x1, e.proj.w[0].y1), (180.0, 140.0)); // attached end followed the pin
    assert_eq!((e.proj.w[0].x2, e.proj.w[0].y2), (60.0, 100.0));  // free end stayed
    assert_eq!(e.proj.w[1], far);
    e.undo();                                            // one undo reverts the whole drag
    assert_eq!((e.proj.c[0].x, e.proj.w[0].x1), (200.0, 160.0));
}

#[test]
fn group_rotate_and_mirror_keep_connections() {
    let mut e = ed();
    place(&mut e, Kind::R, 100.0, 100.0);
    place(&mut e, Kind::R, 300.0, 100.0);
    e.set_tool(Tool::Wire); click(&mut e, 140.0, 100.0); click(&mut e, 260.0, 100.0); e.set_tool(Tool::Select);
    let before = netlist_nodes(&e);
    e.select_all();
    for k in ['r', 'r', 'h', 'v', 'l'] { e.group(k); assert_eq!(netlist_nodes(&e), before, "after {k}"); }
    // four clockwise quarter turns return to the start
    let mut f = ed();
    place(&mut f, Kind::Npn, 100.0, 100.0); f.select_all();
    let orig = (f.proj.c[0].x, f.proj.c[0].y, f.proj.c[0].r);
    for _ in 0..4 { f.group('r'); }
    assert_eq!((f.proj.c[0].x, f.proj.c[0].y, f.proj.c[0].r), orig);
    f.group('h'); assert_eq!(f.proj.c[0].m, 1);
    f.group('h'); assert_eq!((f.proj.c[0].m, f.proj.c[0].r), (0, orig.2));
}

#[test]
fn scope_refuses_group_transform_but_rotates_alone() {
    let mut e = ed();
    place(&mut e, Kind::Os, 100.0, 100.0); place(&mut e, Kind::R, 500.0, 100.0);
    e.select_all(); e.group('h');
    assert!(e.flash.contains("Oscilloscope")); assert_eq!(e.proj.c[0].m, 0);
}

#[test]
fn undo_redo_and_dirty_tracking() {
    let mut e = ed();
    assert!(!e.is_dirty());
    place(&mut e, Kind::R, 100.0, 100.0); place(&mut e, Kind::C, 300.0, 100.0);
    assert!(e.is_dirty() && e.can_undo());
    e.undo(); assert_eq!(e.proj.c.len(), 1);
    e.undo(); assert_eq!(e.proj.c.len(), 0); assert!(!e.is_dirty());
    e.redo(); e.redo(); assert_eq!(e.proj.c.len(), 2);
    e.mark_saved(); assert!(!e.is_dirty());
    place(&mut e, Kind::L, 500.0, 100.0);
    assert!(e.is_dirty()); e.undo(); assert!(!e.is_dirty());
    // history is capped at 80 steps
    for i in 0..100 { place(&mut e, Kind::R, (i * 20) as f64, 400.0); }
    let mut n = 0; while e.can_undo() { e.undo(); n += 1; } assert_eq!(n, 80);
}

#[test]
fn delete_copy_paste_duplicate() {
    let mut e = ed();
    place(&mut e, Kind::R, 100.0, 100.0); place(&mut e, Kind::C, 300.0, 100.0);
    e.set_tool(Tool::Wire); click(&mut e, 140.0, 100.0); click(&mut e, 260.0, 100.0); e.set_tool(Tool::Select);
    e.select_all();
    let js = e.copy().unwrap();
    assert!(js.contains("CircuitLite-clip"));
    e.mouse_in = false; e.paste(false);                 // no pointer: offset by 40
    assert_eq!((e.proj.c.len(), e.proj.w.len()), (4, 2));
    assert_eq!((e.proj.c[2].x, e.proj.c[2].y), (140.0, 140.0));
    assert_eq!(e.proj.c[2].n.as_deref(), Some("R2")); assert_eq!(e.proj.c[3].n.as_deref(), Some("C2"));
    assert_eq!(e.sc, vec![false, false, true, true]);   // pasted items are selected
    e.mouse_in = true; e.mouse = (700.0, 500.0); e.paste(false);   // centred on the pointer
    let cx = (e.proj.c[4].x + e.proj.c[5].x) / 2.0;
    assert!((cx - 700.0).abs() <= 20.0, "{cx}");
    e.duplicate(); assert_eq!(e.proj.c.len(), 8);
    e.clear_selection(); click(&mut e, 100.0, 100.0); e.delete();
    assert_eq!(e.proj.c.len(), 7); assert!(e.proj.c.iter().all(|c| c.x != 100.0 || c.y != 100.0));
    // clipboard text from another instance
    let mut f = ed(); assert!(f.set_clip_json(&js)); f.mouse_in = false; f.paste(true); assert_eq!(f.proj.c.len(), 2);
    assert!(!f.set_clip_json("{\"app\":\"x\"}"));
}

#[test]
fn label_dragging_moves_offsets_with_one_undo() {
    let mut e = ed();
    place(&mut e, Kind::R, 100.0, 100.0);
    e.proj.c[0].n = Some("R1".into());
    let r = Editor::label_rects(&e.proj.c[0])[0].unwrap();
    let (gx, gy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    e.pointer_down((gx, gy), false, 1.0);
    e.pointer_move((gx + 30.0, gy + 10.0)); e.pointer_up();
    assert_eq!(e.proj.c[0].no, Some([30.0, 10.0]));
    assert!(e.proj.c[0].vo.is_none());
    e.undo(); assert!(e.proj.c[0].no.is_none());
}

#[test]
fn running_mode_switches_buttons_and_pot() {
    let mut e = ed();
    place(&mut e, Kind::Sw, 100.0, 100.0); place(&mut e, Kind::Bt, 300.0, 100.0); place(&mut e, Kind::Pot, 500.0, 100.0);
    place(&mut e, Kind::R, 700.0, 100.0);
    e.running = true;
    click(&mut e, 100.0, 100.0);
    assert_eq!(e.proj.c[0].on, 1); click(&mut e, 100.0, 100.0); assert_eq!(e.proj.c[0].on, 0);
    e.take_pokes();
    e.pointer_down((300.0, 100.0), false, 1.0);
    assert_eq!(e.proj.c[1].on, 1);
    e.pointer_up(); assert_eq!(e.proj.c[1].on, 0);               // momentary
    assert_eq!(e.take_pokes().len(), 2);
    e.pointer_down((500.0, 100.0), false, 1.0);                  // pot centre -> 50 %
    e.pointer_move((530.0, 100.0)); e.pointer_up();               // +30 px -> 100 %
    assert_eq!(e.proj.c[2].f.as_deref(), Some("100"));
    assert_eq!(e.take_pokes().last().unwrap().pp, Some(1.0));
    // no editing while running
    let n = e.proj.c.len(); e.select_all(); e.delete(); assert_eq!(e.proj.c.len(), n);
    e.set_tool(Tool::Place(Kind::R)); assert_eq!(e.tool, Tool::Select);
    // dragging parts is locked, selection still works
    e.clear_selection(); e.pointer_down((700.0, 100.0), false, 1.0); e.pointer_move((760.0, 160.0)); e.pointer_up();
    assert_eq!((e.proj.c[3].x, e.proj.c[3].y), (700.0, 100.0));
    assert!(e.sc[3]);
}

#[test]
fn property_edits_group_into_one_undo_step() {
    let mut e = ed();
    place(&mut e, Kind::R, 100.0, 100.0);
    e.edit_field(0, "v", "2".into()); e.edit_field(0, "v", "22".into()); e.edit_field(0, "v", "2.2k".into());
    assert_eq!(e.proj.c[0].v.as_deref(), Some("2.2k"));
    e.undo(); assert_eq!(e.proj.c[0].v.as_deref(), Some("1k"));   // one step undone all keystrokes
    // new selection starts a new editing session
    e.edit_field(0, "v", "5k".into()); e.clear_selection(); e.edit_field(0, "v", "6k".into());
    e.undo(); assert_eq!(e.proj.c[0].v.as_deref(), Some("5k"));
}

#[test]
fn param_tables_and_palette_search() {
    for &k in Kind::ALL { for p in params::params(k) { assert!(["v", "f", "p", "q", "s", "k"].contains(&p.key), "{k:?}"); } }
    assert_eq!(Component::new(Kind::Sg, 0., 0., 0).k.as_deref(), Some("sine"));
    assert_eq!(Component::new(Kind::Led, 0., 0., 0).p.as_deref(), Some("red"));
    assert_eq!(Component::new(Kind::Tr, 0., 0., 0).p.as_deref(), Some("0.995"));
    let hits = |q: &str| params::search(q).into_iter().flat_map(|(_, v)| v).collect::<Vec<_>>();
    assert!(hits("555").contains(&Kind::Tm));
    assert!(hits("flip flop").contains(&Kind::Jk) && hits("flip flop").contains(&Kind::Dff));
    assert!(hits("meter").contains(&Kind::Vm) && hits("meter").contains(&Kind::Am));
    assert!(hits("zzz").is_empty());
    assert_eq!(params::search("").iter().map(|(_, v)| v.len()).sum::<usize>(), 50); // every part is in exactly one palette group
}
