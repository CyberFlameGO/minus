//! Provides the [`handle_event`] function

use std::io::Write;
use std::sync::{atomic::AtomicBool, Arc};

#[cfg(feature = "search")]
use parking_lot::{Condvar, Mutex};

#[cfg(feature = "search")]
use super::search;
use super::utils::display;
use super::utils::text::AppendStyle;
use super::{events::Event, utils::term};
use crate::{error::MinusError, input::InputEvent, PagerState};

/// Respond based on the type of event
///
/// It will match the type of event received and based on that, it can take actions like:-
/// - Mutating fields of [`PagerState`]
/// - Handle cleanup and exits
/// - Call search related functions
#[cfg_attr(not(feature = "search"), allow(unused_mut))]
#[cfg_attr(not(feature = "search"), allow(clippy::unnecessary_wraps))]
#[allow(clippy::too_many_lines)]
pub fn handle_event(
    ev: Event,
    mut out: &mut impl Write,
    p: &mut PagerState,
    is_exitted: &Arc<AtomicBool>,
    #[cfg(feature = "search")] user_input_active: &Arc<(Mutex<bool>, Condvar)>,
) -> Result<(), MinusError> {
    match ev {
        Event::SetData(text) => {
            p.lines = text;
            p.format_lines();
        }
        Event::UserInput(InputEvent::Exit) => {
            p.exit();
            is_exitted.store(true, std::sync::atomic::Ordering::SeqCst);
            term::cleanup(&mut out, &p.exit_strategy, true)?;
        }
        Event::UserInput(InputEvent::UpdateUpperMark(mut um)) => {
            display::draw_for_change(out, p, &mut um)?;
            p.upper_mark = um;
        }
        Event::UserInput(InputEvent::RestorePrompt) => {
            // Set the message to None and new messages to false as all messages have been shown
            p.message = None;
            p.format_prompt();
        }
        Event::UserInput(InputEvent::UpdateTermArea(c, r)) => {
            p.rows = r;
            p.cols = c;
            // Readjust the text wrapping for the new number of columns
            p.format_lines();
        }
        Event::UserInput(InputEvent::UpdateLineNumber(l)) => {
            p.line_numbers = l;
            p.format_lines();
        }
        #[cfg(feature = "search")]
        Event::UserInput(InputEvent::Search(m)) => {
            p.search_mode = m;
            // Pause the main user input thread, read search query and then restart the main input thread
            let (lock, cvar) = (&user_input_active.0, &user_input_active.1);
            let mut active = lock.lock();
            *active = false;
            drop(active);
            let string = search::fetch_input(&mut out, p.search_mode, p.rows)?;
            let mut active = lock.lock();
            *active = true;
            drop(active);
            cvar.notify_one();

            if !string.is_empty() {
                let regex = regex::Regex::new(string.as_str());
                if let Ok(r) = regex {
                    p.search_term = Some(r);
                    // Format the lines, this will automatically generate the PagerState.search_idx
                    p.format_lines();
                    // Reset search mark so it won't be out of bounds if we have
                    // less matches in this search than last time
                    p.search_mark = 0;
                    // Move to next search match after the current upper_mark
                    search::next_nth_match(p, 1);
                    p.format_prompt();
                    display::draw_full(&mut out, p)?;
                } else {
                    // Send invalid regex message at the prompt if invalid regex is given
                    p.message = Some("Invalid regular expression. Press Enter".to_owned());
                    p.format_prompt();
                }
            }
        }
        #[cfg(feature = "search")]
        Event::UserInput(InputEvent::NextMatch | InputEvent::MoveToNextMatch(1))
            if p.search_term.is_some() =>
        {
            // Go to the next match
            search::next_nth_match(p, 1);
        }
        #[cfg(feature = "search")]
        Event::UserInput(InputEvent::PrevMatch | InputEvent::MoveToPrevMatch(1))
            if p.search_term.is_some() =>
        {
            // If no matches, return immediately
            if p.search_idx.is_empty() {
                return Ok(());
            }
            // Decrement the s_mark and get the preceeding index
            p.search_mark = p.search_mark.saturating_sub(1);
            if let Some(y) = p.search_idx.iter().nth(p.search_mark) {
                // If the index is less than or equal to the upper_mark, then set y to the new upper_mark
                if *y < p.upper_mark {
                    p.upper_mark = *y;
                    p.format_prompt();
                }
            }
        }
        #[cfg(feature = "search")]
        Event::UserInput(InputEvent::MoveToNextMatch(n)) if p.search_term.is_some() => {
            // Go to the next match
            search::next_nth_match(p, n.saturating_sub(1));
        }
        #[cfg(feature = "search")]
        Event::UserInput(InputEvent::MoveToPrevMatch(n)) if p.search_term.is_some() => {
            // If no matches, return immediately
            if p.search_idx.is_empty() {
                return Ok(());
            }
            // Decrement the s_mark and get the preceeding index
            p.search_mark = p.search_mark.saturating_sub(n);
            if let Some(y) = p.search_idx.iter().nth(p.search_mark) {
                // If the index is less than or equal to the upper_mark, then set y to the new upper_mark
                if *y < p.upper_mark {
                    p.upper_mark = *y;
                    p.format_prompt();
                }
            }
        }

        Event::AppendData(text) => {
            let append_style = p.append_str(text.as_str());

            if let AppendStyle::FullRedraw = append_style {
                p.format_lines();
            }
            if let AppendStyle::PartialUpdate((fmt_line, num_unterminated)) = append_style {
                p.append_str_on_unterminated(fmt_line, num_unterminated);
            }
        }
        Event::SetPrompt(prompt) => {
            p.prompt = prompt;
            p.format_prompt();
        }
        Event::SendMessage(message) => {
            p.message = Some(message);
            p.format_prompt();
        }
        Event::SetLineNumbers(ln) => {
            p.line_numbers = ln;
            p.format_lines();
        }
        Event::SetExitStrategy(es) => p.exit_strategy = es,
        #[cfg(feature = "static_output")]
        Event::SetRunNoOverflow(val) => p.run_no_overflow = val,
        Event::SetInputClassifier(clf) => p.input_classifier = clf,
        Event::AddExitCallback(cb) => p.exit_callbacks.push(cb),
        Event::UserInput(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::events::Event;
    use super::handle_event;
    use crate::{ExitStrategy, PagerState};
    use std::sync::{atomic::AtomicBool, Arc};
    #[cfg(feature = "search")]
    use {
        once_cell::sync::Lazy,
        parking_lot::{Condvar, Mutex},
    };

    // Tests constants
    #[cfg(feature = "search")]
    static UIA: Lazy<Arc<(Mutex<bool>, Condvar)>> =
        Lazy::new(|| Arc::new((Mutex::new(true), Condvar::new())));
    const TEST_STR: &str = "This is some sample text";

    // Tests for event emitting functions of Pager
    #[test]
    fn set_data() {
        let mut ps = PagerState::new().unwrap();
        let ev = Event::SetData(TEST_STR.to_string());
        let mut out = Vec::new();

        handle_event(
            ev,
            &mut out,
            &mut ps,
            &Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "search")]
            &UIA,
        )
        .unwrap();
        assert_eq!(ps.formatted_lines, vec![TEST_STR.to_string()]);
    }

    #[test]
    fn append_str() {
        let mut ps = PagerState::new().unwrap();
        let ev1 = Event::AppendData(format!("{TEST_STR}\n"));
        let ev2 = Event::AppendData(TEST_STR.to_string());
        let mut out = Vec::new();

        handle_event(
            ev1,
            &mut out,
            &mut ps,
            &Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "search")]
            &UIA,
        )
        .unwrap();
        handle_event(
            ev2,
            &mut out,
            &mut ps,
            &Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "search")]
            &UIA,
        )
        .unwrap();
        assert_eq!(
            ps.formatted_lines,
            vec![TEST_STR.to_string(), TEST_STR.to_string()]
        );
    }

    #[test]
    fn set_prompt() {
        let mut ps = PagerState::new().unwrap();
        let ev = Event::SetPrompt(TEST_STR.to_string());
        let mut out = Vec::new();

        handle_event(
            ev,
            &mut out,
            &mut ps,
            &Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "search")]
            &UIA,
        )
        .unwrap();
        assert_eq!(ps.prompt, TEST_STR.to_string());
    }

    #[test]
    fn send_message() {
        let mut ps = PagerState::new().unwrap();
        let ev = Event::SendMessage(TEST_STR.to_string());
        let mut out = Vec::new();

        handle_event(
            ev,
            &mut out,
            &mut ps,
            &Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "search")]
            &UIA,
        )
        .unwrap();
        assert_eq!(ps.message.unwrap(), TEST_STR.to_string());
    }

    #[test]
    #[cfg(feature = "static_output")]
    fn set_run_no_overflow() {
        let mut ps = PagerState::new().unwrap();
        let ev = Event::SetRunNoOverflow(false);
        let mut out = Vec::new();

        handle_event(
            ev,
            &mut out,
            &mut ps,
            &Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "search")]
            &UIA,
        )
        .unwrap();
        assert!(!ps.run_no_overflow);
    }

    #[test]
    fn set_exit_strategy() {
        let mut ps = PagerState::new().unwrap();
        let ev = Event::SetExitStrategy(ExitStrategy::PagerQuit);
        let mut out = Vec::new();

        handle_event(
            ev,
            &mut out,
            &mut ps,
            &Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "search")]
            &UIA,
        )
        .unwrap();
        assert_eq!(ps.exit_strategy, ExitStrategy::PagerQuit);
    }

    #[test]
    fn add_exit_callback() {
        let mut ps = PagerState::new().unwrap();
        let ev = Event::AddExitCallback(Box::new(|| println!("Hello World")));
        let mut out = Vec::new();

        handle_event(
            ev,
            &mut out,
            &mut ps,
            &Arc::new(AtomicBool::new(false)),
            #[cfg(feature = "search")]
            &UIA,
        )
        .unwrap();
        assert_eq!(ps.exit_callbacks.len(), 1);
    }
}
