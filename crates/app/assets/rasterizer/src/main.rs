//! Rasterise an SVG into PNGs.
//!
//! ```text
//! shadowplayer-icon-rasterizer <in.svg> <out-dir> <size> [<size> …]
//! ```
//!
//! Writes `<out-dir>/icon-<size>.png` for each requested size, with the
//! pixels scaled from the SVG's own viewBox so the mark is identical at every
//! size.

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: {} <in.svg> <out-dir> <size> [<size> …]", args[0]);
        std::process::exit(2);
    }
    let input = &args[1];
    let out_dir = Path::new(&args[2]);

    let opt = usvg::Options::default();
    let data = std::fs::read(input)?;
    let tree = usvg::Tree::from_data(&data, &opt)?;

    for raw in &args[3..] {
        let size: u32 = raw.parse()?;
        let mut pixmap = tiny_skia::Pixmap::new(size, size)
            .ok_or("the requested icon size is not a valid pixmap dimension")?;
        let scale = size as f32 / tree.size().width() as f32;
        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        let path = out_dir.join(format!("icon-{size}.png"));
        std::fs::write(&path, pixmap.encode_png()?)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
