use clap::Parser;

#[derive(Debug, Default, Copy, Clone, clap::ValueEnum)]
pub(crate) enum SegmentSelection {
    /// Cast rays from center of mass, stopping at nearest path segment
    #[default]
    CenterOfMass,
    /// Cast multiple rays perpendicular to each path segment
    AllSegments,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    /// Where to save svg files
    #[arg(short, long, default_value = "/tmp/an.svg")]
    pub(crate) output_svg: String,

    /// The text to draw. You probably want to just leave it as o.
    #[arg(short, long, default_value_t = 'o')]
    pub(crate) char: char,

    /// The font to process
    #[arg(long)]
    pub(crate) font: String,

    /// Debug html
    #[arg(long)]
    pub(crate) debug_html: Option<String>,

    /// How to cast rays to discover strokes
    #[arg(long)]
    pub(crate) method: SegmentSelection,

    /// Whether to draw rays in the output svg
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub(crate) show_rays: bool,

    /// Set the log level, either globally or per module.
    ///
    /// See <https://docs.rs/env_logger/latest/env_logger/#enabling-logging> for format.
    #[arg(long)]
    pub(crate) log: Option<String>,
}
