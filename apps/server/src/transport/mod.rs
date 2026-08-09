pub(crate) mod client_ip;
pub mod http;
pub mod websocket;

pub(crate) fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}
