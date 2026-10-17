use osc7501::parser::{Event, MAX_SEQUENCE, parse_sequence};

#[derive(Default)]
struct Dispatch(bool);

impl vte::Perform for Dispatch {
    fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {
        self.0 = true;
    }
}

#[derive(Default, PartialEq, Eq)]
enum Context {
    #[default]
    Ground,
    Osc,
    Overflow,
}

#[derive(Default)]
pub(super) struct QueryFilter {
    parser: vte::Parser<MAX_SEQUENCE>,
    held: Vec<u8>,
    escaped: bool,
    context: Context,
    dispatched: bool,
}

impl QueryFilter {
    /// Preserve all bytes except complete, valid OSC 7501 support queries.
    pub fn push(&mut self, bytes: &[u8], output: &mut Vec<u8>) -> usize {
        let mut queries = 0;
        for &byte in bytes {
            let mut dispatch = Dispatch::default();
            self.parser.advance(&mut dispatch, &[byte]);
            self.dispatched |= dispatch.0;
            if self.context != Context::Ground && self.escaped && byte != b'\\' {
                // The ESC can start a new control sequence after cancelling OSC.
                self.held.pop();
                output.append(&mut self.held);
                self.reset();
                self.held.push(0x1b);
                self.escaped = true;
            }
            if self.context != Context::Ground {
                if self.held.len() == MAX_SEQUENCE {
                    output.append(&mut self.held);
                    self.context = Context::Overflow;
                }
                if self.context == Context::Overflow && byte != 0x1b {
                    output.append(&mut self.held);
                    output.push(byte);
                } else {
                    self.held.push(byte);
                }
                if byte == 7 || (self.escaped && byte == b'\\') {
                    if self.context != Context::Overflow
                        && self.dispatched
                        && matches!(parse_sequence(&self.held), Ok(Some(Event::Query)))
                    {
                        queries += 1;
                    } else {
                        output.append(&mut self.held);
                    }
                    self.reset();
                } else if byte == 0x18 || byte == 0x1a {
                    output.append(&mut self.held);
                    self.reset();
                } else {
                    self.escaped = byte == 0x1b;
                }
            } else if self.escaped {
                if byte == b']' {
                    self.held.push(byte);
                    self.context = Context::Osc;
                    self.escaped = false;
                } else {
                    output.append(&mut self.held);
                    self.escaped = byte == 0x1b;
                    if self.escaped {
                        self.held.push(byte);
                    } else {
                        output.push(byte);
                    }
                }
            } else if byte == 0x1b {
                self.held.push(byte);
                self.escaped = true;
            } else {
                output.push(byte);
            }
        }
        queries
    }

    pub fn finish(&mut self, output: &mut Vec<u8>) {
        output.append(&mut self.held);
        self.reset();
    }

    fn reset(&mut self) {
        self.held.clear();
        self.escaped = false;
        self.context = Context::Ground;
        self.dispatched = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_are_removed_at_every_chunk_split_and_other_bytes_are_identical() {
        let source = b"before\x1b]7501;?\x1b\\middle\x1b]7501;state=done\x07\x1b]7501;?\x07after";
        for split in 0..=source.len() {
            let (left, right) = source.split_at(split);
            let mut filter = QueryFilter::default();
            let mut output = Vec::new();
            let count = filter.push(left, &mut output) + filter.push(right, &mut output);
            filter.finish(&mut output);
            assert_eq!(count, 2);
            assert_eq!(output, b"beforemiddle\x1b]7501;state=done\x07after");
        }
    }

    #[test]
    fn malformed_cancelled_and_incomplete_sequences_pass_through() {
        for source in [
            &b"\x1b]7501;?\x18"[..],
            &b"\x1b]7501;?\x1a"[..],
            &b"\x1b]7501;?\x1bc"[..],
            &b"\x1b]7501;?"[..],
            &b"\x1b]7501;?:states=\x07"[..],
            &b"\x1b]7501;?\x1b[0m"[..],
            &b"\x1bPignored\x07\x1b\\"[..],
            &b"\x1b]other;7501;?\x07"[..],
            &b"\xc3\x9b7501;?\x07\x1b"[..],
        ] {
            let mut filter = QueryFilter::default();
            let mut output = Vec::new();
            for byte in source {
                assert_eq!(filter.push(&[*byte], &mut output), 0);
            }
            filter.finish(&mut output);
            assert_eq!(output, source);
        }
    }

    #[test]
    fn oversized_osc_does_not_hide_a_later_nested_query() {
        let mut source = b"\x1b]7501;?".to_vec();
        source.extend(std::iter::repeat_n(b'x', MAX_SEQUENCE * 2));
        let prefix = source.clone();
        source.extend_from_slice(b"\x1b]7501;?\x1b\\tail");
        let mut filter = QueryFilter::default();
        let mut output = Vec::new();
        let mut queries = 0;
        for byte in source {
            queries += filter.push(&[byte], &mut output);
        }
        filter.finish(&mut output);
        assert_eq!(queries, 1);
        assert_eq!(output, [prefix.as_slice(), b"tail"].concat());
    }
}
