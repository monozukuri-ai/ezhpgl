use crate::command::is_known_command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectionResult {
    pub container: &'static str,
    pub language: &'static str,
    pub reasons: Vec<&'static str>,
}

/// Plot streams are text: mnemonics, numbers, label text and PCL/PJL escapes. Two or
/// more NUL bytes near the start, or a run of other C0 control bytes, mark a binary
/// file whose first two bytes merely spell a mnemonic (an ARCDRAW `.awf` drawing
/// starts with `AD2008\0\0`, and `AD` is the alternate-font command). A single NUL is
/// allowed: transparent-data labels (`TD1`) may carry one. Tabs, line ends, form feed,
/// ESC, ETX (the default label terminator) and SO/SI (character set shifts) are text.
/// The control-byte ratio is only judged on 64 bytes or more, so a short stream with a
/// single stray control byte keeps its own diagnostic (`DT\0;` is `INVALID_DT`).
fn looks_binary(data: &[u8]) -> bool {
    let head = &data[..data.len().min(1024)];
    if head.iter().take(512).filter(|&&b| b == 0).count() >= 2 {
        return true;
    }
    if head.len() < 64 {
        return false;
    }
    let control = head
        .iter()
        .filter(|&&b| {
            b < 0x20
                && !matches!(
                    b,
                    0 | b'\t' | b'\n' | b'\r' | 0x03 | 0x0c | 0x0e | 0x0f | 0x1b
                )
        })
        .count();
    control * 10 > head.len()
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
    } else if looks_binary(data) {
        (
            "binary",
            "unknown",
            "Binary content (NUL or control bytes near the start); not a plot command stream",
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
