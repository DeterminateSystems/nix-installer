use crossterm::event::{EventStream, KeyCode};
use eyre::{eyre, WrapErr};
use futures::{FutureExt, StreamExt};
use owo_colors::OwoColorize;
use tokio::io::AsyncWriteExt;

pub(crate) async fn confirm(question: impl AsRef<str>) -> eyre::Result<bool> {
    let mut stdout = tokio::io::stdout();
    let with_confirm = format!(
        "\
        {question}\n\
        \n\
        {are_you_sure} ({yes}/{no}): \
    ",
        question = question.as_ref(),
        are_you_sure = "Proceed?".bright_white().bold(),
        no = "N".red().bold(),
        yes = "y".green(),
    );

    stdout.write_all(with_confirm.as_bytes()).await?;
    stdout.flush().await?;

    // crossterm::terminal::enable_raw_mode()?;
    let mut reader = EventStream::new();

    let retval = loop {
        let event = reader.next().fuse().await;
        match event {
            Some(Ok(event)) => {
                if let crossterm::event::Event::Key(key) = event {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            stdout
                                .write_all("Confirmed!\n".green().to_string().as_bytes())
                                .await?;
                            stdout.flush().await?;
                            break Ok(true);
                        },
                        KeyCode::Char('N') | KeyCode::Char('n') => {
                            stdout
                                .write_all("Cancelled!\n".red().to_string().as_bytes())
                                .await?;
                            stdout.flush().await?;
                            break Ok(false);
                        },
                        KeyCode::Enter | _ => continue,
                    }
                }
            },
            Some(Err(err)) => break Err(err).wrap_err("Getting response"),
            None => break Err(eyre!("Bailed, no confirmation event")),
        }
    };
    // crossterm::terminal::disable_raw_mode()?;
    retval
}

pub(crate) async fn clean_exit_with_message(message: impl AsRef<str>) -> ! {
    eprintln!("{}", message.as_ref());
    std::process::exit(0)
}
