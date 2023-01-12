use std::io::{stdin, stdout, BufRead, Write};

use eyre::{eyre, WrapErr};
use owo_colors::OwoColorize;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PromptChoice {
    Yes,
    No,
    Explain,
}

// Do not try to get clever!
//
// Mac is extremely janky if you `curl $URL | sudo sh` and the TTY may not be set up right.
// The below method was adopted from Rustup at https://github.com/rust-lang/rustup/blob/3331f34c01474bf216c99a1b1706725708833de1/src/cli/term2.rs#L37
pub(crate) async fn prompt(
    question: impl AsRef<str>,
    default: PromptChoice,
    explaining: bool,
) -> eyre::Result<PromptChoice> {
    let stdout = stdout();
    let mut term =
        term::terminfo::TerminfoTerminal::new(stdout).ok_or(eyre!("Couldn't get terminal"))?;
    let with_confirm = format!(
        "\
        {question}\n\
        \n\
        {are_you_sure} ({yes}/{no}{maybe_explain}): \
    ",
        question = question.as_ref(),
        are_you_sure = "Proceed?".bold(),
        no = if default == PromptChoice::No {
            "N"
        } else {
            "n"
        }
        .red(),
        yes = if default == PromptChoice::Yes {
            "Y"
        } else {
            "n"
        }
        .green(),
        maybe_explain = if !explaining {
            format!(
                "/{}",
                if default == PromptChoice::Explain {
                    "E"
                } else {
                    "e"
                }
                .green()
            )
        } else {
            "".into()
        },
    );

    term.write_all(with_confirm.as_bytes())?;
    term.flush()?;

    let input = read_line()?;

    let r = match &*input.to_lowercase() {
        "y" | "yes" => PromptChoice::Yes,
        "n" | "no" => PromptChoice::No,
        "e" | "explain" => PromptChoice::Explain,
        "" => default,
        _ => PromptChoice::No,
    };

    Ok(r)
}

pub(crate) fn read_line() -> eyre::Result<String> {
    let stdin = stdin();
    let stdin = stdin.lock();
    let mut lines = stdin.lines();
    let lines = lines.next().transpose()?;
    match lines {
        None => Err(eyre!("no lines found from stdin")),
        Some(v) => Ok(v),
    }
    .context("unable to read from stdin for confirmation")
}

pub(crate) async fn clean_exit_with_message(message: impl AsRef<str>) -> ! {
    eprintln!("{}", message.as_ref());
    std::process::exit(0)
}
