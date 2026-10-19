use crate::command::is_known_command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectionResult {
    pub container: &'static str,
    pub language: &'static str,
    pub reasons: Vec<&'static str>,
}

/// Prefix-only hints, never a certificate that an entire file is valid.
/// In particular, common HP-GL commands cannot distinguish HP-GL from HP-GL/2.
pub fn detect_format(data: &[u8]) -> DetectionResult {
    let start = data
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(data.len());
    let data = &data[start..];
    let (container, language, reason) = if data.starts_with(b"%PDF-") {
        ("pdf", "pdf", "PDF signature")
    } else if data.starts_with(b"%!PS") {
        ("postscript", "postscript", "PostScript signature")
    } else if data.starts_with(b"\x1b%-12345X") || data.starts_with(b"@PJL") {
        (
            "pjl",
            "unknown",
            "PJL or Universal Exit Language prefix; body not inspected",
        )
    } else if data.starts_with(b"\x1b") {
        (
            "pcl_or_control",
            "unknown",
            "Leading escape sequence; container parsing is required",
        )
    } else if data.len() >= 2
        && is_known_command(&[data[0].to_ascii_uppercase(), data[1].to_ascii_uppercase()])
    {
        (
            "plain",
            "hpgl_family",
            "Recognized leading HP-GL-family mnemonic; dialect is ambiguous",
        )
    } else {
        (
            "unknown",
            "unknown",
            "No recognized prefix; no extension-based assumption",
        )
    };
    DetectionResult {
        container,
        language,
        reasons: vec![reason],
    }
}
