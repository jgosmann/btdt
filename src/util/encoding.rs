use data_encoding::Encoding;
use data_encoding_macro::new_encoding;

pub const ICASE_NOPAD_ALPHANUMERIC_ENCODING: Encoding = new_encoding! {
    symbols: "abcdefghijklmnopqrstuvwxyz012345",
    padding: None,
    translate_from: "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    translate_to: "abcdefghijklmnopqrstuvwxyz",
};
