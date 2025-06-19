use std::{env::home_dir, fs, path::PathBuf};

use args::{Args, SegmentSelection};
use clap::Parser;
use fontdrasil::coords::UserLocation;
use log::info;
use read_fonts::types::NameId;
use skrifa::{MetadataProvider, raw::TableProvider};
use stroke_contrast::{WidthReader, csv_fragment, locations_of_interest, normalization_scale};

mod args;

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
