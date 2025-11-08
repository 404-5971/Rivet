use std::{env, io, process, sync::Arc};

use crossterm::{
    event::{self, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    prelude::CrosstermBackend,
    style::{Style, Stylize},
    widgets::{List, ListItem, ListState},
};
use reqwest::Client;
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinHandle,
    time::{self, Duration},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    api::{
        guild::get_guild_channels::get_guild_channels,
        message::{create_message::create_message, get_channel_messages::get_channel_messages},
        user::get_current_user_guilds::get_current_user_guilds,
    },
    model::{
        channel::{Channel, Message},
        guild::Guild,
    },
    signals::{restore_terminal, setup_ctrlc_handler},
};

pub mod api;
pub mod model;
mod signals;

pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug)]
enum AppState {
    SelectingGuild,
    SelectingChannel(String),
    Chatting(String),
}

#[derive(Debug)]
enum AppAction {
    Quit,
    InputChar(char),
    InputBackspace,
    InputEscape,
    InputSubmit,
    SelectNext,
    SelectPrevious,
    ApiUpdateMessages(Vec<Message>),
    ApiUpdateChannel(Vec<Channel>),
    TransitionToChat(String),
    TransitionToChannels(String),
    TransitionToGuilds,
}

struct App {
    state: AppState,
    guilds: Vec<Guild>,
    channels: Vec<Channel>,
    messages: Vec<Message>,
    input: String,
    selection_index: usize,
    status_message: String,
    terminal_height: usize,
    terminal_width: usize,
}

async fn run_app(token: String) -> Result<(), Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let client = Client::new();

    let app_state = Arc::new(Mutex::new(App {
        state: AppState::SelectingGuild,
        guilds: Vec::new(),
        channels: Vec::new(),
        messages: Vec::new(),
        input: String::new(),
        selection_index: 0,
        status_message: "Loading servers...".to_string(),
        terminal_height: 20,
        terminal_width: 80,
    }));

    let (tx_action, mut rx_action) = mpsc::channel::<AppAction>(32);
    let (tx_shutdown, _) = tokio::sync::broadcast::channel::<()>(1);

    let tx_input = tx_action.clone();
    let rx_shutdown_input = tx_shutdown.subscribe();

    let input_handle: JoinHandle<Result<(), io::Error>> = tokio::spawn(async move {
        let res = handle_input_events(tx_input, rx_shutdown_input).await;
        if let Err(e) = &res {
            eprintln!("Input Error: {e}");
        }
        res
    });

    let api_state = Arc::clone(&app_state);
    let api_client = client.clone();
    let api_token = token.clone();
    let tx_api = tx_action.clone();
    let mut rx_shutdown_api = tx_shutdown.subscribe();

    let mut interval = time::interval(Duration::from_secs(1));

    let api_handle: JoinHandle<()> = tokio::spawn(async move {
        match get_current_user_guilds(&api_client, &api_token).await {
            Ok(guilds) => {
                let mut state = api_state.lock().await;
                state.guilds = guilds;
                state.status_message =
                    "Select a server. Use arrows to navigate, Enter to select & Esc to quit."
                        .to_string();
            }
            Err(e) => {
                api_state.lock().await.status_message = format!("Failed to load servers. {e}");
            }
        }

        loop {
            tokio::select! {
                _ = rx_shutdown_api.recv() => {
                    return;
                }

                _ = interval.tick() => {
                    let current_channel_id = {
                        let state = api_state.lock().await;
                        match &state.state {
                            AppState::Chatting(id) => Some(id.clone()),
                            _ => None,
                        }
                    };

                    if let Some(channel_id) = current_channel_id {
                        const MESSAGE_LIMIT: usize = 100;

                        match get_channel_messages(
                            &api_client,
                            &channel_id,
                            &api_token,
                            None,
                            None,
                            None,
                            Some(MESSAGE_LIMIT),
                        )
                        .await
                        {
                            Ok(messages) => {
                                if let Err(e) = tx_api.send(AppAction::ApiUpdateMessages(messages)).await {
                                    eprintln!("Failed to send message update action: {e}");
                                    return;
                                }
                            }
                            Err(e) => {
                                api_state.lock().await.status_message = format!("Error loading chat: {e}");
                            }
                        }
                    }
                }
            }
        }
    });

    fn draw_ui(f: &mut ratatui::Frame, app: &mut App) {
        use ratatui::layout::{Constraint, Direction, Layout};
        use ratatui::text::{Line, Text};
        use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

        let area = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(90), Constraint::Percentage(10)].as_ref())
            .split(area);

        app.terminal_height = chunks[0].height as usize;
        app.terminal_width = chunks[0].width as usize;

        let max_height = app.terminal_height.saturating_sub(2);
        let max_width = app.terminal_width.saturating_sub(2) as u16;

        match &app.state {
            AppState::SelectingGuild => {
                let items: Vec<ListItem> = app
                    .guilds
                    .iter()
                    .map(|g| ListItem::new(g.name.as_str()))
                    .collect();

                let list = List::new(items)
                    .block(
                        Block::default()
                            .title("Servers (Guilds)")
                            .borders(Borders::ALL),
                    )
                    .highlight_style(Style::default().reversed())
                    .highlight_symbol(">> ");

                let mut state = ListState::default().with_selected(Some(app.selection_index));
                f.render_stateful_widget(list, chunks[0], &mut state);
            }
            AppState::SelectingChannel(guild_id) => {
                let title = format!("Channels for Guild: {guild_id}");
                let items: Vec<ListItem> = app
                    .channels
                    .iter()
                    .filter(|c| c.channel_type != 4)
                    .map(|c| ListItem::new(format!("# {}", c.name)))
                    .collect();

                let list = List::new(items)
                    .block(Block::default().title(title).borders(Borders::ALL))
                    .highlight_style(Style::default().reversed())
                    .highlight_symbol(">> ");

                let mut state = ListState::default().with_selected(Some(app.selection_index));
                f.render_stateful_widget(list, chunks[0], &mut state);
            }
            AppState::Chatting(_) => {
                if max_width == 0 {
                    return;
                }

                let mut messages_to_render: Vec<Message> = Vec::new();
                let mut current_height = 0;

                for message in app.messages.iter() {
                    let formatted_text = format!(
                        "[{}] {}: {}",
                        message
                            .timestamp
                            .split('T')
                            .nth(1)
                            .unwrap_or("")
                            .split('.')
                            .next()
                            .unwrap_or(""),
                        message.author.username,
                        message.content.as_deref().unwrap_or("(*non-text*)")
                    );

                    let text_lines: Vec<&str> = formatted_text.split('\n').collect();
                    let mut estimated_height = 0;

                    for line in text_lines {
                        let width = UnicodeWidthStr::width(line) as u16;

                        if width == 0 {
                            estimated_height += 1;
                            continue;
                        }

                        let wrap_lines =
                            (width as usize + max_width as usize - 1) / (max_width as usize);

                        estimated_height += wrap_lines;
                    }

                    if current_height + estimated_height > max_height {
                        break;
                    }

                    current_height += estimated_height;

                    messages_to_render.push(message.clone());
                }

                messages_to_render.reverse();

                let mut final_content: Vec<Line> = Vec::new();

                for message in messages_to_render.into_iter() {
                    let formatted_text = format!(
                        "[{}] {}: {}",
                        message
                            .timestamp
                            .split('T')
                            .nth(1)
                            .unwrap_or("")
                            .split('.')
                            .next()
                            .unwrap_or(""),
                        message.author.username,
                        message.content.as_deref().unwrap_or("(*non-text*)")
                    );
                    let text = Text::raw(formatted_text);

                    final_content.extend(text.lines);
                }

                let scroll_offset = if final_content.len() > max_height {
                    final_content.len().saturating_sub(max_height)
                } else {
                    0
                };

                let paragraph = Paragraph::new(final_content)
                    .block(
                        Block::default()
                            .title("Rivet Client (Esc to return to Servers")
                            .borders(Borders::ALL),
                    )
                    .wrap(Wrap { trim: false })
                    .scroll((scroll_offset as u16, 0));

                f.render_widget(paragraph, chunks[0]);
            }
        };

        f.render_widget(
            Paragraph::new(app.input.as_str()).block(
                Block::default()
                    .title(format!("Input: {}", app.status_message))
                    .borders(Borders::ALL),
            ),
            chunks[1],
        )
    }

    async fn handle_input_events(
        tx: mpsc::Sender<AppAction>,
        mut rx_shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), io::Error> {
        loop {
            tokio::select! {
                _ = rx_shutdown.recv() => {
                    return Ok(());
                }

                _ = time::sleep(Duration::from_millis(50)) => {
                    if event::poll(Duration::from_millis(0))? {
                        if let event::Event::Key(key) = event::read()? {
                            if key.kind == KeyEventKind::Press {
                                match key.code {
                                    KeyCode::Esc => {
                                        tx.send(AppAction::InputEscape).await.ok();
                                    }
                                    KeyCode::Enter => {
                                        tx.send(AppAction::InputSubmit).await.ok();
                                    }
                                    KeyCode::Backspace => {
                                        tx.send(AppAction::InputBackspace).await.ok();
                                    }
                                    KeyCode::Up => {
                                        tx.send(AppAction::SelectPrevious).await.ok();
                                    }
                                    KeyCode::Down => {
                                        tx.send(AppAction::SelectNext).await.ok();
                                    }
                                    KeyCode::Char(c) => {
                                        tx.send(AppAction::InputChar(c)).await.ok();
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    loop {
        {
            let mut state_guard = app_state.lock().await;
            terminal
                .draw(|f| {
                    draw_ui(f, &mut state_guard);
                })
                .unwrap();
        }
        if let Some(action) = rx_action.recv().await {
            let mut state = app_state.lock().await;

            match action {
                AppAction::Quit => {
                    break;
                }
                AppAction::InputEscape => match state.state {
                    AppState::SelectingGuild => {
                        break;
                    }
                    AppState::SelectingChannel(_) => {
                        state.state = AppState::SelectingGuild;
                        state.status_message = "Select a server. Use arrows to navigate, Enter to select & Esc to quit".to_string();
                        state.selection_index = 0;
                    }
                    AppState::Chatting(_) => {
                        state.state = AppState::SelectingGuild;
                        state.status_message = "Select a server. Use arrows to navigate, Enter to select & Esc to quit".to_string();
                        state.selection_index = 0;
                    }
                },
                AppAction::InputChar(c) => {
                    if let AppState::Chatting(_) = state.state {
                        state.input.push(c);
                    }
                }
                AppAction::InputBackspace => {
                    state.input.pop();
                }
                AppAction::InputSubmit => match &state.state {
                    AppState::SelectingGuild => {
                        if state.guilds.is_empty() {
                            continue;
                        }

                        let selected_guild = &state.guilds[state.selection_index];
                        let guild_id_clone = selected_guild.id.clone();
                        let selected_guild_name = selected_guild.name.clone();

                        let client_clone = client.clone();
                        let token_clone = token.clone();
                        let tx_clone = tx_action.clone();

                        state.status_message =
                            format!("Loading channels for {selected_guild_name}...");

                        tokio::spawn(async move {
                            match get_guild_channels(&client_clone, &token_clone, &guild_id_clone)
                                .await
                            {
                                Ok(channels) => {
                                    tx_clone
                                        .send(AppAction::ApiUpdateChannel(channels))
                                        .await
                                        .ok();
                                    tx_clone
                                        .send(AppAction::TransitionToChannels(guild_id_clone))
                                        .await
                                        .ok();
                                }
                                Err(e) => {
                                    eprintln!("Failed to load channels: {e}");
                                }
                            }
                        });
                    }
                    AppState::SelectingChannel(_) => {
                        let text_channels: Vec<&Channel> = state
                            .channels
                            .iter()
                            .filter(|c| c.channel_type != 4)
                            .collect();

                        if text_channels.is_empty() {
                            continue;
                        }

                        let channel_info = {
                            let selected_channel = &text_channels[state.selection_index];
                            (selected_channel.id.clone(), selected_channel.name.clone())
                        };
                        let (channel_id_clone, selected_channel_name) = channel_info;

                        state.state = AppState::Chatting(channel_id_clone.clone());
                        state.status_message =
                            format!("Chatting in channel #{selected_channel_name}");
                        state.selection_index = 0;
                    }
                    AppState::Chatting(_) => {
                        let channel_id_clone = if let AppState::Chatting(id) = &state.state {
                            Some(id.clone())
                        } else {
                            None
                        };

                        let content = state.input.drain(..).collect::<String>();

                        let message_data = if content.is_empty() || channel_id_clone.is_none() {
                            None
                        } else {
                            Some((channel_id_clone.unwrap(), content))
                        };

                        if let Some((channel_id_clone, content)) = message_data {
                            let client_clone = client.clone();
                            let token_clone = token.clone();

                            tokio::spawn(async move {
                                match create_message(
                                    &client_clone,
                                    &channel_id_clone,
                                    &token_clone,
                                    Some(content),
                                    false,
                                )
                                .await
                                {
                                    Ok(_) => {}
                                    Err(e) => {
                                        eprintln!("API Error: {e}");
                                    }
                                }
                            });
                        }
                    }
                },
                AppAction::SelectNext => match state.state {
                    AppState::SelectingGuild => {
                        if !state.guilds.is_empty() {
                            state.selection_index =
                                (state.selection_index + 1) % state.guilds.len();
                        }
                    }
                    AppState::SelectingChannel(_) => {
                        if !state.channels.is_empty() {
                            state.selection_index = (state.selection_index + 1)
                                % state
                                    .channels
                                    .iter()
                                    .filter(|c| c.channel_type != 4)
                                    .count();
                        }
                    }
                    _ => {}
                },
                AppAction::SelectPrevious => match state.state {
                    AppState::SelectingGuild => {
                        if !state.guilds.is_empty() {
                            state.selection_index = if state.selection_index == 0 {
                                state.guilds.len() - 1
                            } else {
                                state.selection_index - 1
                            };
                        }
                    }
                    AppState::SelectingChannel(_) => {
                        if !state.channels.is_empty() {
                            state.selection_index = if state.selection_index == 0 {
                                state
                                    .channels
                                    .iter()
                                    .filter(|c| c.channel_type != 4)
                                    .count()
                                    - 1
                            } else {
                                state.selection_index - 1
                            };
                        }
                    }
                    _ => {}
                },
                AppAction::ApiUpdateMessages(new_messages) => {
                    state.messages = new_messages;
                }
                AppAction::ApiUpdateChannel(new_channels) => {
                    state.channels = new_channels;
                    let text_channels_count = state.channels.len();
                    if text_channels_count > 0 {
                        state.status_message =
                            "Channels loaded. Select one to chat. (Esc to return to Servers)"
                                .to_string();
                    } else {
                        state.status_message =
                            "No text channels found. (Esc to return to Servers)".to_string();
                    }
                    state.selection_index = 0;
                }
                AppAction::TransitionToChannels(guild_id) => {
                    state.state = AppState::SelectingChannel(guild_id);
                    state.selection_index = 0;
                }
                AppAction::TransitionToChat(channel_id) => {
                    state.state = AppState::Chatting(channel_id);
                    state.status_message = "Chatting...".to_string();
                }
                AppAction::TransitionToGuilds => {
                    state.state = AppState::SelectingGuild;
                    state.selection_index = 0;
                }
            }
        }
    }

    drop(rx_action);

    let _ = tx_shutdown.send(());

    let _ = tokio::join!(input_handle, api_handle);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenvy::dotenv().ok();
    const ENV_TOKEN: &str = "DISCORD_TOKEN";

    let token: String = env::var(ENV_TOKEN).unwrap_or_else(|_| {
        eprintln!("Error: DISCORD_TOKEN variable is missing.");
        process::exit(1);
    });

    setup_ctrlc_handler();

    let app_result = run_app(token).await;

    restore_terminal();

    app_result
}
