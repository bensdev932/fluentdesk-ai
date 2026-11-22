//! Private JSON key-scanning and string-unescaping helpers for `SpokenJsonStreamParser`.

#[derive(Debug, Default)]
pub(super) struct JsonScanState {
    pub depth: usize,
    pub in_str: bool,
    pub escaped: bool,
    pub str_buf: String,
    pub awaiting_colon: bool,
    pub awaiting_value_quote: bool,
}

#[derive(Debug, Default)]
pub(super) struct UnescapeState {
    pub escaped: bool,
    pub unicode_hex: Option<String>,
}

pub(super) enum DecodedChar {
    Char(char),
    Pending,
    EndOfString,
}

pub(super) fn step_json_key_str(st: &mut JsonScanState, ch: char) {
    if st.escaped {
        st.escaped = false;
        st.str_buf.push(ch);
        return;
    }
    if ch == '\\' {
        st.escaped = true;
    } else if ch == '"' {
        st.in_str = false;
        st.awaiting_colon =
            st.depth == 1 && (st.str_buf == "spokenReply" || st.str_buf == "spoken_reply");
    } else if st.str_buf.len() < 32 {
        st.str_buf.push(ch);
    }
}

pub(super) fn step_json_unquoted(st: &mut JsonScanState, ch: char) {
    match ch {
        '"' => begin_json_str(st),
        '{' => reset_brace_depth(st, 1),
        '}' => reset_brace_depth(st, -1),
        ':' if st.awaiting_colon => {
            st.awaiting_colon = false;
            st.awaiting_value_quote = true;
        }
        c if c.is_whitespace() => {}
        _ => {
            st.awaiting_colon = false;
            st.awaiting_value_quote = false;
        }
    }
}

fn begin_json_str(st: &mut JsonScanState) {
    st.in_str = true;
    st.str_buf.clear();
    st.awaiting_colon = false;
    st.awaiting_value_quote = false;
}

fn reset_brace_depth(st: &mut JsonScanState, delta: isize) {
    st.depth = st.depth.saturating_add_signed(delta);
    st.awaiting_colon = false;
    st.awaiting_value_quote = false;
}

pub(super) fn decode_spoken_char(st: &mut UnescapeState, ch: char) -> DecodedChar {
    if let Some(hex) = &mut st.unicode_hex {
        hex.push(ch);
        return consume_unicode_hex(st);
    }
    if st.escaped {
        return consume_escape(st, ch);
    }
    match ch {
        '\\' => {
            st.escaped = true;
            DecodedChar::Pending
        }
        '"' => DecodedChar::EndOfString,
        other => DecodedChar::Char(other),
    }
}

fn consume_escape(st: &mut UnescapeState, ch: char) -> DecodedChar {
    st.escaped = false;
    match ch {
        '"' | '\\' | '/' => DecodedChar::Char(ch),
        'n' | 'r' => DecodedChar::Char('\n'),
        't' => DecodedChar::Char('\t'),
        'u' => {
            st.unicode_hex = Some(String::with_capacity(4));
            DecodedChar::Pending
        }
        other => DecodedChar::Char(other),
    }
}

fn consume_unicode_hex(st: &mut UnescapeState) -> DecodedChar {
    if !st.unicode_hex.as_ref().is_some_and(|h| h.len() == 4) {
        return DecodedChar::Pending;
    }
    let hex = st.unicode_hex.take().unwrap_or_default();
    let parsed = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32);
    parsed
        .map(DecodedChar::Char)
        .unwrap_or(DecodedChar::Pending)
}
