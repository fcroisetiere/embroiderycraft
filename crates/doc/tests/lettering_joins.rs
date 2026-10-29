//! Joined lettering travels between strokes and letters instead of trimming.

use embcraft_core::{Point, StitchKind};
use embcraft_doc::lettering::{FontCache, LetterFont, LetterStitch, Lettering};
use embcraft_doc::{Document, Editor};
use embcraft_text::TextOptions;

fn trims(face: &embcraft_text::FontFace, join: Option<f64>) -> usize {
    let l = Lettering {
        text: "Karate".into(),
        font: LetterFont::from_face(face),
        opts: TextOptions {
            cap_height: 10.0,
            ..TextOptions::default()
        },
        stitch: LetterStitch::AutoSatin,
        at: Point::ZERO,
        join_runs: join,
    };
    let mut cache = FontCache::default();
    let (objects, _) = l.build(&mut cache).unwrap_or_default();
    let mut ed = Editor::new(Document::new("t"));
    ed.add_lettering(l, objects, 0);
    let p = ed
        .doc()
        .to_design()
        .to_pattern(&embcraft_design::BuildOptions::default())
        .unwrap_or_default();
    p.stitches
        .iter()
        .filter(|s| s.kind == StitchKind::Trim)
        .count()
}

#[test]
fn joined_lettering_trims_far_less() {
    let fonts = embcraft_text::system_fonts();
    let Some(face) = [
        "DejaVu Sans",
        "Liberation Sans",
        "Noto Sans",
        "Arial",
        "Helvetica",
    ]
    .iter()
    .find_map(|n| fonts.iter().find(|f| f.family == *n && f.style == "Bold")) else {
        eprintln!("no common bold font installed; skipped");
        return;
    };
    let (off, on) = (trims(face, None), trims(face, Some(15.0)));
    assert!(off >= 6, "{off}");
    // most trims go; those left are where a travel line would cross a letter already sewn
    assert!(on * 3 <= off, "{off} -> {on}");
}
