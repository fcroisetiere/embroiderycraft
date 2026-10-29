//! Vector icons drawn with the egui painter (no bitmap or font assets, so nothing to license).
//!
//! Every icon is laid out on a 24 x 24 grid and scaled into the target rectangle; strokes use the
//! current text colour so icons follow the theme and the enabled/selected state.

use eframe::egui::{self, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
    Export,
    MachineFile,
    Undo,
    Redo,
    Duplicate,
    Delete,
    RotateLeft,
    RotateRight,
    FlipH,
    FlipV,
    Grow,
    Shrink,
    SewEarlier,
    SewLater,
    AlignLeft,
    AlignCenterX,
    AlignRight,
    AlignTop,
    AlignCenterY,
    AlignBottom,
    CenterHoop,
    Stitches,
    Grid,
    Hoop,
    ZoomFit,
    Player,
    Play,
    Pause,
    StepBack,
    StepForward,
    ToStart,
    ToEnd,
    Close,
    ShapeAdd,
    ShapeSubtract,
    ShapeOverlap,
    ShapeExclude,
    Group,
    Ungroup,
    // tools
    Select,
    Node,
    Polyline,
    Curve,
    Rectangle,
    Ellipse,
    Freehand,
    Text,
    Hole,
    StartPoint,
    Direction,
    // stitch types
    Run,
    Satin,
    Fill,
    AutoSatin,
    Applique,
}

/// Maps the 24 x 24 design grid into `rect`.
struct Pen<'a> {
    p: &'a egui::Painter,
    rect: Rect,
    s: Stroke,
}

impl Pen<'_> {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.rect.min + Vec2::new(x, y) * (self.rect.width() / 24.0)
    }
    fn w(&self, v: f32) -> f32 {
        v * self.rect.width() / 24.0
    }
    fn line(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::line(v, self.s));
    }
    fn closed(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p.add(Shape::closed_line(v, self.s));
    }
    fn solid(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|&(x, y)| self.at(x, y)).collect();
        self.p
            .add(Shape::convex_polygon(v, self.s.color, Stroke::NONE));
    }
    fn rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.closed(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
    }
    fn fill_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, c: Color32) {
        self.p
            .rect_filled(Rect::from_two_pos(self.at(x0, y0), self.at(x1, y1)), 0.0, c);
    }
    fn circle(&self, x: f32, y: f32, r: f32) {
        self.p.circle_stroke(self.at(x, y), self.w(r), self.s);
    }
    fn dot(&self, x: f32, y: f32, r: f32) {
        self.p.circle_filled(self.at(x, y), self.w(r), self.s.color);
    }
    fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
        (0..=16)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / 16.0).to_radians();
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }
    /// Filled arrow head at `tip`, pointing along (dx, dy).
    fn head(&self, tip: (f32, f32), dx: f32, dy: f32) {
        let l = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (ux, uy) = (dx / l, dy / l);
        let (bx, by) = (tip.0 - ux * 4.5, tip.1 - uy * 4.5);
        self.solid(&[
            tip,
            (bx - uy * 3.2, by + ux * 3.2),
            (bx + uy * 3.2, by - ux * 3.2),
        ]);
    }
}

pub fn paint(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let pen = Pen {
        p: painter,
        rect,
        s: Stroke::new((rect.width() / 13.0).clamp(1.2, 2.4), color),
    };
    let faint = color.gamma_multiply(0.35);
    use Icon::*;
    match icon {
        New => {
            pen.closed(&[
                (5.0, 2.5),
                (14.0, 2.5),
                (19.0, 7.5),
                (19.0, 21.5),
                (5.0, 21.5),
            ]);
            pen.line(&[(14.0, 2.5), (14.0, 7.5), (19.0, 7.5)]);
            pen.line(&[(9.0, 14.5), (15.0, 14.5)]);
            pen.line(&[(12.0, 11.5), (12.0, 17.5)]);
        }
        Open => {
            pen.closed(&[
                (2.5, 5.0),
                (9.0, 5.0),
                (11.0, 7.5),
                (21.5, 7.5),
                (21.5, 19.5),
                (2.5, 19.5),
            ]);
            pen.line(&[(2.5, 11.0), (21.5, 11.0)]);
        }
        Save => {
            pen.closed(&[
                (3.5, 3.5),
                (17.0, 3.5),
                (20.5, 7.0),
                (20.5, 20.5),
                (3.5, 20.5),
            ]);
            pen.rect(7.5, 3.5, 15.5, 9.0);
            pen.rect(7.0, 13.5, 17.0, 20.5);
        }
        Export => {
            pen.line(&[
                (9.0, 6.0),
                (4.0, 6.0),
                (4.0, 20.0),
                (18.0, 20.0),
                (18.0, 15.0),
            ]);
            pen.line(&[(10.0, 14.0), (19.0, 5.0)]);
            pen.head((21.0, 3.0), 1.0, -1.0);
        }
        MachineFile => {
            // a page carrying a line of zigzag stitches
            pen.closed(&[
                (5.0, 2.5),
                (14.0, 2.5),
                (19.0, 7.5),
                (19.0, 21.5),
                (5.0, 21.5),
            ]);
            pen.line(&[(14.0, 2.5), (14.0, 7.5), (19.0, 7.5)]);
            let z = [
                (8.0, 16.5),
                (10.0, 11.5),
                (12.0, 16.5),
                (14.0, 11.5),
                (16.0, 16.5),
            ];
            pen.line(&z);
            for (x, y) in z {
                pen.dot(x, y, 1.1);
            }
        }
        Undo | Redo => {
            let mirror = |x: f32| if icon == Redo { 24.0 - x } else { x };
            let mut a = pen.arc(13.0, 14.0, 7.0, -90.0, 90.0);
            a.push((8.0, 21.0));
            let a: Vec<_> = a.into_iter().map(|(x, y)| (mirror(x), y)).collect();
            pen.line(&a);
            pen.line(&[(mirror(13.0), 7.0), (mirror(7.0), 7.0)]);
            pen.head(
                (mirror(3.0), 7.0),
                if icon == Redo { 1.0 } else { -1.0 },
                0.0,
            );
        }
        Duplicate => {
            pen.rect(3.5, 3.5, 14.5, 14.5);
            pen.rect(9.5, 9.5, 20.5, 20.5);
        }
        Delete => {
            pen.line(&[(3.5, 6.5), (20.5, 6.5)]);
            pen.line(&[(9.0, 6.5), (9.0, 3.5), (15.0, 3.5), (15.0, 6.5)]);
            pen.closed(&[(5.5, 6.5), (18.5, 6.5), (17.0, 21.0), (7.0, 21.0)]);
            pen.line(&[(10.0, 10.5), (10.0, 17.5)]);
            pen.line(&[(14.0, 10.5), (14.0, 17.5)]);
        }
        RotateLeft | RotateRight => {
            let left = icon == RotateLeft;
            let a = if left {
                pen.arc(12.0, 13.0, 7.5, -60.0, 200.0)
            } else {
                pen.arc(12.0, 13.0, 7.5, -20.0, 240.0)
            };
            pen.line(&a);
            if left {
                pen.head((5.4, 9.8), -0.6, -1.0);
            } else {
                pen.head((18.6, 9.8), 0.6, -1.0);
            }
        }
        FlipH => {
            pen.line(&[(12.0, 2.5), (12.0, 21.5)]);
            pen.closed(&[(9.0, 6.0), (9.0, 18.0), (2.5, 18.0)]);
            pen.solid(&[(15.0, 6.0), (15.0, 18.0), (21.5, 18.0)]);
        }
        FlipV => {
            pen.line(&[(2.5, 12.0), (21.5, 12.0)]);
            pen.closed(&[(6.0, 9.0), (18.0, 9.0), (18.0, 2.5)]);
            pen.solid(&[(6.0, 15.0), (18.0, 15.0), (18.0, 21.5)]);
        }
        Grow | Shrink => {
            // dashed outline of the old size, solid square, and a plus or minus
            pen.rect(4.0, 4.0, 20.0, 20.0);
            pen.line(&[(8.5, 12.0), (15.5, 12.0)]);
            if icon == Grow {
                pen.line(&[(12.0, 8.5), (12.0, 15.5)]);
            }
            for (x, y) in [(2.0, 2.0), (22.0, 2.0), (22.0, 22.0), (2.0, 22.0)] {
                if icon == Grow {
                    pen.dot(x, y, 1.2);
                }
            }
        }
        SewEarlier | SewLater => {
            // stacked layers with an arrow
            pen.closed(&[(3.0, 15.0), (12.0, 19.5), (21.0, 15.0), (12.0, 10.5)]);
            if icon == SewLater {
                pen.line(&[(12.0, 13.0), (12.0, 4.5)]);
                pen.head((12.0, 2.0), 0.0, -1.0);
            } else {
                pen.line(&[(12.0, 2.5), (12.0, 10.0)]);
                pen.head((12.0, 13.0), 0.0, 1.0);
            }
        }
        AlignLeft | AlignCenterX | AlignRight => {
            let x = match icon {
                AlignLeft => 3.5,
                AlignRight => 20.5,
                _ => 12.0,
            };
            pen.line(&[(x, 2.5), (x, 21.5)]);
            let (a, b) = match icon {
                AlignLeft => ((3.5, 15.0), (3.5, 10.0)),
                AlignRight => ((5.5, 20.5), (10.5, 20.5)),
                _ => ((5.0, 19.0), (8.0, 16.0)),
            };
            pen.fill_rect(a.0, 5.0, a.1, 10.0, faint);
            pen.rect(a.0, 5.0, a.1, 10.0);
            pen.fill_rect(b.0, 13.0, b.1, 18.0, faint);
            pen.rect(b.0, 13.0, b.1, 18.0);
        }
        AlignTop | AlignCenterY | AlignBottom => {
            let y = match icon {
                AlignTop => 3.5,
                AlignBottom => 20.5,
                _ => 12.0,
            };
            pen.line(&[(2.5, y), (21.5, y)]);
            let (a, b) = match icon {
                AlignTop => ((3.5, 15.0), (3.5, 10.0)),
                AlignBottom => ((5.5, 20.5), (10.5, 20.5)),
                _ => ((5.0, 19.0), (8.0, 16.0)),
            };
            pen.fill_rect(5.0, a.0, 10.0, a.1, faint);
            pen.rect(5.0, a.0, 10.0, a.1);
            pen.fill_rect(13.0, b.0, 18.0, b.1, faint);
            pen.rect(13.0, b.0, 18.0, b.1);
        }
        // two overlapping squares, A (3,3)-(15,15) under B (9,9)-(21,21)
        ShapeAdd => {
            let c = pen.s.color;
            pen.fill_rect(3.0, 3.0, 15.0, 15.0, c);
            pen.fill_rect(9.0, 9.0, 21.0, 21.0, c);
        }
        ShapeSubtract => {
            let c = pen.s.color;
            pen.fill_rect(3.0, 3.0, 15.0, 9.0, c);
            pen.fill_rect(3.0, 9.0, 9.0, 15.0, c);
            Pen {
                s: Stroke::new(pen.s.width, faint),
                ..pen
            }
            .rect(9.0, 9.0, 21.0, 21.0);
        }
        ShapeOverlap => {
            let f = Pen {
                s: Stroke::new(pen.s.width, faint),
                ..pen
            };
            f.rect(3.0, 3.0, 15.0, 15.0);
            f.rect(9.0, 9.0, 21.0, 21.0);
            pen.fill_rect(9.0, 9.0, 15.0, 15.0, pen.s.color);
        }
        ShapeExclude => {
            let c = pen.s.color;
            pen.fill_rect(3.0, 3.0, 15.0, 9.0, c);
            pen.fill_rect(3.0, 9.0, 9.0, 15.0, c);
            pen.fill_rect(15.0, 9.0, 21.0, 15.0, c);
            pen.fill_rect(9.0, 15.0, 21.0, 21.0, c);
        }
        Group | Ungroup => {
            pen.rect(5.0, 6.0, 11.0, 12.0);
            pen.circle(16.0, 15.0, 3.5);
            // the frame of corner marks holding them together
            let frame = if icon == Group {
                pen.s
            } else {
                Stroke::new(pen.s.width, faint)
            };
            let f = Pen { s: frame, ..pen };
            for (x, y, dx, dy) in [
                (2.5, 2.5, 1.0, 1.0),
                (21.5, 2.5, -1.0, 1.0),
                (21.5, 21.5, -1.0, -1.0),
                (2.5, 21.5, 1.0, -1.0),
            ] {
                f.line(&[(x + 4.0 * dx, y), (x, y), (x, y + 4.0 * dy)]);
            }
        }
        CenterHoop => {
            let r = Rect::from_two_pos(pen.at(2.5, 2.5), pen.at(21.5, 21.5));
            painter.rect_stroke(r, pen.w(4.0), pen.s, egui::StrokeKind::Middle);
            pen.circle(12.0, 12.0, 4.0);
            pen.line(&[(12.0, 5.5), (12.0, 9.0)]);
            pen.line(&[(12.0, 15.0), (12.0, 18.5)]);
            pen.line(&[(5.5, 12.0), (9.0, 12.0)]);
            pen.line(&[(15.0, 12.0), (18.5, 12.0)]);
        }
        Stitches => {
            pen.line(&[
                (3.0, 18.0),
                (7.0, 6.0),
                (10.0, 18.0),
                (14.0, 6.0),
                (17.0, 18.0),
                (21.0, 6.0),
            ]);
            for (x, y) in [
                (3.0, 18.0),
                (7.0, 6.0),
                (10.0, 18.0),
                (14.0, 6.0),
                (17.0, 18.0),
                (21.0, 6.0),
            ] {
                pen.dot(x, y, 1.4);
            }
        }
        Grid => {
            pen.rect(3.5, 3.5, 20.5, 20.5);
            for t in [9.17, 14.83] {
                pen.line(&[(t, 3.5), (t, 20.5)]);
                pen.line(&[(3.5, t), (20.5, t)]);
            }
        }
        Hoop => {
            let r = Rect::from_two_pos(pen.at(3.0, 2.5), pen.at(21.0, 21.5));
            painter.rect_stroke(r, pen.w(5.0), pen.s, egui::StrokeKind::Middle);
            let r = Rect::from_two_pos(pen.at(6.0, 5.5), pen.at(18.0, 18.5));
            painter.rect_stroke(
                r,
                pen.w(3.0),
                Stroke::new(pen.s.width * 0.7, faint),
                egui::StrokeKind::Middle,
            );
            pen.line(&[(10.0, 2.5), (10.0, 0.8), (14.0, 0.8), (14.0, 2.5)]);
        }
        ZoomFit => {
            pen.line(&[(3.0, 8.0), (3.0, 3.0), (8.0, 3.0)]);
            pen.line(&[(16.0, 3.0), (21.0, 3.0), (21.0, 8.0)]);
            pen.line(&[(21.0, 16.0), (21.0, 21.0), (16.0, 21.0)]);
            pen.line(&[(8.0, 21.0), (3.0, 21.0), (3.0, 16.0)]);
            pen.rect(8.0, 8.0, 16.0, 16.0);
        }
        Player => {
            // a needle with its eye at the top and thread running through it
            pen.line(&[(17.5, 3.5), (6.0, 20.5)]);
            pen.circle(16.6, 5.0, 1.6);
            pen.line(&[(17.5, 5.5), (20.5, 10.0), (17.0, 15.5), (20.0, 20.5)]);
            pen.dot(6.0, 20.5, 1.0);
        }
        Play => pen.solid(&[(7.0, 4.5), (19.5, 12.0), (7.0, 19.5)]),
        Pause => {
            pen.fill_rect(6.5, 5.0, 10.0, 19.0, color);
            pen.fill_rect(14.0, 5.0, 17.5, 19.0, color);
        }
        StepBack => {
            pen.solid(&[(17.0, 5.0), (8.0, 12.0), (17.0, 19.0)]);
            pen.fill_rect(5.5, 5.0, 7.5, 19.0, color);
        }
        StepForward => {
            pen.solid(&[(7.0, 5.0), (16.0, 12.0), (7.0, 19.0)]);
            pen.fill_rect(16.5, 5.0, 18.5, 19.0, color);
        }
        ToStart => {
            pen.solid(&[(13.0, 5.0), (5.5, 12.0), (13.0, 19.0)]);
            pen.solid(&[(20.5, 5.0), (13.0, 12.0), (20.5, 19.0)]);
            pen.fill_rect(3.0, 5.0, 4.8, 19.0, color);
        }
        ToEnd => {
            pen.solid(&[(3.5, 5.0), (11.0, 12.0), (3.5, 19.0)]);
            pen.solid(&[(11.0, 5.0), (18.5, 12.0), (11.0, 19.0)]);
            pen.fill_rect(19.2, 5.0, 21.0, 19.0, color);
        }
        Close => {
            pen.line(&[(6.0, 6.0), (18.0, 18.0)]);
            pen.line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Select => {
            pen.solid(&[(5.0, 2.5), (5.0, 18.0), (9.0, 14.5), (5.0, 2.5)]);
            pen.solid(&[(5.0, 2.5), (9.0, 14.5), (16.5, 14.5)]);
            pen.line(&[(9.5, 14.0), (13.0, 21.0)]);
        }
        Node => {
            let pts = [(3.5, 19.0), (9.0, 6.0), (16.0, 15.0), (20.5, 5.0)];
            pen.line(&pts);
            for (x, y) in pts {
                let r = Rect::from_center_size(pen.at(x, y), Vec2::splat(pen.w(4.4)));
                painter.rect_filled(r, 0.0, Color32::WHITE.gamma_multiply(0.9));
                painter.rect_stroke(
                    r,
                    0.0,
                    Stroke::new(pen.s.width * 0.8, color),
                    egui::StrokeKind::Middle,
                );
            }
        }
        Polyline => {
            let pts = [(3.0, 18.0), (8.0, 6.0), (15.0, 17.0), (21.0, 7.0)];
            pen.line(&pts);
            for (x, y) in pts {
                pen.dot(x, y, 2.0);
            }
        }
        Curve => {
            // an S-curve with one node and its handle line
            let v: Vec<(f32, f32)> = (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0;
                    let u = 1.0 - t;
                    let (p0, c1, c2, p1) = ((3.0, 19.0), (3.0, 3.0), (21.0, 21.0), (21.0, 5.0));
                    (
                        u * u * u * p0.0
                            + 3.0 * u * u * t * c1.0
                            + 3.0 * u * t * t * c2.0
                            + t * t * t * p1.0,
                        u * u * u * p0.1
                            + 3.0 * u * u * t * c1.1
                            + 3.0 * u * t * t * c2.1
                            + t * t * t * p1.1,
                    )
                })
                .collect();
            pen.line(&v);
            pen.p.line_segment(
                [pen.at(6.0, 15.0), pen.at(18.0, 9.0)],
                Stroke::new(pen.s.width * 0.6, faint),
            );
            pen.dot(12.0, 12.0, 1.9);
            pen.dot(6.0, 15.0, 1.3);
            pen.dot(18.0, 9.0, 1.3);
        }
        Text => {
            // a capital A with a baseline
            pen.line(&[(4.5, 19.0), (12.0, 3.5), (19.5, 19.0)]);
            pen.line(&[(7.5, 13.0), (16.5, 13.0)]);
            pen.line(&[(2.5, 21.5), (21.5, 21.5)]);
        }
        Direction => {
            // a stroke with a two-headed arrow across it
            pen.closed(&[(3.0, 8.0), (21.0, 8.0), (21.0, 16.0), (3.0, 16.0)]);
            pen.line(&[(8.0, 20.5), (16.0, 3.5)]);
            pen.head((16.8, 1.8), 0.47, -1.0);
            pen.head((7.2, 22.2), -0.47, 1.0);
        }
        StartPoint => {
            // a flag on a pole: where the sewing begins
            pen.line(&[(6.0, 21.5), (6.0, 2.5)]);
            pen.solid(&[(6.0, 3.0), (19.5, 7.5), (6.0, 12.0)]);
            pen.dot(6.0, 21.5, 1.6);
        }
        Rectangle => pen.rect(3.5, 5.5, 20.5, 18.5),
        Ellipse => {
            let v: Vec<Pos2> = (0..40)
                .map(|i| {
                    let a = i as f32 / 40.0 * std::f32::consts::TAU;
                    pen.at(12.0 + 8.5 * a.cos(), 12.0 + 6.5 * a.sin())
                })
                .collect();
            painter.add(Shape::closed_line(v, pen.s));
        }
        Freehand => {
            let v: Vec<(f32, f32)> = (0..=30)
                .map(|i| {
                    let t = i as f32 / 30.0;
                    (
                        3.0 + 18.0 * t,
                        13.0 + 6.0 * (t * 9.0).sin() * (1.0 - t * 0.4) - 2.0 * t,
                    )
                })
                .collect();
            pen.line(&v);
        }
        Hole => {
            let ring = |r: f32| -> Vec<Pos2> {
                (0..40)
                    .map(|i| {
                        let a = i as f32 / 40.0 * std::f32::consts::TAU;
                        pen.at(12.0 + r * a.cos(), 12.0 + r * a.sin())
                    })
                    .collect()
            };
            painter.add(Shape::closed_line(ring(9.0), pen.s));
            painter.add(Shape::closed_line(
                ring(4.0),
                Stroke::new(pen.s.width, color),
            ));
            for (x0, y0, x1, y1) in [
                (5.0, 8.0, 8.0, 5.0),
                (4.0, 14.0, 14.0, 4.0),
                (10.0, 20.0, 20.0, 10.0),
                (16.0, 19.0, 19.0, 16.0),
            ] {
                pen.p.line_segment(
                    [pen.at(x0, y0), pen.at(x1, y1)],
                    Stroke::new(pen.s.width * 0.6, faint),
                );
            }
        }
        Run => {
            for i in 0..4 {
                let x = 2.5 + i as f32 * 5.0;
                pen.line(&[(x, 12.0), (x + 3.5, 12.0)]);
            }
            pen.dot(2.5, 12.0, 1.3);
            pen.dot(21.5, 12.0, 1.3);
        }
        Satin => {
            let mut v = Vec::new();
            for i in 0..=9 {
                let x = 3.0 + i as f32 * 2.0;
                v.push((x, if i % 2 == 0 { 6.0 } else { 18.0 }));
            }
            pen.line(&v);
        }
        AutoSatin => {
            // an L-shaped stroke: satin across the upright, then across the foot
            pen.closed(&[
                (4.0, 3.0),
                (10.0, 3.0),
                (10.0, 15.0),
                (21.0, 15.0),
                (21.0, 21.0),
                (4.0, 21.0),
            ]);
            let thin = Stroke::new(pen.s.width * 0.7, color);
            for k in 0..5 {
                let y = 5.0 + k as f32 * 2.2;
                pen.p.line_segment([pen.at(5.5, y), pen.at(8.5, y)], thin);
            }
            for k in 0..5 {
                let x = 12.0 + k as f32 * 2.0;
                pen.p.line_segment([pen.at(x, 16.5), pen.at(x, 19.5)], thin);
            }
        }
        Applique => {
            // a patch of fabric with a satin border round its edge
            pen.fill_rect(5.0, 5.0, 19.0, 19.0, faint);
            let thin = Stroke::new(pen.s.width * 0.6, color);
            let zig = |a: (f32, f32), b: (f32, f32), n: usize, out: (f32, f32)| {
                for k in 0..=n {
                    let t = k as f32 / n as f32;
                    let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                    pen.p.line_segment(
                        [pen.at(x - out.0, y - out.1), pen.at(x + out.0, y + out.1)],
                        thin,
                    );
                }
            };
            zig((4.5, 4.5), (19.5, 4.5), 7, (0.0, 1.6));
            zig((4.5, 19.5), (19.5, 19.5), 7, (0.0, 1.6));
            zig((4.5, 6.5), (4.5, 17.5), 5, (1.6, 0.0));
            zig((19.5, 6.5), (19.5, 17.5), 5, (1.6, 0.0));
        }
        Fill => {
            pen.rect(3.5, 3.5, 20.5, 20.5);
            for k in 0..5 {
                let o = -8.5 + k as f32 * 4.25;
                // diagonal rows clipped to the square: points where x - y = o
                let (x0, y0) = if o >= 0.0 {
                    (3.5 + o, 3.5)
                } else {
                    (3.5, 3.5 - o)
                };
                let (x1, y1) = if o >= 0.0 {
                    (20.5, 20.5 - o)
                } else {
                    (20.5 + o, 20.5)
                };
                pen.p.line_segment(
                    [pen.at(x0, y0), pen.at(x1, y1)],
                    Stroke::new(pen.s.width * 0.75, color),
                );
            }
        }
    }
}

/// A square icon button. Hover text names the action and its shortcut.
pub fn button(ui: &mut Ui, icon: Icon, selected: bool, enabled: bool, tip: &str) -> Response {
    let size = Vec2::splat(30.0);
    let (rect, resp) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if ui.is_rect_visible(rect) {
        let vis = ui.style().interact_selectable(&resp, selected);
        let fill = if selected {
            ui.visuals().selection.bg_fill
        } else if resp.hovered() && enabled {
            vis.weak_bg_fill
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 5.0, fill);
        if selected {
            ui.painter().rect_stroke(
                rect,
                5.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
        let color = if !enabled {
            ui.visuals().weak_text_color().gamma_multiply(0.6)
        } else if selected {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        };
        paint(ui.painter(), rect.shrink(5.0), icon, color);
    }
    resp.on_hover_text(tip)
}

/// Icon followed by a text label, as one selectable row (tool lists, stitch types).
pub fn labeled(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    hint: &str,
    selected: bool,
    width: f32,
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let vis = ui.style().interact_selectable(&resp, selected);
        if selected {
            ui.painter()
                .rect_filled(rect, 5.0, ui.visuals().selection.bg_fill);
            ui.painter().rect_stroke(
                rect,
                5.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Inside,
            );
        } else if resp.hovered() {
            ui.painter().rect_filled(rect, 5.0, vis.weak_bg_fill);
        }
        let color = if selected {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        };
        let ir = Rect::from_min_size(rect.min + Vec2::new(5.0, 4.0), Vec2::splat(20.0));
        paint(ui.painter(), ir, icon, color);
        ui.painter().text(
            rect.min + Vec2::new(32.0, rect.height() / 2.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            color,
        );
        if !hint.is_empty() {
            ui.painter().text(
                rect.right_center() - Vec2::new(8.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                hint,
                egui::FontId::proportional(12.0),
                ui.visuals().weak_text_color(),
            );
        }
    }
    resp
}
