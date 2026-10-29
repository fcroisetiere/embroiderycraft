//! `embcraft` — command-line front end for EmbroideryCraft.

use embcraft_core::{Point, Thread, svg::to_svg};
use embcraft_design::{BuildOptions, Design, Object, ObjectKind};
use embcraft_dst::{WriteOptions, read, write};
use embcraft_stitch::{FillParams, RunParams, SatinParams};
use std::{env, fs, process::ExitCode};

const USAGE: &str = "\
embcraft — free embroidery digitizing tools

USAGE:
  embcraft demo    <design.json>                 write an example design
  embcraft build   <design.json> -o <out.dst> [--svg <preview.svg>] [--merge-colors] [--no-tie]
  embcraft testpattern -o <trimtest.dst>
  embcraft info    <file.dst>
  embcraft preview <file.dst> -o <preview.svg>
";

type Res<T> = Result<T, String>;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn opt<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn run(args: &[String]) -> Res<()> {
    match args.first().map(String::as_str) {
        Some("demo") => {
            let out = args.get(1).ok_or("demo needs an output path")?;
            let json = demo().to_json().map_err(|e| e.to_string())?;
            fs::write(out, json).map_err(|e| format!("{out}: {e}"))?;
            println!("wrote {out}");
            Ok(())
        }
        Some("build") => {
            let input = args.get(1).ok_or("build needs a design file")?;
            let out = opt(args, "-o").ok_or("build needs -o <out.dst>")?;
            let text = fs::read_to_string(input).map_err(|e| format!("{input}: {e}"))?;
            let design = Design::from_json(&text).map_err(|e| e.to_string())?;
            let opts = BuildOptions {
                merge_colors: args.iter().any(|a| a == "--merge-colors"),
                tie: !args.iter().any(|a| a == "--no-tie"),
                ..BuildOptions::default()
            };
            let pat = design.to_pattern(&opts).map_err(|e| e.to_string())?;
            let bytes = write(&pat, &WriteOptions::default()).map_err(|e| e.to_string())?;
            fs::write(out, bytes).map_err(|e| format!("{out}: {e}"))?;
            if let Some(svg) = opt(args, "--svg") {
                fs::write(svg, to_svg(&pat, 0.35)).map_err(|e| format!("{svg}: {e}"))?;
            }
            print_info(&pat);
            println!("wrote {out}");
            Ok(())
        }
        Some("testpattern") => {
            let out = opt(args, "-o").ok_or("testpattern needs -o <out.dst>")?;
            fs::write(out, embcraft_dst::trim_test_pattern()).map_err(|e| format!("{out}: {e}"))?;
            println!("wrote {out}\n\nGap after patch -> encoding:");
            for v in &embcraft_dst::TRIM_VARIANTS {
                println!(
                    "  {}: {} zero-move jump(s) + travel in {} record(s)",
                    v.n, v.zero_jumps, v.move_records
                );
            }
            println!("  9: colour change, then jump to patch 10");
            Ok(())
        }
        Some("info") => {
            let input = args.get(1).ok_or("info needs a DST file")?;
            let bytes = fs::read(input).map_err(|e| format!("{input}: {e}"))?;
            print_info(&read(&bytes).map_err(|e| e.to_string())?);
            Ok(())
        }
        Some("preview") => {
            let input = args.get(1).ok_or("preview needs a DST file")?;
            let out = opt(args, "-o").ok_or("preview needs -o <preview.svg>")?;
            let bytes = fs::read(input).map_err(|e| format!("{input}: {e}"))?;
            let pat = read(&bytes).map_err(|e| e.to_string())?;
            fs::write(out, to_svg(&pat, 0.35)).map_err(|e| format!("{out}: {e}"))?;
            println!("wrote {out}");
            Ok(())
        }
        _ => Err("missing or unknown command".into()),
    }
}

fn print_info(p: &embcraft_core::Pattern) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    // Ignore write errors (e.g. a closed pipe) instead of panicking like println! does.
    let _ = writeln!(out, "name:           {}", p.name);
    let _ = writeln!(out, "stitches:       {}", p.stitch_count());
    let _ = writeln!(out, "jumps:          {}", p.jumps());
    let _ = writeln!(out, "trims:          {}", p.trims());
    let _ = writeln!(out, "colour changes: {}", p.color_changes());
    if let Some((lo, hi)) = p.bounds() {
        let _ = writeln!(
            out,
            "size:           {:.1} x {:.1} mm",
            hi.x - lo.x,
            hi.y - lo.y
        );
    }
    let _ = writeln!(out, "longest stitch: {:.2} mm", p.max_stitch_length());
}

/// Example design: a ring (tatami with a hole), an S-curve satin column and a triple-run triangle.
fn demo() -> Design {
    let mut d = Design::new("demo");
    d.threads = vec![
        Thread::new([0x1f, 0x3a, 0x93], "Blue"),
        Thread::new([0xc8, 0x1e, 0x2d], "Red"),
        Thread::new([0x20, 0x20, 0x20], "Black"),
    ];
    let circle = |cx: f64, cy: f64, r: f64, n: usize| -> Vec<Point> {
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                Point::new(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    d.objects.push(Object {
        name: "ring".into(),
        thread: 0,
        start: None,
        join_run: None,
        kind: ObjectKind::Fill {
            rings: vec![circle(0.0, 0.0, 15.0, 96), circle(0.0, 0.0, 7.0, 64)],
            params: FillParams::default(),
        },
    });
    let s_curve: Vec<Point> = (0..=60)
        .map(|i| {
            let t = i as f64 / 60.0;
            Point::new(30.0 + 25.0 * t, 8.0 * (t * std::f64::consts::TAU).sin())
        })
        .collect();
    d.objects.push(Object {
        name: "wave".into(),
        thread: 1,
        start: None,
        join_run: None,
        kind: ObjectKind::Satin {
            center: s_curve,
            width: 4.0,
            params: SatinParams::default(),
        },
    });
    d.objects.push(Object {
        name: "triangle".into(),
        thread: 2,
        start: None,
        join_run: None,
        kind: ObjectKind::Run {
            points: vec![
                Point::new(-12.0, 28.0),
                Point::new(12.0, 28.0),
                Point::new(0.0, 45.0),
                Point::new(-12.0, 28.0),
            ],
            params: RunParams {
                triple: true,
                ..RunParams::default()
            },
        },
    });
    d
}
