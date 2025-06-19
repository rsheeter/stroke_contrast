use std::{collections::HashSet, env::home_dir, fs, path::PathBuf};

use clap::Parser;
use fontdrasil::coords::UserCoord;
use gf_metadata::GoogleFonts;
use regex::Regex;
use skrifa::{MetadataProvider, Tag};
use stroke_contrast::{csv_fragment, locations_of_interest, normalization_scale, WidthReader};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Where to save svg files
    #[arg(long, default_value = "~/oss/fonts")]
    gf_repo: String,

    /// Family path filter, retain only paths that contain this regex.
    #[arg(long)]
    family_filter: Option<String>,

    /// Tag filter, retain only families that have tags that contain this regex.
    #[arg(long)]
    tag_filter: String,
}

fn main() {
    const WGHT_TAG: Tag = Tag::new(b"wght");
    const ITAL_TAG: Tag = Tag::new(b"ital");

    let args = Args::parse();

    let tag_filter = Regex::new(&args.tag_filter).expect("A valid tag filter");
    let family_filter = args
        .family_filter
        .map(|f| Regex::new(&f).expect("A valid filter regex"));

    let gf_repo = if args.gf_repo.starts_with("~/") {
        let mut d = home_dir().expect("Must have a home dir");
        d.push(&args.gf_repo[2..]);
        d
    } else {
        PathBuf::from(&args.gf_repo)
    };

    eprintln!("Loading from {gf_repo:?}");
    let gf = GoogleFonts::new(gf_repo, family_filter);

    let mut char_not_supported = Vec::new();
    let family_names = gf
        .tags()
        .expect("To read tags")
        .iter()
        .filter_map(|t| tag_filter.find(&t.tag).map(|_| t.family.as_str()))
        .collect::<HashSet<_>>();
    for (local_path, family, font) in gf
        .families()
        .iter()
        .filter_map(|(p, f)| f.as_ref().ok().and_then(|f| Some((p, f))))
        .filter(|(_, f)| family_names.contains(f.name()))
        .flat_map(|(p, f)| f.fonts.iter().map(move |font| (p, f, font)))
    {
        let mut font_path = local_path.clone();
        font_path.pop();
        font_path.push(font.filename());

        let raw_font =
            fs::read(&font_path).unwrap_or_else(|e| panic!("Unable to read {font_path:?}: {e}"));
        let font_ref = skrifa::FontRef::new(&raw_font).expect("A font");

        if font_ref.charmap().map('o').is_none() {
            char_not_supported.push(font.filename());
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
            println!(
                "{}, {}, /quant/stroke_width_min, {:.2}",
                family.name(),
                csv_fragment(&user_loc),
                width_candidates.min_width * scale
            );
            println!(
                "{}, {}, /quant/stroke_width_max, {:.2}",
                family.name(),
                csv_fragment(&user_loc),
                width_candidates.max_width * scale
            );
        }
    }

    for unsupported in char_not_supported {
        eprintln!("Measurement char not supported by {unsupported}");
    }
}
