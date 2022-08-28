//! Provides the [`InputClassifier`] trait, which can be used
//! to customize the default keybindings of minus

pub(crate) mod keyevent;

#[cfg(feature = "search")]
use crate::minus_core::search::SearchMode;
use crate::{LineNumbers, PagerState};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use std::{
    collections::hash_map::RandomState, collections::HashMap, hash::BuildHasher, hash::Hash,
    sync::Arc,
};

/// Events handled by the `minus` pager.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(clippy::module_name_repetitions)]
pub enum InputEvent {
    /// `Ctrl+C` or `Q`, exits the application.
    Exit,
    /// The terminal was resized. Contains the new number of rows.
    UpdateTermArea(usize, usize),
    /// Sent by movement keys like `Up` `Down`, `PageUp`, 'PageDown', 'g', `G` etc. Contains the new value for the upper mark.
    UpdateUpperMark(usize),
    /// `Ctrl+L`, inverts the line number display. Contains the new value.
    UpdateLineNumber(LineNumbers),
    /// A number key has been pressed. This inner value is stored as a `char`.
    /// The input loop will append this number to its `count` string variable
    Number(char),
    /// Restore the original prompt
    RestorePrompt,
    Ignore,
    /// `/`, Searching for certain pattern of text
    #[cfg(feature = "search")]
    Search(SearchMode),
    /// Get to the next match in forward mode
    #[cfg(feature = "search")]
    NextMatch,
    /// Get to the previous match in forward mode
    #[cfg(feature = "search")]
    PrevMatch,
    /// Move to the next nth match in the given direction
    #[cfg(feature = "search")]
    MoveToNextMatch(usize),
    /// Move to the previous nth match in the given direction
    #[cfg(feature = "search")]
    MoveToPrevMatch(usize),
}

/// Define custom keybindings
///
/// This trait can help define custom keybindings in case
/// the downsteam applications aren't satisfied with the
/// defaults
///
/// **Please do note that, in order to match the keybindings,
/// you need to directly work with the underlying [`crossterm`]
/// crate**
///
/// # Example
/// ```
/// use minus::{input::{InputEvent, InputClassifier}, LineNumbers, Pager, PagerState};
#[cfg_attr(feature = "search", doc = "use minus::SearchMode;")]
/// use crossterm::event::{Event, KeyEvent, KeyCode, KeyModifiers};
///
/// struct CustomInputClassifier;
/// impl InputClassifier for CustomInputClassifier {
///     fn classify_input(
///         &self,
///         ev: Event,
///         ps: &PagerState
///     ) -> Option<InputEvent> {
///             match ev {
///                 Event::Key(KeyEvent {
///                     code: KeyCode::Up,
///                     modifiers: KeyModifiers::NONE,
///                 })
///                 | Event::Key(KeyEvent {
///                     code: KeyCode::Char('j'),
///                     modifiers: KeyModifiers::NONE,
///                 }) => Some(InputEvent::UpdateUpperMark
///                       (ps.upper_mark.saturating_sub(1))),
///                 _ => None
///         }
///     }
/// }
///
/// let mut pager = Pager::new();
/// pager.set_input_classifier(
///                 Box::new(CustomInputClassifier)
///             );
/// ```
#[allow(clippy::module_name_repetitions)]
pub trait InputClassifier {
    fn classify_input(&self, ev: Event, ps: &PagerState) -> Option<InputEvent>;
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum EventWrapper {
    ExactMatchEvent(Event),
    WildEvent,
}

impl From<Event> for EventWrapper {
    fn from(e: Event) -> Self {
        EventWrapper::ExactMatchEvent(e)
    }
}

impl From<&Event> for EventWrapper {
    fn from(e: &Event) -> Self {
        EventWrapper::ExactMatchEvent(*e)
    }
}

impl Hash for EventWrapper {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let tag = std::mem::discriminant(self);
        tag.hash(state);
        match self {
            Self::ExactMatchEvent(Event::Mouse(MouseEvent {
                kind, modifiers, ..
            })) => {
                kind.hash(state);
                modifiers.hash(state);
            }
            Self::ExactMatchEvent(v) => {
                v.hash(state);
            }
            _ => {}
        }
    }
}

pub struct HashedEventRegister<S>(
    HashMap<EventWrapper, Arc<dyn Fn(Event, &PagerState) -> InputEvent + Send + Sync>, S>,
);

impl<S> HashedEventRegister<S>
where
    S: BuildHasher,
{
    fn new(s: S) -> Self {
        Self(HashMap::with_hasher(s))
    }

    fn insert(
        &mut self,
        btype: &BindType,
        k: &str,
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        let v = Arc::new(v);
        self.insert_rc(btype, k, v);
    }

    fn insert_wild_event_matcher(
        &mut self,
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        self.0.insert(EventWrapper::WildEvent, Arc::new(v));
    }

    fn insert_rc(
        &mut self,
        btype: &BindType,
        k: &str,
        v: Arc<impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static>,
    ) {
        match btype {
            BindType::Key => {
                self.0
                    .insert(Event::Key(keyevent::parse_key_event(k)).into(), v);
            }
            _ => {}
        }
    }

    fn get(
        &self,
        k: &Event,
    ) -> Option<&Arc<dyn Fn(Event, &PagerState) -> InputEvent + Send + Sync>> {
        if let Some(ev) = self.0.get(&k.into()) {
            Some(ev)
        } else if let Some(wild_event) = self.0.get(&EventWrapper::WildEvent) {
            Some(wild_event)
        } else {
            None
        }
    }

    fn insert_all(
        &mut self,
        btype: &BindType,
        keys: &[&str],
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        let v = Arc::new(v);
        for k in keys {
            self.insert_rc(btype, *k, v.clone());
        }
    }
}

impl<'a> Default for HashedEventRegister<RandomState> {
    fn default() -> Self {
        Self::new(RandomState::new())
    }
}

impl<S> InputClassifier for HashedEventRegister<S>
where
    S: BuildHasher,
{
    fn classify_input(&self, ev: Event, ps: &crate::PagerState) -> Option<InputEvent> {
        self.get(&ev).map(|c| c(ev, ps))
    }
}

pub enum BindType {
    Key,
    Mouse,
    Resize,
}

/// The default keybindings in `minus`. These can be overriden by
/// making a custom input handler struct and implementing the [`InputClassifier`] trait
pub struct DefaultInputClassifier;

impl InputClassifier for DefaultInputClassifier {
    #[allow(clippy::too_many_lines)]
    fn classify_input(&self, ev: Event, ps: &PagerState) -> Option<InputEvent> {
        #[allow(clippy::unnested_or_patterns)]
        match ev {
            // Scroll up by one.
            Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
            }) if code == KeyCode::Up || code == KeyCode::Char('k') => {
                let position = ps.prefix_num.parse::<usize>().unwrap_or(1);
                Some(InputEvent::UpdateUpperMark(
                    ps.upper_mark.saturating_sub(position),
                ))
            }

            // Scroll down by one.
            Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
            }) if code == KeyCode::Down || code == KeyCode::Char('j') => {
                let position = ps.prefix_num.parse::<usize>().unwrap_or(1);
                Some(InputEvent::UpdateUpperMark(
                    ps.upper_mark.saturating_add(position),
                ))
            }

            // For number keys
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
            }) if c.is_ascii_digit() => Some(InputEvent::Number(c)),

            // Enter key
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
            }) => {
                if ps.message.is_some() {
                    Some(InputEvent::RestorePrompt)
                } else {
                    let position = ps.prefix_num.parse::<usize>().unwrap_or(1);
                    Some(InputEvent::UpdateUpperMark(
                        ps.upper_mark.saturating_add(position),
                    ))
                }
            }

            // Scroll up by half screen height.
            Event::Key(KeyEvent {
                code: KeyCode::Char('u'),
                modifiers,
            }) if modifiers == KeyModifiers::CONTROL || modifiers == KeyModifiers::NONE => {
                let half_screen = (ps.rows / 2) as usize;
                Some(InputEvent::UpdateUpperMark(
                    ps.upper_mark.saturating_sub(half_screen),
                ))
            }
            // Scroll down by half screen height.
            Event::Key(KeyEvent {
                code: KeyCode::Char('d'),
                modifiers,
            }) if modifiers == KeyModifiers::CONTROL || modifiers == KeyModifiers::NONE => {
                let half_screen = (ps.rows / 2) as usize;
                Some(InputEvent::UpdateUpperMark(
                    ps.upper_mark.saturating_add(half_screen),
                ))
            }

            // Mouse scroll up/down
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                ..
            }) => Some(InputEvent::UpdateUpperMark(ps.upper_mark.saturating_sub(5))),
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                ..
            }) => Some(InputEvent::UpdateUpperMark(ps.upper_mark.saturating_add(5))),
            // Go to top.
            Event::Key(KeyEvent {
                code: KeyCode::Char('g'),
                modifiers: KeyModifiers::NONE,
            }) => Some(InputEvent::UpdateUpperMark(0)),
            // Go to bottom.
            Event::Key(KeyEvent {
                code: KeyCode::Char('g'),
                modifiers: KeyModifiers::SHIFT,
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Char('G'),
                modifiers: KeyModifiers::SHIFT,
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Char('G'),
                modifiers: KeyModifiers::NONE,
            }) => {
                let mut position = ps
                    .prefix_num
                    .parse::<usize>()
                    .unwrap_or(usize::MAX)
                    // Reduce 1 here, because line numbering starts from 1
                    // while upper_mark starts from 0
                    .saturating_sub(1);
                if position == 0 {
                    position = usize::MAX;
                }
                Some(InputEvent::UpdateUpperMark(position))
            }

            // Page Up/Down
            Event::Key(KeyEvent {
                code: KeyCode::PageUp,
                modifiers: KeyModifiers::NONE,
            }) => Some(InputEvent::UpdateUpperMark(
                ps.upper_mark.saturating_sub(ps.rows - 1),
            )),
            Event::Key(KeyEvent {
                code: c,
                modifiers: KeyModifiers::NONE,
            }) if c == KeyCode::PageDown || c == KeyCode::Char(' ') => Some(
                InputEvent::UpdateUpperMark(ps.upper_mark.saturating_add(ps.rows - 1)),
            ),

            // Resize event from the terminal.
            Event::Resize(cols, rows) => {
                Some(InputEvent::UpdateTermArea(cols as usize, rows as usize))
            }
            // Switch line number display.
            Event::Key(KeyEvent {
                code: KeyCode::Char('l'),
                modifiers: KeyModifiers::CONTROL,
            }) => Some(InputEvent::UpdateLineNumber(!ps.line_numbers)),
            // Quit.
            Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
            })
            | Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
            }) => Some(InputEvent::Exit),
            #[cfg(feature = "search")]
            Event::Key(KeyEvent {
                code: KeyCode::Char('/'),
                modifiers: KeyModifiers::NONE,
            }) => Some(InputEvent::Search(SearchMode::Forward)),
            #[cfg(feature = "search")]
            Event::Key(KeyEvent {
                code: KeyCode::Char('?'),
                modifiers: KeyModifiers::NONE,
            }) => Some(InputEvent::Search(SearchMode::Reverse)),
            #[cfg(feature = "search")]
            Event::Key(KeyEvent {
                code: KeyCode::Char('n'),
                modifiers: KeyModifiers::NONE,
            }) => {
                let position = ps.prefix_num.parse::<usize>().unwrap_or(1);
                if ps.search_mode == SearchMode::Reverse {
                    Some(InputEvent::MoveToPrevMatch(position))
                } else {
                    Some(InputEvent::MoveToNextMatch(position))
                }
            }
            #[cfg(feature = "search")]
            Event::Key(KeyEvent {
                code: KeyCode::Char('p'),
                modifiers: KeyModifiers::NONE,
            }) => {
                let position = ps.prefix_num.parse::<usize>().unwrap_or(1);
                if ps.search_mode == SearchMode::Reverse {
                    Some(InputEvent::MoveToNextMatch(position))
                } else {
                    Some(InputEvent::MoveToPrevMatch(position))
                }
            }
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests;
