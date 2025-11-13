use std::io;

use crossterm::event::{self, KeyCode, KeyEventKind};
use reqwest::Client;
use tokio::{
    sync::{MutexGuard, mpsc::Sender},
    time::{self, Duration},
};

use crate::{
    App, AppAction, AppState, KeywordAction, api::channel::get_channel::get_channel, input_submit,
    move_selection,
};

pub async fn handle_input_events(
    tx: Sender<AppAction>,
    mut rx_shutdown: tokio::sync::broadcast::Receiver<()>,
) -> Result<(), io::Error> {
    loop {
        tokio::select! {
            _ = rx_shutdown.recv() => {
                return Ok(());
            }

            _ = time::sleep(Duration::from_millis(50)) => {
                if event::poll(Duration::from_millis(0))?
                    && let event::Event::Key(key) = event::read()?
                        && key.kind == KeyEventKind::Press {
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

pub async fn handle_keys_events(
    mut state: MutexGuard<'_, App>,
    action: AppAction,
    client: &Client,
    token: String,
    tx_action: Sender<AppAction>,
) -> Option<KeywordAction> {
    match action {
        AppAction::InputEscape => match &state.state {
            AppState::SelectingGuild => {
                return Some(KeywordAction::Break);
            }
            AppState::SelectingChannel(_) => {
                state.state = AppState::SelectingGuild;
                state.status_message =
                    "Select a server. Use arrows to navigate, Enter to select & Esc to quit"
                        .to_string();
                state.selection_index = 0;
            }
            AppState::Chatting(channel_id) => {
                let channel = get_channel(client, &token, channel_id).await.unwrap();
                match channel.guild_id {
                    Some(guild_id) => {
                        state.state = AppState::SelectingChannel(guild_id);
                        state.status_message = "Select a server. Use arrows to navigate, Enter to select & Esc to quit".to_string();
                        state.selection_index = 0;
                    }
                    None => {
                        state.state = AppState::SelectingGuild;
                        state.status_message = "Select a server. Use arrows to navigate, Enter to select & Esc to quit".to_string();
                        state.selection_index = 0;
                    }
                }
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
        AppAction::InputSubmit => {
            if input_submit(&mut state, client, token.clone(), &tx_action).await {
                return Some(KeywordAction::Continue);
            }
        }
        AppAction::SelectNext => move_selection(&mut state, 1).await,
        AppAction::SelectPrevious => move_selection(&mut state, -1).await,
        AppAction::ApiUpdateMessages(new_messages) => {
            state.messages = new_messages;
        }
        AppAction::ApiUpdateChannel(new_channels) => {
            state.channels = new_channels;
            let text_channels_count = state.channels.len();
            if text_channels_count > 0 {
                state.status_message =
                    "Channels loaded. Select one to chat. (Esc to return to Servers)".to_string();
            } else {
                state.status_message =
                    "No text channels found. (Esc to return to Servers)".to_string();
            }
            state.selection_index = 0;
        }
        AppAction::TransitionToChannels(guild_id) => {
            state.state = AppState::SelectingChannel(guild_id);
            state.status_message =
                "Select a channel. Use arrows to navigate, Enter to select & Esc to quit"
                    .to_string();
            state.selection_index = 0;
        }
        AppAction::TransitionToChat(channel_id) => {
            state.state = AppState::Chatting(channel_id);
            state.status_message = "Chatting...".to_string();
        }
        AppAction::TransitionToGuilds => {
            state.state = AppState::SelectingGuild;
            state.status_message =
                "Select a server. Use arrows to navigate, Enter to select & Esc to quit"
                    .to_string();
            state.selection_index = 0;
        }
    }
    None
}
