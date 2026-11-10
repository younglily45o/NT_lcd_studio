use lcd_studio::{Project, history::History};
#[test]
fn drag_coalesces_and_redo_restores_final_value() {
    let mut h = History::default();
    let mut p = Project::dc162();
    let original = p.clone();
    for v in [120, 100, 64] {
        h.begin(&p);
        p.current.blue[16] = v;
    }
    h.finish(&p);
    assert!(h.undo(&mut p));
    assert_eq!(p, original);
    assert!(!h.undo(&mut p));
    assert!(h.redo(&mut p));
    assert_eq!(p.current.blue[16], 64);
}
#[test]
fn baseline_replacement_is_undoable_and_new_edit_clears_redo() {
    let mut h = History::default();
    let mut p = Project::dc162();
    p.current.blue[16] = 64;
    h.begin(&p);
    p.baseline = p.current.clone();
    h.finish(&p);
    assert_eq!(p.baseline.blue[16], 64);
    assert!(h.undo(&mut p));
    assert_eq!(p.baseline.blue[16], 128);
    h.begin(&p);
    p.current.red[16] = 100;
    h.finish(&p);
    assert!(!h.can_redo());
}
#[test]
fn no_op_does_not_consume_history_and_history_is_bounded() {
    let mut h = History::default();
    let mut p = Project::identity();
    h.begin(&p);
    h.finish(&p);
    assert!(!h.can_undo());
    for i in 0..110 {
        h.begin(&p);
        p.name = format!("edit {i}");
        h.finish(&p);
    }
    let mut count = 0;
    while h.undo(&mut p) {
        count += 1;
    }
    assert_eq!(count, 100);
}
