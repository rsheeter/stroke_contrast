use std::{
    collections::HashSet,
    env::home_dir,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

use clap::Parser;
use fontdrasil::coords::UserCoord;
use gf_metadata::GoogleFonts;
use regex::Regex;
use skrifa::{MetadataProvider, Tag};
use stroke_contrast::{WidthReader, csv_fragment, locations_of_interest, normalization_scale};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Where the Google Fonts github repo is cloned
    #[arg(long, default_value = "~/oss/fonts")]
    gf_repo: String,

    /// Family path filter, retain only paths that contain this regex.
    #[arg(long)]
    family_filter: Option<String>,

    /// Tag filter, retain only families that have tags that contain this regex.
    #[arg(long)]
    tag_filter: String,

    /// What file stores values
    #[arg(long, default_value = "~/oss/fonts/tags/all/experimental_quant.csv")]
    target: String,
}

fn flag_path(flag: &str) -> PathBuf {
    if flag.starts_with("~/") {
        let mut d = home_dir().expect("Must have a home dir");
        d.push(&flag[2..]);
        d
    } else {
        PathBuf::from(flag)
    }
}

fn main() {
    const STROKE_WIDTH_MIN_TAG: &str = "/quant/stroke_width_min";
    const STROKE_WIDTH_MAX_TAG: &str = "/quant/stroke_width_max";
    const WGHT_TAG: Tag = Tag::new(b"wght");
    const ITAL_TAG: Tag = Tag::new(b"ital");

    let args = Args::parse();

    let tag_filter = Regex::new(&args.tag_filter).expect("A valid tag filter");
    let family_filter = args
        .family_filter
        .map(|f| Regex::new(&f).expect("A valid filter regex"));

    let gf_repo = flag_path(&args.gf_repo);
    let target_file = flag_path(&args.target);

    println!("Loading from {gf_repo:?}");
    let gf = GoogleFonts::new(gf_repo, family_filter);

    println!("Writing tags to {target_file:?}");
    let existing_tags = gf
        .tags()
        .expect("To read tags")
        .iter()
        .map(|t| (t.family.as_str(), t.tag.as_str()))
        .collect::<HashSet<_>>();

    let family_names = gf
        .tags()
        .expect("To read tags")
        .iter()
        .filter_map(|t| tag_filter.find(&t.tag).map(|_| t.family.as_str()))
        .collect::<HashSet<_>>();
    let mut families = gf
        .families()
        .iter()
        .filter_map(|(p, f)| f.as_ref().ok().and_then(|f| Some((p, f))))
        .collect::<Vec<_>>();
    families.sort_by_key(|(_, f)| f.name());

    for (local_path, family, font) in families
        .iter()
        .filter(|(_, f)| family_names.contains(f.name()))
        .flat_map(|(p, f)| f.fonts.iter().map(move |font| (p, f, font)))
    {
        // TODO: we should check if we have values for the exact set of user loc and redo
        // the entire family - meaning delete old values and write new ones - if not
        let has_min = existing_tags.contains(&(family.name(), STROKE_WIDTH_MIN_TAG));
        let has_max = existing_tags.contains(&(family.name(), STROKE_WIDTH_MAX_TAG));
        if has_min != has_max {
            panic!("{} has only ONE of stroke min/max", family.name());
        }
        if has_min {
            println!("Skip {}, has values already", family.name());
            continue;
        }

        let mut font_path = (*local_path).clone();
        font_path.pop();
        font_path.push(font.filename());

        let raw_font =
            fs::read(&font_path).unwrap_or_else(|e| panic!("Unable to read {font_path:?}: {e}"));
        let font_ref = skrifa::FontRef::new(&raw_font).expect("A font");

        if font_ref.charmap().map('o').is_none() {
            eprintln!("Measurement char not supported by {}", font.filename());
            continue;
        }

        let mut user_locs = locations_of_interest(&font_ref);
        let scale = normalization_scale(&font_ref);
        let italic = match font.style() {
            "italic" => true,
            "normal" => false,
            _ => panic!("What is the style {}", font.style()),
        };

        for user_loc in user_locs.iter_mut() {
            if !user_loc.contains(WGHT_TAG) {
                user_loc.insert(WGHT_TAG, UserCoord::new(font.weight()));
            }
            if !user_loc.contains(ITAL_TAG) && italic {
                user_loc.insert(ITAL_TAG, UserCoord::new(1));
            }
        }

        let mut tag_lines = Vec::new();
        for user_loc in user_locs {
            let norm_loc = font_ref.axes().location(
                &user_loc
                    .iter()
                    .map(|(tag, coord)| (tag.clone(), coord.to_f64() as f32))
                    .collect::<Vec<_>>(),
            );
            let builder = WidthReader::new(&raw_font, 'o', &norm_loc);

            let width_candidates = builder.cast_rays_around_center_of_mass();
            // Emit tags in normalized scale

            tag_lines.push(format!(
                "{},{},{STROKE_WIDTH_MIN_TAG},{:.2}",
                family.name(),
                csv_fragment(&user_loc),
                width_candidates.min_width * scale
            ));
            tag_lines.push(format!(
                "{},{},{STROKE_WIDTH_MAX_TAG},{:.2}",
                family.name(),
                csv_fragment(&user_loc),
                width_candidates.max_width * scale
            ));
        }

        let mut file = OpenOptions::new()
            .append(true)
            .open(&target_file)
            .expect("To open output file");
        for line in tag_lines.iter() {
            writeln!(file, "{line}").expect("To write target file");
        }
        println!("Wrote {} tag lines for {}", tag_lines.len(), family.name());
    }
}
