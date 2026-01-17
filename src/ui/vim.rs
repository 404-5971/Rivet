use std::time::Instant;
use tokio::sync::{MutexGuard, mpsc::Sender};

use crate::{App, AppAction, InputMode};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VimOperator {
    Delete,
    Change,
    Yank,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VimMotion {
    WordForward,
    WordBackward,
    Line,
    CharRight,
    CharLeft,
    StartOfLine,
    EndOfLine,
}

#[derive(Debug, Clone)]
pub struct VimState {
    pub operator: Option<VimOperator>,
    pub pending_keys: String,
    pub last_action_time: Instant,
}

impl Default for VimState {
    fn default() -> Self {
        Self {
            operator: None,
            pending_keys: String::new(),
            last_action_time: Instant::now(),
        }
    }
}

pub fn clamp_cursor(state: &mut MutexGuard<'_, App>) {
    let len = state.input.len();
    if len == 0 {
        state.cursor_position = 0;
    } else if state.cursor_position >= len {
        let last_char_len = state
            .input
            .chars()
            .last()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        state.cursor_position = len.saturating_sub(last_char_len);
    }
}

fn get_motion_range(state: &MutexGuard<'_, App>, motion: VimMotion) -> (usize, usize) {
    let start = state.cursor_position;
    let len = state.input.len();
    let input = &state.input;

    let end = match motion {
        VimMotion::WordForward => {
            let mut pos = start;
            if pos < len {
                // If on a space, skip spaces
                while pos < len {
                    let c = input[pos..].chars().next().unwrap();
                    if c.is_whitespace() {
                        pos += c.len_utf8();
                    } else {
                        break;
                    }
                }
                // Skip current word
                while pos < len {
                    let c = input[pos..].chars().next().unwrap();
                    if !c.is_whitespace() {
                        pos += c.len_utf8();
                    } else {
                        break;
                    }
                }
                // Skip spaces to next word
                while pos < len {
                    let c = input[pos..].chars().next().unwrap();
                    if c.is_whitespace() {
                        pos += c.len_utf8();
                    } else {
                        break;
                    }
                }
            }
            pos.min(len)
        }
        VimMotion::WordBackward => {
            let mut pos = start;
            if pos > 0 {
                let c = input[..pos].chars().next_back().unwrap();
                pos -= c.len_utf8();
                // Skip spaces backwards
                while pos > 0 {
                    let c = input[..pos].chars().next_back().unwrap();
                    if c.is_whitespace() {
                        pos -= c.len_utf8();
                    } else {
                        break;
                    }
                }
                // Skip word backwards
                while pos > 0 {
                    let c = input[..pos].chars().next_back().unwrap();
                    if !c.is_whitespace() {
                        pos -= c.len_utf8();
                    } else {
                        break;
                    }
                }
            }
            pos
        }
        VimMotion::Line => len, // Special case, usually handled by operator logic
        VimMotion::CharRight => {
            if start < len {
                start
                    + input[start..]
                        .chars()
                        .next()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0)
            } else {
                len
            }
        }
        VimMotion::CharLeft => {
            if start > 0 {
                start
                    - input[..start]
                        .chars()
                        .next_back()
                        .map(|c| c.len_utf8())
                        .unwrap_or(0)
            } else {
                0
            }
        }
        VimMotion::StartOfLine => 0,
        VimMotion::EndOfLine => len,
    };

    (start, end)
}

fn execute_operator(state: &mut MutexGuard<'_, App>, operator: VimOperator, range: (usize, usize)) {
    let (start, end) = range;
    let (low, high) = if start < end {
        (start, end)
    } else {
        (end, start)
    };

    match operator {
        VimOperator::Delete => {
            if high > low && state.input.is_char_boundary(low) && state.input.is_char_boundary(high)
            {
                state.input.drain(low..high);
                state.cursor_position = low;
            }
        }
        VimOperator::Change => {
            // Not implemented yet
        }
        VimOperator::Yank => {
            // Not implemented yet
        }
    }
}

pub async fn handle_vim_keys(
    mut state: MutexGuard<'_, App>,
    c: char,
    tx_action: Sender<AppAction>,
) {
    // Check for timeout
    if let Some(vim_state) = &mut state.vim_state {
        if vim_state.operator.is_some() {
            if Instant::now()
                .duration_since(vim_state.last_action_time)
                .as_secs()
                >= 1
            {
                vim_state.operator = None;
                vim_state.pending_keys.clear();
            }
        }
    }

    // Ensure vim_state exists (it should, but for safety)
    if state.vim_state.is_none() {
        state.vim_state = Some(VimState::default());
    }

    // We need to clone some state to avoid borrow checker issues when calling async functions
    // or when mutating state later.
    let current_operator = state.vim_state.as_ref().unwrap().operator;

    match c {
        'i' => {
            state.mode = InputMode::Insert;
        }
        'I' => {
            state.cursor_position = 0;
            state.mode = InputMode::Insert;
        }
        'a' => {
            if state.cursor_position < state.input.len() {
                let c = state.input[state.cursor_position..].chars().next().unwrap();
                state.cursor_position += c.len_utf8();
            }
            state.mode = InputMode::Insert;
        }
        'A' => {
            state.cursor_position = state.input.len();
            state.mode = InputMode::Insert;
        }
        'j' => {
            tx_action.send(AppAction::SelectNext).await.ok();
        }
        'k' => {
            tx_action.send(AppAction::SelectPrevious).await.ok();
        }
        'h' => {
            if state.cursor_position > 0 {
                let c = state.input[..state.cursor_position]
                    .chars()
                    .next_back()
                    .unwrap();
                state.cursor_position -= c.len_utf8();
            }
        }
        'l' => {
            if state.cursor_position < state.input.len() {
                let c = state.input[state.cursor_position..].chars().next().unwrap();
                let next_pos = state.cursor_position + c.len_utf8();
                if next_pos < state.input.len() {
                    state.cursor_position = next_pos;
                }
            }
        }
        'w' => {
            if let Some(op) = current_operator {
                let range = get_motion_range(&state, VimMotion::WordForward);
                execute_operator(&mut state, op, range);
                if let Some(vim_state) = &mut state.vim_state {
                    vim_state.operator = None;
                }
            } else {
                let (_, end) = get_motion_range(&state, VimMotion::WordForward);
                state.cursor_position = end;
                clamp_cursor(&mut state);
            }
        }
        'b' => {
            if let Some(op) = current_operator {
                let range = get_motion_range(&state, VimMotion::WordBackward);
                execute_operator(&mut state, op, range);
                if let Some(vim_state) = &mut state.vim_state {
                    vim_state.operator = None;
                }
            } else {
                let (_, end) = get_motion_range(&state, VimMotion::WordBackward);
                state.cursor_position = end;
            }
        }
        'd' => {
            if let Some(VimOperator::Delete) = current_operator {
                // dd case
                state.input.clear();
                state.cursor_position = 0;
                if let Some(vim_state) = &mut state.vim_state {
                    vim_state.operator = None;
                }
            } else {
                if let Some(vim_state) = &mut state.vim_state {
                    vim_state.operator = Some(VimOperator::Delete);
                    vim_state.last_action_time = Instant::now();
                }
            }
        }
        'x' => {
            let pos = state.cursor_position;
            if pos < state.input.len() && state.input.is_char_boundary(pos) {
                if let Some(ch) = state.input[pos..].chars().next() {
                    let char_end = pos + ch.len_utf8();
                    state.input.drain(pos..char_end);
                    clamp_cursor(&mut state);
                }
            }
        }
        ':' => {
            tx_action.send(AppAction::SelectEmoji).await.ok();
        }
        _ => {
            if let Some(vim_state) = &mut state.vim_state {
                vim_state.operator = None;
                vim_state.pending_keys.clear();
            }
        }
    }
}
