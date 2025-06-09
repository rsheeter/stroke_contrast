use std::{env::home_dir, fs, path::PathBuf};

use args::{Args, SegmentSelection};
use clap::Parser;
use fontdrasil::coords::{UserCoord, UserLocation};
use harfruzz::{GlyphBuffer, ShaperFont};
use kurbo::{
    Affine, BezPath, Circle, Line, ParamCurve, ParamCurveNearest, PathSeg, Point, Rect, Shape, Vec2,
};
use log::{debug, info, warn};
use ordered_float::OrderedFloat;
use read_fonts::types::{F2Dot14, NameId};
use skrifa::{
    MetadataProvider, Tag,
    instance::Location,
    outline::{DrawSettings, OutlinePen},
    prelude::{LocationRef, Size},
    raw::TableProvider,
};

mod args;

trait Tangent {
    // Returns (point at t, vector in direction of tangent)
    fn tangent(self, t: f64) -> (Point, Vec2);
}

impl Tangent for PathSeg {
    fn tangent(self, t: f64) -> (Point, Vec2) {
        // <https://en.wikipedia.org/wiki/B%C3%A9zier_curve>

        match self {
            PathSeg::Line(line) => {
                let curr = line.eval(t);
                let tan = line.p1 - line.p0;
                (curr, tan)
            }
            PathSeg::Quad(quad) => {
                // B'(t) = 2(1-t)(p1-p0)+2t(p2-p1)
                let curr = quad.eval(t);
                let tan = 2.0 * (1.0 - t) * (quad.p1 - quad.p0) + 2.0 * t * (quad.p2 - quad.p1);
                (curr, tan)
            }
            PathSeg::Cubic(_cubic) => {
                // B'(t) = 3(1-t)^2(p1-p0) + 6(1-t)t(p2 - p1) + 3 * t^2 * (p3 - p2)
                todo!("Implement cubic per comment above")
            }
        }
    }
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
fn shape(text: &str, font: &harfruzz::FontRef, loc: &LocationRef) -> GlyphBuffer {
    let coords = loc
        .coords()
        .iter()
        .map(|v| F2Dot14::from_f32(v.to_f32()))
        .collect::<Vec<_>>();
    let shaper_font = ShaperFont::new(font);
    let face = shaper_font.shaper(font, &coords);

    let mut buffer = harfruzz::UnicodeBuffer::new();
    buffer.push_str(text);

    harfruzz::shape(&face, &[], buffer)
}

struct WidthReader {
    path: BezPath,
    bbox: Rect,
    max_dim: f64,
    ray_width: f64,
}

impl WidthReader {
    fn new(raw_font: &[u8], ch: char, loc: &Location) -> Self {
        let harf_font_ref =
            harfruzz::FontRef::new(&raw_font).expect("For font files to be font files!");
        let skrifa_font_ref = skrifa::FontRef::new(&raw_font).expect("Fonts to be fonts");

        let outlines = skrifa_font_ref.outline_glyphs();
        let mut pen = PathPen::default();

        let glyphs = shape(&format!("{}", ch), &harf_font_ref, &LocationRef::from(loc));
        for (glyph_info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
            let glyph = outlines
                .get(glyph_info.glyph_id.into())
                .expect("Glyphs to exist!");
            glyph
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), LocationRef::from(loc)),
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
        let max_dim = bbox.width().max(bbox.height());
        let margin = 0.03 * max_dim;
        let bbox = bbox.inflate(margin, margin).expand();
        let ray_width = margin / 64.0;
        Self {
            path,
            bbox,
            max_dim,
            ray_width,
        }
    }

    /// Spray rays from center of mass. Currently baselessly assumes center of mass will be uninked.
    fn cast_rays_around_center_of_mass(&self) -> WidthCandidates {
        // Brute force discovery of interior pixels and center of mass
        // TODO: migrate to analytic solution once available in kurbo
        let bbox = self.bbox;
        let mut live = Vec::new();
        let mut num_filled = 0;
        let mut num_unfilled = 0;
        for x in (bbox.min_x() as i32..(bbox.min_x() + bbox.width()) as i32)
            .step_by((bbox.width() / 100.0).floor() as usize)
        {
            for y in (bbox.min_y() as i32..(bbox.min_y() + bbox.height()) as i32)
                .step_by((bbox.height() / 100.0).floor() as usize)
            {
                if self.path.winding(Point {
                    x: x as f64,
                    y: y as f64,
                }) != 0
                {
                    live.push((x as f64, y as f64));
                    num_filled += 1;
                } else {
                    num_unfilled += 1;
                }
            }
        }

        if num_filled as f64 > 0.75 * (num_filled as f64 + num_unfilled as f64) {
            warn!("OMG reverse video?! TODO: invert winding?");
        };

        let (sum_x, sum_y) = live
            .iter()
            .fold((0.0, 0.0), |acc, e| (acc.0 + e.0, acc.1 + e.1));
        let center_of_mass = Point::new(sum_x / live.len() as f64, sum_y / live.len() as f64);
        if self.path.winding(center_of_mass) != 0 {
            panic!("Being filled at center of mass not supported for this method");
        }
        // svg.push_str(&format!("  <circle r=\"{margin}\" "));
        // svg.push_str(&format!("cx=\"{}\" cy=\"{}\" ", center_of_mass.x, center_of_mass.y));
        // svg.push_str("fill=\"purple\"");
        // svg.push_str("/>\n");

        // Spray rays passing through center of mass
        let ray = self.make_x_ray(center_of_mass);
        let mut rays = Vec::new();
        let mut ribs = Vec::new();
        for i in 0..360 {
            //for i in 0..1 {
            let rot = Affine::rotate_about((i as f64).to_radians(), center_of_mass);
            let ray = rot
                * Line {
                    p0: center_of_mass,
                    p1: ray.p1,
                };

            // Find the nearest intersection with a segment, if any
            let Some((isct, seg)) = self
                .path
                .segments()
                .flat_map(|s| s.intersect_line(ray).into_iter().map(move |i| (i, s)))
                .reduce(|acc, e| if acc.0.line_t <= e.0.line_t { acc } else { e })
            else {
                // Swing and a miss
                rays.push(ray);
                continue;
            };

            // Find the next nearest intersection along the normal away from center of mass
            let (pt, tan) = seg.tangent(isct.segment_t);
            let normal1 = tan.turn_90();
            let normal2 = -normal1;
            let pn1 = pt + normal1;
            let pn2 = pt + normal2;
            let away_from_center =
                if (pn1 - center_of_mass).length() > (pn2 - center_of_mass).length() {
                    normal1
                } else {
                    normal2
                };

            // If away from center is wildly divergent from ray discard it
            // This helps with things like Kablammo taking readings from outcrops
            // Annoyingly it also over-drops for Lobster
            // if (away_from_center.angle() - (ray.p1 - ray.p0).angle()).abs() > 30_f64.to_radians() {
            //     debug!("Discard!");
            //     continue;
            // }

            // record our ray as far as the point of intersection
            rays.push(Line {
                p0: center_of_mass,
                p1: pt,
            });

            // new ray perpendicular to isct
            let ray = Affine::rotate_about(away_from_center.angle(), pt) * self.make_x_ray(pt);

            // Keep the nearest candidate only
            if let Some(nearest_candidate) =
                self.ray_to_inked_segments(ray)
                    .into_iter()
                    .reduce(|best, candidate| {
                        if best.nearest(pt, 0.000001).distance_sq
                            <= candidate.nearest(pt, 0.000001).distance_sq
                        {
                            best
                        } else {
                            candidate
                        }
                    })
            {
                ribs.push(nearest_candidate);
            }
        }

        WidthCandidates::new(&self.path, rays, ribs)
    }

    fn cast_rays_from_all_segments(&self) -> WidthCandidates {
        let mut rays = Vec::new();
        let mut ribs = Vec::new();
        for segment in self.path.segments() {
            for i in 0..10 {
                let t = 0.1 * i as f64;
                let (on_path, tangent) = segment.tangent(t);
                let normal = tangent.turn_90();
                let ray = Affine::rotate_about(normal.angle(), on_path) * self.make_x_ray(on_path);
                rays.push(ray);
                // Keep all the candidates
                ribs.extend(self.ray_to_inked_segments(ray));
            }
        }
        WidthCandidates::new(&self.path, rays, ribs)
    }

    // Returns one line segment per continuously inked area encountered
    fn ray_to_inked_segments(&self, ray: Line) -> Vec<Line> {
        let mut intersections = self
            .path
            .segments()
            .flat_map(|s| s.intersect_line(ray).into_iter())
            // Discard interior intersections, e.g. those where we're inked on both sides
            .filter(|isct| {
                let before = ray.eval(isct.line_t - 0.00001);
                let after = ray.eval(isct.line_t + 0.00001);
                let filled_before = self.path.winding(before) != 0;
                let filled_after = self.path.winding(after) != 0;
                // Discard if inked before and after
                !(filled_before && filled_after)
            })
            .collect::<Vec<_>>();
        intersections.sort_by_key(|isct| OrderedFloat(isct.line_t));

        // Sometimes we get the same value repeatedly
        for i in (1..intersections.len()).rev() {
            if (intersections[i].line_t - intersections[i - 1].line_t).abs() < 0.000001 {
                intersections.remove(i);
            }
        }

        let mut results = Vec::new();
        for window in intersections.windows(2) {
            let segment = Line {
                p0: ray.eval(window[0].line_t),
                p1: ray.eval(window[1].line_t),
            };
            // Retain only segments through inked regions
            if self.path.winding(segment.midpoint()) != 0 {
                results.push(segment);
            }
        }
        results
    }

    /// Make a line (-lots, 0) to (+lots, 0)
    fn make_x_ray(&self, through: Point) -> Line {
        Affine::translate(through.to_vec2())
            * Line::new(
                Point {
                    x: -100.0 * self.max_dim,
                    y: 0.0,
                },
                Point {
                    x: 100.0 * self.max_dim,
                    y: 0.0,
                },
            )
    }

    fn debug_svg(&self, show_rays: bool, candidates: &WidthCandidates) -> String {
        let mut svg = String::new();
        svg.push_str(r#"<svg xmlns="http://www.w3.org/2000/svg" version="1.1" viewBox=""#);
        svg.push_str(&format!("{:02} ", self.bbox.min_x()));
        svg.push_str(&format!("{:02} ", self.bbox.min_y()));
        svg.push_str(&format!("{:02} ", self.bbox.width()));
        svg.push_str(&format!("{:02}", self.bbox.height()));
        svg.push_str(r#"">"#);
        svg.push_str("\n");
        svg.push_str("  <path fill=\"darkgray\" d=\"");
        svg.push_str(&self.path.to_svg());
        svg.push_str("\" />\n");

        if show_rays {
            for ray in candidates.rays.iter() {
                svg.push_str(&format!("  <line stroke=\"lightblue\" stroke-width=\"{}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" />\n",
                self.ray_width, ray.p0.x, ray.p0.y, ray.p1.x, ray.p1.y));
            }
        }

        let tolerance = 0.1;
        for (candidate, stroke_dot) in candidates.ribs.iter() {
            let (width, color) = match candidate.length() {
                l if (l - candidates.max_width).abs() <= tolerance => {
                    (3.0 * self.ray_width, "green")
                }
                l if (l - candidates.min_width).abs() <= tolerance => (3.0 * self.ray_width, "red"),
                _ => (self.ray_width, "pink"),
            };
            svg.push_str(&format!("  <line stroke=\"{color}\" stroke-width=\"{}\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" />\n",
                width, candidate.p0.x, candidate.p0.y, candidate.p1.x, candidate.p1.y));

            svg.push_str(&format!("  <circle r=\"{}\" ", stroke_dot.radius));
            svg.push_str(&format!(
                "cx=\"{}\" cy=\"{}\" ",
                stroke_dot.center.x, stroke_dot.center.y
            ));
            svg.push_str(&format!(
                "fill=\"none\" stroke=\"magenta\" stroke-width=\"{}\"",
                self.ray_width
            ));
            svg.push_str("/>\n");
        }

        svg.push_str("</svg>\n");
        svg
    }
}

/// Each candidate is a line segment contained within the inked part of a path that might be a stroke width
#[derive(Debug, Default)]
struct WidthCandidates {
    rays: Vec<Line>,
    ribs: Vec<(Line, Circle)>,
    min_width: f64,
    max_width: f64,
}

impl WidthCandidates {
    fn new(path: &BezPath, rays: Vec<Line>, rib_candidates: Vec<Line>) -> Self {
        // For each each candidate fit a circle around it's midpoint into the inked shape
        let mut min_width = f64::MAX;
        let mut max_width = f64::MIN;
        let ribs = rib_candidates
            .into_iter()
            .filter_map(|candidate| {
                // See if a circle around the midpoint of our line goes into unpainted area
                let mid = candidate.midpoint();

                // Try from the full line through to almost nothing
                // Often times the end (t=0) reports winding 0
                let mut t = 0.0;
                let mut inc = 0.001;
                let mut solution = None;
                while solution.is_none() && t <= 0.1 {
                    // If points around mid are all inked take this as a valid result
                    // TODO: not brute force :)
                    let pt = candidate.eval(t);
                    let samples = 90;
                    if (0..samples).all(|i| {
                        let rot = i as f64 * 360.0 / samples as f64;
                        let pt = Affine::rotate_about(rot.to_radians(), mid) * pt;
                        path.winding(pt) != 0
                    }) {
                        let radius = (pt - mid).length();
                        if radius > 1.0 {
                            let candidate_length = candidate.length();
                            min_width = min_width.min(candidate_length);
                            max_width = max_width.max(candidate_length);
                            solution = Some((candidate, Circle::new(mid, radius)));
                        } else {
                            // Still getting very short line segments sometimes
                            debug!("Suspiciously small rib, {candidate:?}");
                        }
                    }
                    t += inc;
                    inc += inc;
                }
                solution
            })
            .collect::<Vec<_>>();

        Self {
            rays,
            ribs,
            min_width,
            max_width,
        }
    }
}

/// Multiplier to convert font units to normalized (common upem) units
fn normalization_scale(font: &skrifa::FontRef) -> f64 {
    let head = font.head().expect("Must have head");
    let upem = head.units_per_em() as f64;
    1000.0 / upem
}

fn setup_logging(log_filters: Option<&str>) {
    use std::io::Write;
    let mut log_cfg = env_logger::builder();
    log_cfg.format(|buf, record| {
        let ts = buf.timestamp_micros();
        let style = buf.default_level_style(record.level());
        writeln!(
            buf,
            "[{ts} {:?} {} {style}{}{style:#}] {}",
            std::thread::current().id(),
            record.target(),
            record.level(),
            record.args()
        )
    });
    if let Some(log_filters) = log_filters {
        log_cfg.parse_filters(log_filters);
    }
    log_cfg.init();
}

fn locations_of_interest(font: &skrifa::FontRef) -> Vec<UserLocation> {
    const WGHT_TAG: Tag = Tag::new(b"wght");
    let result = vec![UserLocation::new()];

    let Ok(fvar) = font.fvar() else {
        return result;
    };
    let Some(wght_axis) = fvar
        .axes()
        .unwrap()
        .iter()
        .find(|a| a.axis_tag() == WGHT_TAG)
    else {
        return result;
    };

    let mut result = Vec::new();
    for wght in
        (wght_axis.min_value.get().to_i32()..=wght_axis.max_value.get().to_i32()).step_by(100)
    {
        let mut user = UserLocation::new();
        user.insert(WGHT_TAG, UserCoord::new(wght));
        result.push(user);
    }
    result
}

fn name(font: &skrifa::FontRef) -> String {
    let table = font.name().expect("Must have name");
    let nr = table
        .name_record()
        .iter()
        // aren't mismatched copies of read-fonts fun
        .find(|nr| nr.name_id().to_u16() == NameId::FAMILY_NAME.to_u16())
        .expect("Must have a family name");
    let name = nr
        .string(table.string_data())
        .expect("To read name contents");
    name.to_string()
}

fn csv_fragment(user: &UserLocation) -> String {
    user.iter()
        .map(|(tag, coord)| {
            let v = coord.to_f64();
            if v == v.round() {
                format!("{tag}@{}", v as i32)
            } else {
                format!("{tag}@{:.2}", v)
            }
        })
        .collect::<Vec<_>>()
        .join("_")
}

fn filename_fragment(user: &UserLocation) -> String {
    user.iter()
        .map(|(tag, coord)| format!("{tag}{:.2}", coord.to_f64()))
        .collect::<Vec<_>>()
        .join("_")
}

fn main() {
    let args = Args::parse();
    setup_logging(args.log.as_deref());

    let font_path = if args.font.starts_with("~") {
        let mut d = home_dir().expect("Must have a home dir");
        d.push(&args.font[1..]);
        d
    } else {
        PathBuf::from(&args.font)
    };
    let raw_font =
        fs::read(&font_path).unwrap_or_else(|e| panic!("Unable to read {font_path:?}: {e}"));
    let font = skrifa::FontRef::new(&raw_font).expect("A font");

    let locs = locations_of_interest(&font);
    let scale = normalization_scale(&font);
    let name = name(&font);

    let mut debug_html = String::new();
    debug_html.push_str(
        r#"
        <style>
        .grid {
            display: grid;
            grid-template-columns: 1fr 1fr 1fr;
        }
        </style>
        "#,
    );
    debug_html.push_str("<div class=\"grid\">\n");

    for user_loc in locs.iter() {
        let norm_loc = font.axes().location(
            &user_loc
                .iter()
                .map(|(tag, coord)| (tag.clone(), coord.to_f64() as f32))
                .collect::<Vec<_>>(),
        );
        let builder = WidthReader::new(&raw_font, args.char, &norm_loc);

        let width_candidates = match args.method {
            SegmentSelection::CenterOfMass => builder.cast_rays_around_center_of_mass(),
            SegmentSelection::AllSegments => builder.cast_rays_from_all_segments(),
        };

        // Emit tags in normalized scale
        println!(
            "{name}, {}, /quant/stroke_width_min, {:.2}",
            csv_fragment(user_loc),
            width_candidates.min_width * scale
        );
        println!(
            "{name}, {}, /quant/stroke_width_max, {:.2}",
            csv_fragment(user_loc),
            width_candidates.max_width * scale
        );

        let svg = builder.debug_svg(args.show_rays, &width_candidates);

        let output_file = PathBuf::from(&args.output_svg);
        let output_file = output_file.with_file_name(format!(
            "{}{}.{}",
            output_file.file_stem().unwrap().to_str().unwrap(),
            filename_fragment(user_loc),
            output_file.extension().unwrap().to_str().unwrap()
        ));
        info!("Writing {:?}", output_file);
        fs::write(&output_file, &svg).expect("To write output file");

        // debug_html.push_str("<div>\n");
        // debug_html.push_str(output_file.file_stem().unwrap().to_str().unwrap());
        // debug_html.push_str("</div><div>\n");
        debug_html.push_str("<div>\n");
        debug_html.push_str(&svg);
        debug_html.push_str("</div>\n");
    }
    debug_html.push_str("</div>\n");

    if let Some(debug_html_file) = &args.debug_html {
        let debug_html_file = PathBuf::from(&debug_html_file);
        info!("Writing {:?}", debug_html_file);
        fs::write(debug_html_file, &debug_html).expect("To write output file");
    }
}
