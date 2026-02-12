use std::{
    collections::VecDeque,
    io::{self, Write},
};

use anyhow::Result;
use crossterm::{
    cursor,
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    style::{self, Color, Stylize},
    terminal::{self, ClearType},
};
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::types::{CliCommand, DisplayMessage, UiEvent};

const MAX_MESSAGES: usize = 500;
const MAX_INPUT_LEN: usize = 2048;

// ── Screen state ──────────────────────────────────────────────────────────────

#[derive(PartialEq)]
enum Screen {
    MainMenu,
    CreateRoom { step: u8 },
    JoinRoom { step: u8 },
    ChangeNickname,
    Chat,
}

// ── CLI state ─────────────────────────────────────────────────────────────────

struct CliState {
    messages: VecDeque<DisplayMessage>,
    input_buffer: String,
    current_room: Option<String>,
    peer_count: usize,
    /// Currently masking input (password entry).
    masking: bool,
    /// Label shown before the input field (e.g. "Room name: ").
    prompt_label: String,
    /// Current nickname (kept in sync with the app layer).
    nickname: String,
}

impl CliState {
    fn new(nickname: String) -> Self {
        Self {
            messages: VecDeque::new(),
            input_buffer: String::new(),
            current_room: None,
            peer_count: 0,
            masking: false,
            prompt_label: String::new(),
            nickname,
        }
    }

    fn push_message(&mut self, msg: DisplayMessage) {
        if self.messages.len() >= MAX_MESSAGES {
            self.messages.pop_front();
        }
        self.messages.push_back(msg);
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Runs the full CLI lifecycle.  Call from a dedicated Tokio task.
pub async fn run_cli(
    cli_cmd_tx: mpsc::UnboundedSender<CliCommand>,
    ui_event_rx: mpsc::UnboundedReceiver<UiEvent>,
    nickname: String,
) -> Result<()> {
    // Enter alternate screen + raw mode.
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        terminal::EnterAlternateScreen,
        cursor::Hide,
        terminal::Clear(ClearType::All)
    )?;

    let result = cli_inner(cli_cmd_tx, ui_event_rx, &mut stdout, nickname).await;

    // Cleanup — always restore terminal.
    let _ = execute!(
        stdout,
        terminal::LeaveAlternateScreen,
        cursor::Show
    );
    let _ = terminal::disable_raw_mode();

    result
}

// ── Main loop ─────────────────────────────────────────────────────────────────

async fn cli_inner(
    cmd_tx: mpsc::UnboundedSender<CliCommand>,
    mut ui_rx: mpsc::UnboundedReceiver<UiEvent>,
    stdout: &mut io::Stdout,
    nickname: String,
) -> Result<()> {
    let mut state = CliState::new(nickname);
    let mut event_stream = EventStream::new();

    let mut screen = Screen::MainMenu;
    let mut create_name = String::new();
    let mut join_code = String::new();

    draw_main_menu(stdout, &state.nickname)?;

    loop {
        tokio::select! {
            // ── Keyboard input ────────────────────────────────────────
            Some(Ok(event)) = event_stream.next() => {
                match event {
                    Event::Key(key) => {
                        let quit = handle_key(
                            key,
                            &mut state,
                            &mut screen,
                            &mut create_name,
                            &mut join_code,
                            &cmd_tx,
                            stdout,
                        ).await?;
                        if quit { break; }

                        // Redraw after input
                        match &screen {
                            Screen::MainMenu => draw_main_menu(stdout, &state.nickname)?,
                            Screen::CreateRoom { .. }
                            | Screen::JoinRoom { .. }
                            | Screen::ChangeNickname => {
                                redraw_prompt(stdout, &state)?
                            }
                            Screen::Chat => redraw_chat(stdout, &state)?,
                        }
                    }

                    Event::Resize(_, _) => {
                        match &screen {
                            Screen::MainMenu => draw_main_menu(stdout, &state.nickname)?,
                            Screen::Chat => redraw_chat(stdout, &state)?,
                            _ => {}
                        }
                    }

                    _ => {}
                }
            }

            // ── App event (message, status, navigation) ───────────────
            Some(ui_event) = ui_rx.recv() => {
                match ui_event {
                    UiEvent::NewMessage(msg) => {
                        state.push_message(msg);
                        if screen == Screen::Chat {
                            redraw_chat(stdout, &state)?;
                        }
                    }

                    UiEvent::StatusUpdate { room, peers } => {
                        state.current_room = room;
                        state.peer_count = peers;
                        if screen == Screen::Chat {
                            redraw_header(stdout, &state)?;
                        }
                    }

                    UiEvent::RoomCreated { name, code } => {
                        state.messages.clear();
                        state.current_room = Some(name.clone());
                        state.input_buffer.clear();
                        state.masking = false;
                        screen = Screen::Chat;

                        let msg = DisplayMessage::system(&format!(
                            "Room '{}' created. Share this code: {}",
                            name, code
                        ));
                        state.push_message(msg);
                        redraw_chat(stdout, &state)?;
                    }

                    UiEvent::RoomJoined(name) => {
                        state.messages.clear();
                        state.current_room = Some(name.clone());
                        state.input_buffer.clear();
                        state.masking = false;
                        screen = Screen::Chat;

                        let msg = DisplayMessage::system(&format!("Joined room '{}'", name));
                        state.push_message(msg);
                        redraw_chat(stdout, &state)?;
                    }

                    UiEvent::AccessDenied => {
                        state.input_buffer.clear();
                        state.masking = false;
                        let msg = DisplayMessage::system("Access denied — wrong password.");
                        state.push_message(msg);
                        redraw_chat(stdout, &state)?;
                    }

                    UiEvent::ShowMainMenu => {
                        state.messages.clear();
                        state.input_buffer.clear();
                        state.current_room = None;
                        screen = Screen::MainMenu;
                        draw_main_menu(stdout, &state.nickname)?;
                    }

                    UiEvent::NicknameChanged(new_nick) => {
                        state.nickname = new_nick.clone();
                        state.input_buffer.clear();
                        state.prompt_label.clear();
                        screen = Screen::MainMenu;
                        draw_main_menu(stdout, &state.nickname)?;
                    }

                    UiEvent::Error(err) => {
                        let msg = DisplayMessage::system(&format!("[!] {}", err));
                        state.push_message(msg);
                        if screen == Screen::Chat {
                            redraw_chat(stdout, &state)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

// ── Key handling ──────────────────────────────────────────────────────────────

/// Returns `true` when the user confirmed quit.
async fn handle_key(
    key: KeyEvent,
    state: &mut CliState,
    screen: &mut Screen,
    create_name: &mut String,
    join_code: &mut String,
    cmd_tx: &mpsc::UnboundedSender<CliCommand>,
    stdout: &mut io::Stdout,
) -> Result<bool> {
    // Ignore key-release and key-repeat events (Windows sends both Press and Release).
    if key.kind == KeyEventKind::Release {
        return Ok(false);
    }

    // Ctrl-C anywhere → quit
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        let _ = cmd_tx.send(CliCommand::Quit);
        return Ok(true);
    }

    match screen {
        // ── Main menu ─────────────────────────────────────────────────
        Screen::MainMenu => match key.code {
            KeyCode::Char('1') => {
                *screen = Screen::CreateRoom { step: 0 };
                state.input_buffer.clear();
                state.prompt_label = "Room name: ".to_string();
                draw_prompt(stdout, "Room name: ", false)?;
            }
            KeyCode::Char('2') => {
                *screen = Screen::JoinRoom { step: 0 };
                state.input_buffer.clear();
                state.prompt_label = "Room code: ".to_string();
                draw_prompt(stdout, "Room code: ", false)?;
            }
            KeyCode::Char('3') => {
                *screen = Screen::ChangeNickname;
                state.input_buffer.clear();
                let label = format!("New nickname (current: {}): ", state.nickname);
                state.prompt_label = label.clone();
                draw_prompt(stdout, &label, false)?;
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                let _ = cmd_tx.send(CliCommand::Quit);
                return Ok(true);
            }
            _ => {}
        },

        // ── Create room ───────────────────────────────────────────────
        Screen::CreateRoom { step } => match key.code {
            KeyCode::Enter => {
                match step {
                    0 => {
                        *create_name = state.input_buffer.trim().to_string();
                        state.input_buffer.clear();
                        *step = 1;
                        state.masking = true;
                        state.prompt_label = "Password (leave blank for none): ".to_string();
                        draw_prompt(stdout, "Password (leave blank for none): ", true)?;
                    }
                    _ => {
                        let password = state.input_buffer.clone();
                        let name = create_name.clone();
                        state.input_buffer.clear();
                        state.masking = false;
                        let _ = cmd_tx.send(CliCommand::CreateRoom { name, password });
                    }
                }
            }
            KeyCode::Esc => {
                state.input_buffer.clear();
                state.masking = false;
                *screen = Screen::MainMenu;
            }
            _ => handle_text_input(key, &mut state.input_buffer),
        },

        // ── Join room ─────────────────────────────────────────────────
        Screen::JoinRoom { step } => match key.code {
            KeyCode::Enter => {
                match step {
                    0 => {
                        *join_code = state.input_buffer.trim().to_string();
                        state.input_buffer.clear();
                        *step = 1;
                        state.masking = true;
                        state.prompt_label = "Password (leave blank for none): ".to_string();
                        draw_prompt(stdout, "Password (leave blank for none): ", true)?;
                    }
                    _ => {
                        let password = state.input_buffer.clone();
                        let code = join_code.clone();
                        state.input_buffer.clear();
                        state.masking = false;
                        let _ = cmd_tx.send(CliCommand::JoinRoom { code, password });
                    }
                }
            }
            KeyCode::Esc => {
                state.input_buffer.clear();
                state.masking = false;
                *screen = Screen::MainMenu;
            }
            _ => handle_text_input(key, &mut state.input_buffer),
        },

        // ── Change nickname ───────────────────────────────────────────
        Screen::ChangeNickname => match key.code {
            KeyCode::Enter => {
                let new_nick = state.input_buffer.trim().to_string();
                state.input_buffer.clear();
                state.prompt_label.clear();
                if !new_nick.is_empty() {
                    let _ = cmd_tx.send(CliCommand::ChangeNickname(new_nick));
                } else {
                    // Empty input → cancel, return to menu
                    *screen = Screen::MainMenu;
                    draw_main_menu(stdout, &state.nickname)?;
                }
            }
            KeyCode::Esc => {
                state.input_buffer.clear();
                state.prompt_label.clear();
                *screen = Screen::MainMenu;
            }
            _ => handle_text_input(key, &mut state.input_buffer),
        },

        // ── Chat ──────────────────────────────────────────────────────
        Screen::Chat => match key.code {
            KeyCode::Enter => {
                let input = state.input_buffer.trim().to_string();
                state.input_buffer.clear();
                if !input.is_empty() {
                    match input.as_str() {
                        "/quit" => {
                            let _ = cmd_tx.send(CliCommand::LeaveRoom);
                        }
                        "/peers" => {
                            let _ = cmd_tx.send(CliCommand::ListPeers);
                        }
                        "/help" => {
                            let _ = cmd_tx.send(CliCommand::Help);
                        }
                        _ if input.starts_with('/') => {
                            let _ = cmd_tx.send(CliCommand::Help);
                        }
                        _ => {
                            let _ = cmd_tx.send(CliCommand::SendMessage(input));
                        }
                    }
                }
            }
            _ => {
                if state.input_buffer.len() < MAX_INPUT_LEN {
                    handle_text_input(key, &mut state.input_buffer);
                }
            }
        },
    }
    Ok(false)
}

fn handle_text_input(key: KeyEvent, buf: &mut String) {
    match key.code {
        KeyCode::Char(c) => buf.push(c),
        KeyCode::Backspace => { buf.pop(); }
        _ => {}
    }
}

// ── Drawing ───────────────────────────────────────────────────────────────────

fn draw_main_menu(stdout: &mut io::Stdout, nickname: &str) -> Result<()> {
    let (width, height) = terminal::size()?;
    execute!(stdout, terminal::Clear(ClearType::All))?;

    let title = "=== P2P Chat ===";
    let logged_in = format!("Logged in as: {}", nickname);
    let items = [
        "[1] Create room",
        "[2] Join room",
        "[3] Change nickname",
        "[Q] Quit",
    ];

    let start_row = height / 2 - 4;
    let col = (width / 2).saturating_sub(12);

    execute!(stdout, cursor::MoveTo(col, start_row))?;
    execute!(stdout, style::PrintStyledContent(title.bold()))?;

    execute!(stdout, cursor::MoveTo(col, start_row + 1))?;
    execute!(stdout, style::PrintStyledContent(logged_in.dark_grey()))?;

    for (i, item) in items.iter().enumerate() {
        execute!(stdout, cursor::MoveTo(col, start_row + 3 + i as u16))?;
        execute!(stdout, style::Print(item))?;
    }

    execute!(stdout, cursor::MoveTo(col, start_row + 8))?;
    execute!(stdout, style::Print("> "))?;
    execute!(stdout, cursor::Show)?;
    stdout.flush()?;
    Ok(())
}

fn draw_prompt(stdout: &mut io::Stdout, label: &str, _masking: bool) -> Result<()> {
    let (_, height) = terminal::size()?;
    execute!(stdout, cursor::MoveTo(0, height - 1), terminal::Clear(ClearType::CurrentLine))?;
    execute!(stdout, style::Print(label))?;
    execute!(stdout, cursor::Show)?;
    stdout.flush()?;
    Ok(())
}

fn redraw_prompt(stdout: &mut io::Stdout, state: &CliState) -> Result<()> {
    let (width, height) = terminal::size()?;
    execute!(stdout, cursor::MoveTo(0, height - 1), terminal::Clear(ClearType::CurrentLine))?;

    let input_display = if state.masking {
        "•".repeat(state.input_buffer.len())
    } else {
        state.input_buffer.clone()
    };

    // Scroll to end: only show the tail of the input that fits on one line.
    // This prevents long inputs (e.g. room codes) from wrapping and leaving
    // uncleared artefacts on previous lines.
    let label_len = state.prompt_label.chars().count();
    let available = (width as usize).saturating_sub(label_len);
    let char_count = input_display.chars().count();
    let visible_input: String = if char_count > available {
        input_display.chars().skip(char_count - available).collect()
    } else {
        input_display
    };

    execute!(stdout, style::Print(format!("{}{}", state.prompt_label, visible_input)))?;
    execute!(stdout, cursor::Show)?;
    stdout.flush()?;
    Ok(())
}

fn redraw_chat(stdout: &mut io::Stdout, state: &CliState) -> Result<()> {
    let (width, height) = terminal::size()?;
    let w = width as usize;
    let h = height as u16;

    // ── Header (row 0) ──────────────────────────────────────────────
    execute!(stdout, cursor::MoveTo(0, 0), terminal::Clear(ClearType::CurrentLine))?;
    let room_str = state
        .current_room
        .as_deref()
        .unwrap_or("(no room)");
    let header = format!(
        " Room: {}  |  {} peer(s) online",
        room_str, state.peer_count
    );
    let header_truncated = truncate_str(&header, w);
    execute!(stdout, style::PrintStyledContent(header_truncated.clone().on(Color::DarkBlue).white()))?;

    // Pad remainder of header row
    let pad = w.saturating_sub(header_truncated.len());
    if pad > 0 {
        execute!(stdout, style::PrintStyledContent(" ".repeat(pad).on(Color::DarkBlue)))?;
    }

    // ── Separator (row 1) ────────────────────────────────────────────
    execute!(stdout, cursor::MoveTo(0, 1), terminal::Clear(ClearType::CurrentLine))?;
    execute!(stdout, style::Print("\u{2500}".repeat(w)))?;

    // ── Messages (rows 2 .. h-3) ─────────────────────────────────────
    let msg_area_height = (h.saturating_sub(4)) as usize;
    let msgs: Vec<&DisplayMessage> = state
        .messages
        .iter()
        .rev()
        .take(msg_area_height)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    for row in 0..msg_area_height {
        let screen_row = (row + 2) as u16;
        execute!(stdout, cursor::MoveTo(0, screen_row), terminal::Clear(ClearType::CurrentLine))?;
        if let Some(msg) = msgs.get(row) {
            let rendered = msg.render(w);
            if msg.is_system {
                execute!(stdout, style::PrintStyledContent(rendered.dark_grey()))?;
            } else {
                execute!(stdout, style::Print(rendered))?;
            }
        }
    }

    // ── Separator (row h-2) ──────────────────────────────────────────
    execute!(stdout, cursor::MoveTo(0, h - 2), terminal::Clear(ClearType::CurrentLine))?;
    execute!(stdout, style::Print("\u{2500}".repeat(w)))?;

    // ── Input bar (row h-1) ──────────────────────────────────────────
    execute!(stdout, cursor::MoveTo(0, h - 1), terminal::Clear(ClearType::CurrentLine))?;
    let input_display = format!("> {}", state.input_buffer);
    let input_truncated = truncate_str(&input_display, w);
    execute!(stdout, style::Print(&input_truncated))?;

    // Position cursor at end of input
    let cursor_x = input_truncated.len().min(w.saturating_sub(1)) as u16;
    execute!(stdout, cursor::MoveTo(cursor_x, h - 1), cursor::Show)?;

    stdout.flush()?;
    Ok(())
}

fn redraw_header(stdout: &mut io::Stdout, state: &CliState) -> Result<()> {
    let (width, _) = terminal::size()?;
    let w = width as usize;

    execute!(stdout, cursor::MoveTo(0, 0), terminal::Clear(ClearType::CurrentLine))?;
    let room_str = state.current_room.as_deref().unwrap_or("(no room)");
    let header = format!(
        " Room: {}  |  {} peer(s) online",
        room_str, state.peer_count
    );
    let header_truncated = truncate_str(&header, w);
    execute!(stdout, style::PrintStyledContent(header_truncated.clone().on(Color::DarkBlue).white()))?;

    let pad = w.saturating_sub(header_truncated.len());
    if pad > 0 {
        execute!(stdout, style::PrintStyledContent(" ".repeat(pad).on(Color::DarkBlue)))?;
    }

    stdout.flush()?;
    Ok(())
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}
