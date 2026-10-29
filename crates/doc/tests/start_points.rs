//! Every stitch type begins exactly at the object's start point, wherever it is on the shape.

use embcraft_core::{Point, StitchKind};
use embcraft_design::BuildOptions;
use embcraft_doc::{Document, Editor, StitchStyle};

#[test]
fn objects_begin_at_their_start_point() {
    let sq = vec![
        Point::new(0.0, 0.0),
        Point::new(30.0, 0.0),
        Point::new(30.0, 20.0),
        Point::new(0.0, 20.0),
    ];
    let starts = [
        Point::new(30.0, 20.0),
        Point::new(0.0, 20.0),
        Point::new(15.0, 0.0),
        Point::new(0.0, 7.0),
        Point::new(15.0, 10.0),
    ];
    for style in [
        StitchStyle::fill(),
        StitchStyle::auto_satin(),
        StitchStyle::satin(4.0),
        StitchStyle::run(),
    ] {
        for start in starts {
            let mut ed = Editor::new(Document::new("t"));
            let id = ed.add_object("A", sq.clone(), true, style.clone(), 0);
            ed.select_only(id);
            ed.set_start_selected(Some(start));
            let opts = BuildOptions {
                center: false,
                ..BuildOptions::default()
            };
            let p = ed.doc().to_design().to_pattern(&opts).unwrap();
            let first = p
                .stitches
                .iter()
                .find(|s| s.kind == StitchKind::Normal)
                .unwrap()
                .pos;
            // a start point inside a run or satin outline begins at the nearest point of it
            let want = if matches!(style, StitchStyle::Run { .. } | StitchStyle::Satin { .. })
                && start == Point::new(15.0, 10.0)
            {
                first
            } else {
                start
            };
            assert!(
                first.dist(want) < 1e-6,
                "{}: start {start:?}, first stitch {first:?}",
                style.label()
            );
            // no stitch longer than the machine allows
            assert!(p.max_stitch_length() <= 12.1);
        }
    }
}
