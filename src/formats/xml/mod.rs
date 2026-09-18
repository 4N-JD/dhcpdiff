#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlDocument {
    pub root: String,
    pub content: String,
}

pub fn parse_xml(input: &str) -> anyhow::Result<XmlDocument> {
    let trimmed = input.trim();
    if !trimmed.starts_with('<') {
        anyhow::bail!("not valid XML");
    }
    let root = trimmed
        .trim_start_matches('<')
        .split([' ', '>', '/'])
        .next()
        .unwrap_or("unknown")
        .to_string();
    Ok(XmlDocument {
        root,
        content: input.to_string(),
    })
}
