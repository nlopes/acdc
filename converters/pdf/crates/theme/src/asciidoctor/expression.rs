/// Evaluate only numeric theme arithmetic. No theme text is executed as code.
pub(super) fn evaluate(input: &str) -> Option<f64> {
    if input.len() > 1024 {
        return None;
    }
    if let Ok(value) = input.trim().parse::<f64>() {
        return value.is_finite().then_some(value);
    }
    let mut parser = Parser {
        remaining: input.trim(),
        depth: 0,
    };
    let value = parser.sum()?;
    (parser.remaining.trim().is_empty() && value.is_finite()).then_some(value)
}

struct Parser<'a> {
    remaining: &'a str,
    depth: usize,
}

impl Parser<'_> {
    fn consume(&mut self, token: &str) -> bool {
        self.remaining = self.remaining.trim_start();
        if let Some(rest) = self.remaining.strip_prefix(token) {
            self.remaining = rest;
            true
        } else {
            false
        }
    }

    fn sum(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        loop {
            if self.consume("+") {
                value += self.product()?;
            } else if self.consume("-") {
                value -= self.product()?;
            } else {
                return Some(value);
            }
        }
    }

    fn product(&mut self) -> Option<f64> {
        let mut value = self.atom()?;
        loop {
            if self.consume("*") {
                value *= self.atom()?;
            } else if self.consume("/") {
                value /= self.atom()?;
            } else {
                return Some(value);
            }
        }
    }

    fn atom(&mut self) -> Option<f64> {
        if self.depth >= 16 {
            return None;
        }
        for function in ["round", "floor", "ceil", ""] {
            let original = self.remaining;
            if (function.is_empty() || self.consume(function)) && self.consume("(") {
                self.depth += 1;
                let value = self.sum()?;
                self.depth -= 1;
                if !self.consume(")") {
                    return None;
                }
                return Some(match function {
                    "round" => value.round(),
                    "floor" => value.floor(),
                    "ceil" => value.ceil(),
                    _ => value,
                });
            }
            self.remaining = original;
        }
        self.remaining = self.remaining.trim_start();
        let sign = if self.consume("-") { -1.0 } else { 1.0 };
        let _ = self.consume("+");
        let end = self
            .remaining
            .find(|character: char| !character.is_ascii_digit() && character != '.')
            .unwrap_or(self.remaining.len());
        let number = self.remaining.get(..end)?.parse::<f64>().ok()?;
        self.remaining = self.remaining.get(end..)?;
        let mut factor = 1.0;
        for (unit, scale) in [
            ("pt", 1.0),
            ("in", 72.0),
            ("cm", 72.0 / 2.54),
            ("mm", 72.0 / 25.4),
            ("px", 0.75),
            ("pc", 12.0),
        ] {
            if self.remaining.starts_with(unit) {
                self.remaining = self.remaining.strip_prefix(unit)?;
                factor = scale;
                break;
            }
        }
        Some(sign * number * factor)
    }
}
