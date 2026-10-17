use osc7501_parser::MAX_SEQUENCE;

#[derive(Default)]
struct Dispatch {
    osc: bool,
    reset: bool,
}

impl vte::Perform for Dispatch {
    fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {
        self.osc = true;
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.reset = !ignore && intermediates.is_empty() && byte == b'c';
    }
}

pub(super) enum Control {
    Osc(Vec<u8>),
    Oversized,
    Reset,
}

#[derive(Default)]
pub(super) struct Framer {
    // vte decides whether OSC bytes occur in a valid terminal context.
    // Keep the raw frame because its parameter list can exceed vte's field cap.
    parser: vte::Parser<MAX_SEQUENCE>,
    held: Vec<u8>,
    escaped: bool,
    in_osc: bool,
    dispatched: bool,
    length: usize,
}

impl Framer {
    pub fn discard_pending(&mut self) {
        self.held.clear();
        self.in_osc = false;
        self.dispatched = false;
        self.length = 0;
    }

    pub fn boundary(&mut self) {
        self.discard_pending();
        self.escaped = false;
    }

    pub fn push(&mut self, byte: u8) -> Option<Control> {
        let mut dispatch = Dispatch::default();
        self.parser.advance(&mut dispatch, &[byte]);
        if self.in_osc {
            // vte dispatches at ESC; wait for ST's backslash before applying it.
            self.dispatched |= dispatch.osc;
            if self.held.len() < MAX_SEQUENCE {
                self.held.push(byte);
            }
            self.length = self.length.saturating_add(1);
            let complete = byte == 7 || (self.escaped && byte == b'\\');
            let cancelled = byte == 0x18 || byte == 0x1a || (self.escaped && byte != b'\\');
            if complete {
                let control = if self.dispatched && self.length > MAX_SEQUENCE {
                    self.held
                        .starts_with(b"\x1b]7501;")
                        .then_some(Control::Oversized)
                } else if self.dispatched {
                    Some(Control::Osc(std::mem::take(&mut self.held)))
                } else {
                    None
                };
                self.discard_pending();
                self.escaped = false;
                return control;
            }
            if cancelled {
                self.discard_pending();
                if self.escaped && byte == b']' {
                    self.held.extend_from_slice(b"\x1b]");
                    self.in_osc = true;
                    self.length = 2;
                }
            }
        } else if self.escaped && byte == b']' {
            self.held.clear();
            self.held.extend_from_slice(b"\x1b]");
            self.in_osc = true;
            self.length = 2;
        }
        self.escaped = byte == 0x1b;
        dispatch.reset.then_some(Control::Reset)
    }
}
