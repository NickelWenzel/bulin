use std::ops::{Deref, DerefMut};

use iced::widget::text_editor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub struct Content(text_editor::Content);

impl Content {
    pub fn new() -> Self {
        Content(text_editor::Content::new())
    }

    pub fn with_text(text: &str) -> Self {
        Content(text_editor::Content::with_text(text))
    }
}

impl Deref for Content {
    type Target = text_editor::Content;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Content {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Serialize for Content {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.text().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Content {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Content::with_text(&s))
    }
}
