//! Lossless Forge configuration editing. Only a selected value span changes;
//! comments, unknown properties, whitespace and line endings remain intact.
use std::ops::Range;
use std::sync::OnceLock;
#[derive(Clone, Debug)]
pub struct Field {
    pub section: String,
    pub key: String,
    pub kind: char,
    pub value: String,
    pub description: String,
    pub multiline: bool,
    range: Range<usize>,
    indent: String,
}
#[derive(Clone, Debug)]
pub struct Document {
    pub fields: Vec<Field>,
}
pub fn is_forge_file(path: &str) -> bool {
    let path = path.to_lowercase();
    path.ends_with(".cfg") || path.ends_with(".properties")
}
pub fn parse(content: &str) -> Option<Document> {
    static PROPERTY: OnceLock<regex::Regex> = OnceLock::new();
    let property = PROPERTY.get_or_init(|| {
        regex::Regex::new(r#"^\s*([BISD]):(?:"([^"]+)"|([^=<]+?))\s*(=|<)(.*)$"#).unwrap()
    });
    let lines: Vec<_> = content.split_inclusive('\n').collect();
    let mut section: Vec<String> = Vec::new();
    let mut fields = Vec::new();
    let mut description = Vec::new();
    let mut offset = 0;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if let Some(comment) = trimmed.strip_prefix('#') {
            description.push(comment.trim().to_string());
        } else if trimmed.ends_with('{') {
            section.push(
                trimmed
                    .trim_end_matches('{')
                    .trim()
                    .trim_matches('"')
                    .to_string(),
            );
            description.clear();
        } else if trimmed == "}" {
            section.pop();
            description.clear();
        } else if !trimmed.is_empty() {
            let captures = property.captures(line.trim_end_matches(['\r', '\n']))?;
            if section.is_empty() {
                return None;
            }
            let kind = captures[1].chars().next()?;
            let key = captures
                .get(2)
                .or_else(|| captures.get(3))?
                .as_str()
                .trim()
                .to_string();
            let multiline = &captures[4] == "<";
            let indent: String = line
                .chars()
                .take_while(|c| c.is_whitespace() && *c != '\r' && *c != '\n')
                .collect();
            let (range, value) = if multiline {
                if !captures[5].trim().is_empty() {
                    return None;
                }
                let start = offset + line.len();
                let mut end = start;
                let mut values = Vec::new();
                let mut j = i + 1;
                while j < lines.len() && lines[j].trim() != ">" {
                    if !lines[j].trim().is_empty() {
                        values.push(lines[j].trim());
                    }
                    end += lines[j].len();
                    j += 1;
                }
                if j == lines.len() {
                    return None;
                }
                offset = end;
                i = j;
                (start..end, values.join("\n"))
            } else {
                let raw = captures.get(5)?;
                let value = raw.as_str().trim();
                let leading = raw.as_str().len() - raw.as_str().trim_start().len();
                let start = offset + raw.start() + leading;
                (start..start + value.len(), value.to_string())
            };
            fields.push(Field {
                section: section.join(" / "),
                key,
                kind,
                value,
                description: description.join("\n"),
                multiline,
                range,
                indent,
            });
            description.clear();
        }
        offset += lines[i].len();
        i += 1;
    }
    if fields.is_empty() {
        None
    } else {
        Some(Document { fields })
    }
}
pub fn update(content: &str, index: usize, value: &str) -> anyhow::Result<String> {
    let doc =
        parse(content).ok_or_else(|| anyhow::anyhow!("This file needs the advanced editor"))?;
    let field = doc
        .fields
        .get(index)
        .ok_or_else(|| anyhow::anyhow!("Property no longer exists"))?;
    if !field.multiline {
        anyhow::ensure!(
            !value.contains(['\r', '\n']),
            "This property needs a single-line value"
        );
        match field.kind {
            'B' => anyhow::ensure!(value == "true" || value == "false", "Enter true or false"),
            'I' => {
                value
                    .parse::<i64>()
                    .map_err(|_| anyhow::anyhow!("Enter a whole number"))?;
            }
            'D' => {
                let number = value
                    .parse::<f64>()
                    .map_err(|_| anyhow::anyhow!("Enter a number"))?;
                anyhow::ensure!(number.is_finite(), "Enter a finite number");
            }
            _ => {}
        }
    }
    let replacement = if field.multiline {
        let newline = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        value
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| format!("{}    {}{}", field.indent, line, newline))
            .collect::<String>()
    } else {
        value.into()
    };
    let mut next = content.to_string();
    next.replace_range(field.range.clone(), &replacement);
    Ok(next)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_edit_preserves_other_values_comments_and_crlf() {
        let source = "# Configuration file\r\n\r\ngeneral {\r\n    # first\r\n    # second\r\n    B:enabled = true  \r\n    I:percent=75\r\n    D:factor=1.25\r\n    S:\"research parents\" <\r\n        FOCALMANIPULATION\r\n     >\r\n}\r\n";
        let doc = parse(source).unwrap();
        assert_eq!(doc.fields.len(), 4);
        assert_eq!(doc.fields[0].description, "first\nsecond");
        assert_eq!(
            update(source, 0, "false").unwrap(),
            source.replacen("= true", "= false", 1)
        );
        assert_eq!(
            update(source, 1, "80").unwrap(),
            source.replacen("=75", "=80", 1)
        );
        assert!(update(source, 1, "7x").is_err());
        assert!(update(source, 2, "NaN").is_err());
        let lists = update(source, 3, "A\nB").unwrap();
        assert_eq!(parse(&lists).unwrap().fields[3].value, "A\nB");
        assert!(lists.contains("\r\n"));
        assert!(parse("difficulty=2\n").is_none());
        assert!(parse("general {\nS:list <\nA\n").is_none());
    }
}
