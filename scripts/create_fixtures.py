"""Reproduce authored synthetic lexer fixtures (not external CAD exports)."""

import json
from pathlib import Path

CASES = [
    ("empty", b"", []),
    ("padding", b" \r\n;;\t", []),
    ("line", b"IN;SP1;PU0,0;PD40,80;", ["IN", "SP", "PU", "PD"]),
    ("concatenated", b"INSP1PA0,0PD40,80;", ["IN", "SP", "PA", "PD"]),
    ("lowercase", b"in;sp1;pu;", ["IN", "SP", "PU"]),
    ("decimals", b"PA+.5-1.25,2.;", ["PA"]),
    ("whitespace", b"PA 1 2\nSP 1\t", ["PA", "SP"]),
    ("label", b"LBhello\x03", ["LB"]),
    ("label_commands", b"LBIN;SP1;PD;\x03IN;", ["LB", "IN"]),
    ("label_binary", b"LB\xff\x80\x03", ["LB"]),
    ("dt_custom", b"DT#;LBitem#;SP1;", ["DT", "LB", "SP"]),
    ("dt_print", b"DT#,0;LBitem#", ["DT", "LB"]),
    ("dt_space", b"DT ;LBitem SP1;", ["DT", "LB", "SP"]),
    ("dt_digit", b"DT7;LBitem7", ["DT", "LB"]),
    ("dt_reset", b"DT#;DT;LB#\x03", ["DT", "DT", "LB"]),
    ("in_reset", b"DT#;IN;LB#\x03", ["DT", "IN", "LB"]),
    ("df_reset", b"DT#;DF;LB#\x03", ["DT", "DF", "LB"]),
    ("begin_plot", b"BPINPS16800,23760;", ["BP", "IN", "PS"]),
    ("comment", b'CO"LB;IN;\xff"SP1;', ["CO", "SP"]),
    ("pe_raw", b"PE:>=<7AB\x80\xff;IN;", ["PE", "IN"]),
    ("pe_empty", b"PE;", ["PE"]),
    ("exponent", b"PA1e3,2;", "EXPONENT_NOT_ALLOWED"),
    ("empty_parameter", b"PA1,,2;", "MISSING_PARAMETER"),
    ("missing_final_terminator", b"IN", "MISSING_TERMINATOR"),
    ("missing_label_terminator", b"LBtext;SP1;", "UNTERMINATED_LABEL"),
    ("missing_pe_terminator", b"PEabcIN", "UNTERMINATED_PE"),
    ("missing_comment_quote", b'CO"text;IN;', "UNTERMINATED_COMMENT"),
    ("invalid_dt", b"DT#,2;", "INVALID_DT"),
    ("unsupported_grammar", b"ZZabc;", "UNSUPPORTED_SYNTAX"),
    ("pcl_container", b"\x1b%0BIN;", "UNSUPPORTED_CONTAINER"),
    ("pdf_signature", b"%PDF-1.7", "UNSUPPORTED_CONTAINER"),
]


def main():
    directory = Path(__file__).resolve().parents[1] / "tests" / "fixtures"
    directory.mkdir(parents=True, exist_ok=True)
    manifest = []
    for name, data, expected in CASES:
        name += ".hpgl"
        (directory / name).write_bytes(data)
        manifest.append({"file": name, "kind": "synthetic", "author": "ezhpgl contributors",
                         "commands" if isinstance(expected, list) else "error": expected})
    (directory / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
