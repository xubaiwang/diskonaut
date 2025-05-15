use std::{
    env, io,
    path::PathBuf,
    process,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, park_timeout},
    time,
};

use argh::FromArgs;
use crossterm::{
    event::{Event as BackEvent, KeyCode, KeyEvent, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use jwalk::{
    Parallelism::{RayonDefaultPool, Serial},
    WalkDir,
};
use tui::backend::{Backend, CrosstermBackend};

use app::{App, UiMode};
use input::TerminalEvents;
use messages::{Event, Instruction, handle_events};

#[cfg(test)]
mod tests;

mod app;
mod input;
mod messages;
mod os;
mod state;
mod ui;

#[cfg(not(test))]
const SHOULD_SHOW_LOADING_ANIMATION: bool = true;
#[cfg(test)]
const SHOULD_SHOW_LOADING_ANIMATION: bool = false;
#[cfg(not(test))]
const SHOULD_HANDLE_WIN_CHANGE: bool = true;
#[cfg(test)]
const SHOULD_HANDLE_WIN_CHANGE: bool = false;
#[cfg(not(test))]
const SHOULD_SCAN_HD_FILES_IN_MULTIPLE_THREADS: bool = true;
#[cfg(test)]
const SHOULD_SCAN_HD_FILES_IN_MULTIPLE_THREADS: bool = false;

/// Terminal disk space visual navigator
#[derive(FromArgs)]
pub struct Opt {
    /// the folde to scan
    #[argh(option)]
    folder: Option<PathBuf>,
    /// show file sizes rather than their block usage on disk
    #[argh(switch, short = 'a')]
    apparent_size: bool,
    /// don't ask for confirmation before deleting
    #[argh(switch, short = 'd')]
    disable_delete_confirmation: bool,
}

fn main() {
    if let Err(err) = try_main() {
        println!("Error: {}", err);
        process::exit(2);
    }
}
fn get_stdout() -> io::Result<io::Stdout> {
    Ok(io::stdout())
}

fn try_main() -> anyhow::Result<()> {
    let opts: Opt = argh::from_env();

    match get_stdout() {
        Ok(stdout) => {
            enable_raw_mode()?;
            let terminal_backend = CrosstermBackend::new(stdout);
            let terminal_events = TerminalEvents {};
            let folder = match opts.folder {
                Some(folder) => folder,
                None => env::current_dir()?,
            };
            if !folder.as_path().is_dir() {
                anyhow::bail!("Folder '{}' does not exist", folder.to_string_lossy())
            }
            start(
                terminal_backend,
                Box::new(terminal_events),
                folder,
                opts.apparent_size,
                opts.disable_delete_confirmation,
            );
        }
        Err(_) => anyhow::bail!("Failed to get stdout: are you trying to pipe 'diskonaut'?"),
    }
    disable_raw_mode()?;
    Ok(())
}

pub fn start<B>(
    terminal_backend: B,
    terminal_events: Box<dyn Iterator<Item = BackEvent> + Send>,
    path: PathBuf,
    show_apparent_size: bool,
    disable_delete_confirmation: bool,
) where
    B: Backend + Send + 'static,
{
    let mut active_threads = vec![];

    let (event_sender, event_receiver): (SyncSender<Event>, Receiver<Event>) =
        mpsc::sync_channel(1);
    let (instruction_sender, instruction_receiver): (
        SyncSender<Instruction>,
        Receiver<Instruction>,
    ) = mpsc::sync_channel(100);

    let running = Arc::new(AtomicBool::new(true));
    let loaded = Arc::new(AtomicBool::new(false));

    active_threads.push(
        thread::Builder::new()
            .name("event_executer".to_string())
            .spawn({
                let instruction_sender = instruction_sender.clone();
                || handle_events(event_receiver, instruction_sender)
            })
            .unwrap(),
    );

    active_threads.push(
        thread::Builder::new()
            .name("stdin_handler".to_string())
            .spawn({
                let instruction_sender = instruction_sender.clone();
                let running = running.clone();
                move || {
                    for evt in terminal_events {
                        if let BackEvent::Resize(_x, _y) = evt {
                            if SHOULD_HANDLE_WIN_CHANGE {
                                let _ = instruction_sender.send(Instruction::ResetUiMode);
                                let _ = instruction_sender.send(Instruction::Render);
                            }
                            continue;
                        }

                        if let BackEvent::Key(KeyEvent {
                            code: KeyCode::Char('y'),
                            modifiers: KeyModifiers::NONE,
                        })
                        | BackEvent::Key(KeyEvent {
                            code: KeyCode::Char('q'),
                            modifiers: KeyModifiers::NONE,
                        })
                        | BackEvent::Key(KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers: KeyModifiers::CONTROL,
                        }) = evt
                        {
                            // not ideal, but works in a pinch
                            let _ = instruction_sender.send(Instruction::Keypress(evt));
                            park_timeout(time::Duration::from_millis(100));
                            // if we don't wait, the app won't have time to quit
                            if !running.load(Ordering::Acquire) {
                                // sometimes ctrl-c doesn't shut down the app
                                // (eg. dismissing an error message)
                                // in order not to be aware of those particularities
                                // we check "running"
                                break;
                            }
                        } else if instruction_sender.send(Instruction::Keypress(evt)).is_err() {
                            break;
                        }
                    }
                }
            })
            .unwrap(),
    );

    active_threads.push(
        thread::Builder::new()
            .name("hd_scanner".to_string())
            .spawn({
                let path = path.clone();
                let instruction_sender = instruction_sender.clone();
                let loaded = loaded.clone();
                move || {
                    'scanning: for entry in WalkDir::new(&path)
                        .parallelism(if SHOULD_SCAN_HD_FILES_IN_MULTIPLE_THREADS {
                            RayonDefaultPool
                        } else {
                            Serial
                        })
                        .skip_hidden(false)
                        .follow_links(false)
                        .into_iter()
                    {
                        let instruction_sent = match entry {
                            Ok(entry) => match entry.metadata() {
                                Ok(file_metadata) => {
                                    let entry_path = entry.path();
                                    instruction_sender.send(Instruction::AddEntryToBaseFolder((
                                        file_metadata,
                                        entry_path,
                                    )))
                                }
                                Err(_) => {
                                    instruction_sender.send(Instruction::IncrementFailedToRead)
                                }
                            },
                            Err(_) => instruction_sender.send(Instruction::IncrementFailedToRead),
                        };
                        if instruction_sent.is_err() {
                            // if we fail to send an instruction here, this likely means the program has
                            // ended and we need to break this loop as well in order not to hang
                            break 'scanning;
                        };
                    }
                    let _ = instruction_sender.send(Instruction::StartUi);
                    loaded.store(true, Ordering::Release);
                }
            })
            .unwrap(),
    );

    if SHOULD_SHOW_LOADING_ANIMATION {
        active_threads.push(
            thread::Builder::new()
                .name("loading_loop".to_string())
                .spawn({
                    let instruction_sender = instruction_sender.clone();
                    let running = running.clone();
                    move || {
                        while running.load(Ordering::Acquire) && !loaded.load(Ordering::Acquire) {
                            let _ =
                                instruction_sender.send(Instruction::ToggleScanningVisualIndicator);
                            let _ = instruction_sender.send(Instruction::RenderAndUpdateBoard);
                            park_timeout(time::Duration::from_millis(100));
                        }
                    }
                })
                .unwrap(),
        );
    }

    let mut app = App::new(
        terminal_backend,
        path,
        event_sender,
        show_apparent_size,
        disable_delete_confirmation,
    );
    app.start(instruction_receiver);
    running.store(false, Ordering::Release);

    for thread_handler in active_threads {
        thread_handler.join().unwrap();
    }
}
