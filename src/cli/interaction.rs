use std::io::{stdin, stdout, BufRead, Write};

use eyre::{eyre, WrapErr};
use owo_colors::OwoColorize;

// Do not try to get clever!
//
// Mac is extremely janky if you `curl $URL | sudo sh` and the TTY may not be set up right.
// The below method was adopted from Rustup at https://github.com/rust-lang/rustup/blob/3331f34c01474bf216c99a1b1706725708833de1/src/cli/term2.rs#L37
pub(crate) async fn confirm(question: impl AsRef<str>, default: bool) -> eyre::Result<bool> {
    let stdout = stdout();
    let mut term =
        term::terminfo::TerminfoTerminal::new(stdout).ok_or(eyre!("Couldn't get terminal"))?;
    let with_confirm = format!(
        "\
        {question}\n\
        \n\
        {are_you_sure} ({yes}/{no}): \
    ",
        question = question.as_ref(),
        are_you_sure = "Proceed?".bold(),
        no = "N".red().bold(),
        yes = "y".green(),
    );

    term.write_all(with_confirm.as_bytes())?;
    term.flush()?;

    let input = read_line()?;

    let r = match &*input.to_lowercase() {
        "y" | "yes" => true,
        "n" | "no" => false,
        "" => default,
        _ => false,
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
