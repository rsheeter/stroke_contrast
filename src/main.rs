use std::{
    env::home_dir,
    fs,
    path::{Path, PathBuf},
};

use clap::Parser;
use harfruzz::{GlyphBuffer, ShaperFont};
use kurbo::{Affine, BezPath, Line, ParamCurve, Point, Shape, Vec2};
use skrifa::{
    outline::{DrawSettings, OutlinePen}, prelude::{LocationRef, Size}, MetadataProvider
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Where to save svg files
    #[arg(short, long, default_value = "/tmp/an.svg")]
    output_svg: String,

    /// The text to draw
    #[arg(short, long)]
    char: char,

    /// The font to process
    #[arg(long)]
    font: String,
}

struct PathPen {
    transform: Affine,
    path: BezPath,
}

impl Default for PathPen {
    fn default() -> Self {
        // flip y because fonts are y-up and svg is y-down
        Self {
            transform: Affine::FLIP_Y,
            path: Default::default(),
        }
    }
}

impl OutlinePen for PathPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(
            self.transform
                * Point {
                    x: x.into(),
                    y: y.into(),
                },
        );
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(
            self.transform
                * Point {
                    x: x.into(),
                    y: y.into(),
                },
        );
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.path.quad_to(
            self.transform
                * Point {
                    x: cx0.into(),
                    y: cy0.into(),
                },
            self.transform
                * Point {
                    x: x.into(),
                    y: y.into(),
                },
        );
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to(
            self.transform
                * Point {
                    x: cx0.into(),
                    y: cy0.into(),
                },
            self.transform
                * Point {
                    x: cx1.into(),
                    y: cy1.into(),
                },
            self.transform
                * Point {
                    x: x.into(),
                    y: y.into(),
                },
        );
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

// Simplified version of <https://github.com/harfbuzz/harfruzz/blob/006472176ab87e3a84e799e74e0ac19fbe943dd7/tests/shaping/main.rs#L107>
// Will have to update if/when that API updates
fn shape(text: &str, font: &harfruzz::FontRef) -> GlyphBuffer {
    let shaper_font = ShaperFont::new(font);
    let face = shaper_font.shaper(font, &[]);

    let mut buffer = harfruzz::UnicodeBuffer::new();
    buffer.push_str(text);

    harfruzz::shape(&face, &[], buffer)
}

fn main() {
    let args = Args::parse();
    let font_path = if args.font.starts_with("~") {
        let mut d = home_dir().expect("Must have a home dir");
        d.push(&args.font[1..]);
        d
    } else {
        PathBuf::from(&args.font)
    };
    let raw_font =
        fs::read(&font_path).unwrap_or_else(|e| panic!("Unable to read {font_path:?}: {e}"));
    let harf_font_ref =
        harfruzz::FontRef::new(&raw_font).expect("For font files to be font files!");
    let skrifa_font_ref = skrifa::FontRef::new(&raw_font).expect("Fonts to be fonts");

    let outlines = skrifa_font_ref.outline_glyphs();
    let mut pen = PathPen::default();

    let glyphs = shape(&format!("{}", args.char), &harf_font_ref);
    for (glyph_info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
        let glyph = outlines
            .get(glyph_info.glyph_id.into())
            .expect("Glyphs to exist!");
        glyph
            .draw(
                DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
                &mut pen,
            )
            .expect("To draw!");

        pen.transform = pen.transform.then_translate(Vec2 {
            x: pos.x_advance.into(),
            y: pos.y_advance.into(),
        });
    }

    let path = pen.path;
    let bbox = path.bounding_box();
    let margin = 0.03 * bbox.width().max(bbox.height());
    let bbox = bbox.inflate(margin, margin).expand();

    let mut svg = String::new();
    svg.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" viewBox=""#);
    svg.push_str(&format!("{:02} ", bbox.min_x()));
    svg.push_str(&format!("{:02} ", bbox.min_y()));
    svg.push_str(&format!("{:02} ", bbox.width()));
    svg.push_str(&format!("{:02}", bbox.height()));
    svg.push_str(r#"">"#);
    svg.push_str("\n");
    svg.push_str("  <path d=\"");
    svg.push_str(&path.to_svg());
    svg.push_str("\" />\n");

    let r_dot = margin / 32.0;
    let r_isct = margin / 16.0;
    let w_ray = margin / 64.0;
    let mut live = Vec::new();

    for x in (bbox.min_x() as i32..(bbox.min_x() + bbox.width()) as i32).step_by((bbox.width() / 100.0).floor() as usize) {
        for y in (bbox.min_y() as i32..(bbox.min_y() + bbox.height()) as i32).step_by((bbox.height() / 100.0).floor() as usize) {
            let color = if path.winding(Point { x: x as f64, y: y as f64}) != 0 {
                live.push((x as f64, y as f64));
                "green"
            } else {
                //"red"
                continue;
            };

            svg.push_str(&format!("  <circle r=\"{r_dot}\" "));
            svg.push_str(&format!("cx=\"{x}\" cy=\"{y}\" "));
            svg.push_str("fill=\"");
            svg.push_str(color);
            svg.push_str("\" ");
            svg.push_str("/>\n");

        }
    }

    let (sum_x, sum_y) = live.iter().fold((0.0, 0.0), |acc, e| (
        acc.0 + e.0,
        acc.1 + e.1
    ));
    let center_of_mass = Point::new(sum_x / live.len() as f64, sum_y / live.len() as f64);
    // svg.push_str(&format!("  <circle r=\"{margin}\" "));
    // svg.push_str(&format!("cx=\"{}\" cy=\"{}\" ", center_of_mass.x, center_of_mass.y));
    // svg.push_str("fill=\"purple\"");
    // svg.push_str("/>\n");
    
    let ray = Affine::translate(center_of_mass.to_vec2()) * Line::new(Point { x: 0.0, y: 0.0 }, Point { x:bbox.width(), y: bbox.height() });    
    let mut rays = Vec::new();
    let mut nearest_iscts = Vec::new();
    for i in 0..360 {
        let rot = Affine::rotate_about((i as f64).to_radians(), center_of_mass);
        let ray = rot * ray;
        svg.push_str(&format!("  <line stroke=\"lightblue\" stroke-width=\"{w_ray}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" />\n",
            ray.p0.x, ray.p0.y, ray.p1.x, ray.p1.y));            

        let nearest_isct = path.segments()
            .flat_map(|s| s.intersect_line(ray).into_iter().map(move |i| (i, s)))
            .reduce(|acc, e| {
                if acc.0.line_t <= e.0.line_t {
                    acc
                } else {
                    e
                }
            });
        rays.push(ray);
        nearest_iscts.push(nearest_isct);
    }

    let n_isct: i32 = nearest_iscts.iter().map(|i| i.map(|_| 1).unwrap_or_default()).sum();
    eprintln!("{n_isct} intersections");
    for (isct, ray) in nearest_iscts.iter().zip(rays) {
        let Some((isct, seg)) = isct else {
            continue;
        };
        //let pt = ((1.0 - isct.line_t) * ray.p0.to_vec2() + isct.line_t * ray.p1.to_vec2()).to_point();
        let pt = seg.eval(isct.segment_t);

        svg.push_str(&format!("  <circle r=\"{r_isct}\" "));
        svg.push_str(&format!("cx=\"{}\" cy=\"{}\" ", pt.x, pt.y));
        svg.push_str("fill=\"cyan\"");
        svg.push_str("/>\n");
    }

    svg.push_str("</svg>\n");

    eprintln!("Writing {}", args.output_svg);
    fs::write(Path::new(&args.output_svg), &svg).expect("To write output file");
}
