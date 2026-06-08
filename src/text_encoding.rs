use anyhow::{Context, Result};
use encoding_rs::GBK;
use std::borrow::Cow;
use std::fs;
use std::path::Path;

pub fn read_to_string(path: &Path) -> Result<String> {
    decode_bytes(&fs::read(path).with_context(|| format!("Reading {}", path.display()))?)
}

pub fn decode_bytes(bytes: &[u8]) -> Result<String> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_string()),
        Err(_) => {
            let (text, _, _) = GBK.decode(bytes);
            Ok(text.into_owned())
        }
    }
}

pub fn write_utf8(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text.as_bytes()).with_context(|| format!("Writing {}", path.display()))
}

pub fn copy_as_utf8(source: &Path, target: &Path) -> Result<()> {
    let bytes = fs::read(source).with_context(|| format!("Reading {}", source.display()))?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| format!("Creating {}", parent.display()))?;
    }
    match std::str::from_utf8(&bytes) {
        Ok(_) => fs::write(target, bytes)
            .with_context(|| format!("Copying {} to {}", source.display(), target.display())),
        Err(_) => {
            let (text, _, _) = GBK.decode(&bytes);
            fs::write(target, text.as_bytes())
                .with_context(|| format!("Converting {} to UTF-8", source.display()))
        }
    }
}

pub fn repair_mojibake(text: &str) -> String {
    if !looks_like_mojibake(text) {
        return text.to_string();
    }
    let bytes: Vec<u8> = text.chars().filter_map(latin1_byte).collect();
    if bytes.len() != text.chars().count() {
        return text.to_string();
    }
    String::from_utf8(bytes)
        .map(Cow::Owned)
        .unwrap_or_else(|_| Cow::Borrowed(text))
        .into_owned()
}

fn looks_like_mojibake(text: &str) -> bool {
    text.contains('Ã')
        || text.contains('Â')
        || text.contains('ä')
        || text.contains('å')
        || text.contains('æ')
        || text.contains('ç')
}

fn latin1_byte(ch: char) -> Option<u8> {
    let code = ch as u32;
    if code <= 0xff {
        return Some(code as u8);
    }
    match ch {
        '€' => Some(0x80),
        '‚' => Some(0x82),
        'ƒ' => Some(0x83),
        '„' => Some(0x84),
        '…' => Some(0x85),
        '†' => Some(0x86),
        '‡' => Some(0x87),
        'ˆ' => Some(0x88),
        '‰' => Some(0x89),
        'Š' => Some(0x8a),
        '‹' => Some(0x8b),
        'Œ' => Some(0x8c),
        'Ž' => Some(0x8e),
        '‘' => Some(0x91),
        '’' => Some(0x92),
        '“' => Some(0x93),
        '”' => Some(0x94),
        '•' => Some(0x95),
        '–' => Some(0x96),
        '—' => Some(0x97),
        '˜' => Some(0x98),
        '™' => Some(0x99),
        'š' => Some(0x9a),
        '›' => Some(0x9b),
        'œ' => Some(0x9c),
        'ž' => Some(0x9e),
        'Ÿ' => Some(0x9f),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_gbk_when_utf8_fails() {
        let (bytes, _, _) = GBK.encode("中文 session");
        assert_eq!(decode_bytes(&bytes).unwrap(), "中文 session");
    }

    #[test]
    fn repairs_latin1_mojibake() {
        assert_eq!(repair_mojibake("ä¸­æ–‡"), "中文");
    }

    #[test]
    fn leaves_normal_text_unchanged() {
        assert_eq!(repair_mojibake("中文 session"), "中文 session");
    }
}
