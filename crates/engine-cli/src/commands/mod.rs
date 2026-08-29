pub mod import;
pub mod inspect;
pub mod render;
pub mod replay;
pub mod run;
pub mod test;

#[derive(Copy, Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

impl OutputFormat {
    pub fn is_json(self) -> bool {
        self == OutputFormat::Json
    }
}
