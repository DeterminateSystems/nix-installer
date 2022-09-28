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
        are_you_sure = "Are you sure?".bright_white().bold(),
        no = "N".red().bold(),
        yes = "y".green(),
    );

    stdout.write_all(with_confirm.as_bytes()).await?;
    stdout.flush().await?;

    crossterm::terminal::enable_raw_mode()?;
    let mut reader = EventStream::new();
    let retval = loop {
        let event = reader.next().fuse().await;
        match event {
            Some(Ok(event)) => {
                if let crossterm::event::Event::Key(key) = event {
                    match key.code {
                        KeyCode::Enter => continue, // Many users will hit enter accidentally when they are agreeing/declining prompts.
                        // TODO(@hoverbear): Should maybe actually even wait for it?
                        KeyCode::Char('y') => break Ok(true),
                        _ => {
                            stdout
                                .write_all("Cancelled!".red().to_string().as_bytes())
                                .await?;
                            stdout.flush().await?;
                            break Ok(false);
                        },
                    }
                }
            },
            Some(Err(err)) => break Err(err).wrap_err("Getting response"),
            None => break Err(eyre!("Bailed, no confirmation event")),
        }
    };
    crossterm::terminal::disable_raw_mode()?;
    retval
}

pub(crate) async fn clean_exit_with_message(message: impl AsRef<str>) -> ! {
    eprintln!("{}", message.as_ref());
    std::process::exit(0)
}
