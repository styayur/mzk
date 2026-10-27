use clap::{Parser, Subcommand};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Play one character-matched song through Jamendo and mpv"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Find and play a song; text is treated literally, without intent analysis
    P {
        #[arg(value_parser = validate_text, allow_hyphen_values = true)]
        text: String,
    },
}

pub fn validate_text(text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("text must not be empty".into());
    }
    if text.len() > 4096 || text.graphemes(true).count() > 256 {
        return Err("text must be at most 256 graphemes and 4096 UTF-8 bytes".into());
    }
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_unicode_and_only_one_command() {
        for text in [
            "Clair de lune",
            "我希望明天是晴天",
            "é日本語",
            "123",
            "x != y; --stop",
            "--flag",
        ] {
            let cli = Cli::try_parse_from(["mzk", "p", text]).unwrap();
            let Command::P { text: actual } = cli.command;
            assert_eq!(actual, text);
        }
        for args in [
            vec!["mzk"],
            vec!["mzk", "c"],
            vec!["mzk", "stop"],
            vec!["mzk", "p"],
            vec!["mzk", "p", ""],
            vec!["mzk", "p", "  "],
            vec!["mzk", "p", "a", "b"],
            vec!["mzk", "--unknown"],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
        assert!(Cli::try_parse_from(["mzk", "p", &"中".repeat(257)]).is_err());
        assert!(validate_text(&format!("x{}", "\u{301}".repeat(4096))).is_err());
    }
}
