//! Pin-Referenzen wie in der picsimu.ini ("RB0", "nc", "Vdd", "GND").

use std::fmt;

pub const PORT_NAMES: [&str; 5] = ["RA", "RB", "RC", "RD", "RE"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PinRef {
    #[default]
    Nc,
    Vdd,
    Gnd,
    Port { port: u8, bit: u8 },
}

impl PinRef {
    pub fn port(port: u8, bit: u8) -> Self {
        PinRef::Port { port, bit }
    }

    pub fn parse(s: &str) -> Self {
        let s = s.trim();
        match s.to_ascii_lowercase().as_str() {
            "" | "nc" => return PinRef::Nc,
            "vdd" | "vcc" | "5v" => return PinRef::Vdd,
            "gnd" | "0v" => return PinRef::Gnd,
            _ => {}
        }
        let up = s.to_ascii_uppercase();
        if up.len() == 3 && up.starts_with('R') {
            let port = PORT_NAMES.iter().position(|p| up.starts_with(p));
            let bit = up[2..].parse::<u8>().ok();
            if let (Some(port), Some(bit)) = (port, bit) {
                if bit < 8 {
                    return PinRef::port(port as u8, bit);
                }
            }
        }
        PinRef::Nc
    }

    pub fn is_connected(&self) -> bool {
        !matches!(self, PinRef::Nc)
    }
}

impl fmt::Display for PinRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PinRef::Nc => write!(f, "nc"),
            PinRef::Vdd => write!(f, "Vdd"),
            PinRef::Gnd => write!(f, "GND"),
            PinRef::Port { port, bit } => write!(f, "{}{}", PORT_NAMES[*port as usize], bit),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for s in ["RB0", "RA4", "RC7", "nc", "Vdd", "GND"] {
            assert_eq!(PinRef::parse(s).to_string(), s);
        }
        assert_eq!(PinRef::parse("rb3"), PinRef::port(1, 3));
        assert_eq!(PinRef::parse("RB9"), PinRef::Nc);
        assert_eq!(PinRef::parse("quatsch"), PinRef::Nc);
    }
}
