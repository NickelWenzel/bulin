pub fn serialize<S>(value: &text_editor::Content, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.text())
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<text_editor::Content, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(text_editor::Content::with_text(s))
}
