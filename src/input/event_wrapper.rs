use super::{InputClassifier, InputEvent};
use crate::PagerState;
use crossterm::event::{Event, MouseEvent};
use std::{
    collections::hash_map::RandomState, collections::HashMap, hash::BuildHasher, hash::Hash,
    sync::Arc,
};

type EventReturnType = Arc<dyn Fn(Event, &PagerState) -> InputEvent + Send + Sync>;

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
            Self::ExactMatchEvent(v) => {
                v.hash(state);
            }
            Self::WildEvent => {}
        }
    }
}

pub struct HashedEventRegister<S>(HashMap<EventWrapper, EventReturnType, S>);

impl<S> HashedEventRegister<S>
where
    S: BuildHasher,
{
    fn new(s: S) -> Self {
        Self(HashMap::with_hasher(s))
    }

    pub fn insert(
        &mut self,
        btype: &BindType,
        k: &str,
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        let v = Arc::new(v);
        self.insert_rc(btype, k, v);
    }

    pub fn insert_wild_event_matcher(
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
                self.0.insert(
                    Event::Key(super::definitions::keydefs::parse_key_event(k)).into(),
                    v,
                );
            }
            BindType::Mouse => {
                self.0.insert(
                    Event::Mouse(super::definitions::mousedefs::parse_mouse_event(k)).into(),
                    v,
                );
            }
            BindType::Resize => todo!(),
        }
    }

    pub fn get(&self, k: &Event) -> Option<&EventReturnType> {
        self.0
            .get(&k.into())
            .map_or_else(|| self.0.get(&EventWrapper::WildEvent), |k| Some(k))
    }

    pub fn insert_all(
        &mut self,
        btype: &BindType,
        keys: &[&str],
        v: impl Fn(Event, &PagerState) -> InputEvent + Send + Sync + 'static,
    ) {
        let v = Arc::new(v);
        for k in keys {
            self.insert_rc(btype, k, v.clone());
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

pub enum BindType {
    Key,
    Mouse,
    Resize,
}
