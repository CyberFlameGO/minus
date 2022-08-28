pub mod keydefs;

use crossterm::event::KeyModifiers;
use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static MODIFIERS: Lazy<HashMap<char, KeyModifiers>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert('m', KeyModifiers::ALT);
    map.insert('c', KeyModifiers::CONTROL);
    map.insert('s', KeyModifiers::SHIFT);

    map
});
