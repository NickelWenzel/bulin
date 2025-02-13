use crate::editor::Editor;
use serde::de;
use serde::de::{MapAccess, SeqAccess, Visitor};
use std::fmt;

pub struct EditorVisitor;
impl<'de> Visitor<'de> for EditorVisitor {
    type Value = Editor;

    // Format a message stating what data this Visitor expects to receive.
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("file, content and theme of an bulin editor")
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
    where
        V: SeqAccess<'de>,
    {
        let file = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let content: String = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;
        let theme = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;

        Ok(Editor::simple_new()
            .with_file(file)
            .with_content(&content)
            .with_theme(theme))
    }
    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut file = None;
        let mut content: Option<String> = None;
        let mut theme = None;

        while let Some(key) = map.next_key()? {
            match key {
                "file" => {
                    if file.is_some() {
                        return Err(de::Error::duplicate_field("file"));
                    }
                    file = Some(map.next_value()?);
                }
                "content" => {
                    if content.is_some() {
                        return Err(de::Error::duplicate_field("content"));
                    }
                    content = Some(map.next_value()?);
                }
                "theme" => {
                    if theme.is_some() {
                        return Err(de::Error::duplicate_field("theme"));
                    }
                    theme = Some(map.next_value()?);
                }
                unknown => {
                    return Err(de::Error::unknown_field(
                        unknown,
                        &["file", "content", "theme"],
                    ))
                }
            }
        }
        let file = file.ok_or_else(|| de::Error::missing_field("file"))?;
        let content = content.ok_or_else(|| de::Error::missing_field("content"))?;
        let theme = theme.ok_or_else(|| de::Error::missing_field("theme"))?;

        Ok(Editor::simple_new()
            .with_file(file)
            .with_content(&content)
            .with_theme(theme))
    }
}
