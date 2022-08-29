use super::{InputClassifier, InputEvent};
use crate::PagerState;
use crossterm::event::{Event, MouseEvent};
use std::{
    collections::hash_map::RandomState, collections::HashMap, hash::BuildHasher, hash::Hash,
    sync::Arc,
};

type EventReturnType = Arc<dyn Fn(Event, &PagerState) -> InputEvent + Send + Sync>;

pub struct HashedEventRegister<S>(HashMap<EventWrapper, EventReturnType, S>);

#[derive(Copy, Clone, Eq)]
enum EventWrapper {
    ExactMatchEvent(Event),
    WildEvent,
}

impl From<Event> for EventWrapper {
    fn from(e: Event) -> Self {
        Self::ExactMatchEvent(e)
    }
}

impl From<&Event> for EventWrapper {
    fn from(e: &Event) -> Self {
        Self::ExactMatchEvent(*e)
    }
}

impl PartialEq for EventWrapper {
    fn eq(&self, other: &Self) -> bool {
        if let Self::ExactMatchEvent(Event::Mouse(MouseEvent {
            kind, modifiers, ..
        })) = self
        {
            let (o_kind, o_modifiers) = if let Self::ExactMatchEvent(Event::Mouse(MouseEvent {
                kind: o_kind,
                modifiers: o_modifiers,
                ..
            })) = other
            {
                (o_kind, o_modifiers)
            } else {
                unreachable!()
            };
            kind == o_kind && modifiers == o_modifiers
        } else {
            self == other
        }
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
            Self::WildEvent | Self::ExactMatchEvent(Event::Resize(..)) => {}
            Self::ExactMatchEvent(v) => {
                v.hash(state);
            }
        }
    }
}

impl<S> HashedEventRegister<S>
where
    S: BuildHasher,
{
    fn new(s: S) -> Self {
        Self(HashMap::with_hasher(s))
    }

    pub fn insert_wild_event_matcher(
        &mut self,
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        self.0.insert(EventWrapper::WildEvent, Arc::new(v));
    }

    pub fn get(&self, k: &Event) -> Option<&EventReturnType> {
        self.0
            .get(&k.into())
            .map_or_else(|| self.0.get(&EventWrapper::WildEvent), |k| Some(k))
    }

    pub fn add_resize_event(
        &mut self,
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        let v = Arc::new(v);
        self.0
            .insert(EventWrapper::ExactMatchEvent(Event::Resize(0, 0)), v);
    }
}

// Key event Insertions functions
impl<S> HashedEventRegister<S>
where
    S: BuildHasher,
{
    pub fn add_key_event(
        &mut self,
        k: &str,
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        let v = Arc::new(v);
        self.0.insert(
            Event::Key(super::definitions::keydefs::parse_key_event(k)).into(),
            v,
        );
    }

    pub fn add_key_events(
        &mut self,
        keys: &[&str],
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        let v = Arc::new(v);
        for k in keys {
            self.0.insert(
                Event::Key(super::definitions::keydefs::parse_key_event(k)).into(),
                v.clone(),
            );
        }
    }
}

// Mouse event insertions functions
impl<S> HashedEventRegister<S>
where
    S: BuildHasher,
{
    pub fn add_mouse_event(
        &mut self,
        k: &str,
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        let v = Arc::new(v);
        self.0.insert(
            Event::Mouse(super::definitions::mousedefs::parse_mouse_event(k)).into(),
            v,
        );
    }

    pub fn add_mouse_events(
        &mut self,
        keys: &[&str],
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        let v = Arc::new(v);
        for k in keys {
            self.0.insert(
                Event::Mouse(super::definitions::mousedefs::parse_mouse_event(k)).into(),
                v.clone(),
            );
        }
    }
}

impl Default for HashedEventRegister<RandomState> {
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
