use fern::{
    colors::{Color, ColoredLevelConfig},
    Dispatch,
};

#[tokio::main]
async fn main() {
    // configure the logger
    let colors = ColoredLevelConfig::new()
        .debug(Color::BrightBlue)
        .info(Color::Green)
        .warn(Color::Yellow)
        .error(Color::Red);

    Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} {}",
                format!(
                    "\x1B[{}m>>\x1B[0m",
                    colors.get_color(&record.level()).to_fg_str()
                ),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()
        .expect("Failed to apply logger");

    let matches = clap::App::new("brust launcher installer")
        .about("Installs/looks for the Brickadia launcher")
        .author("voximity")
        .get_matches();

    launcher::install(&matches).await;
}
