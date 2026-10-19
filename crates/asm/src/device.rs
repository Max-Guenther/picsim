//! Vordefinierte Symbole je PIC-Typ. Namen wie in iL_ASS16, damit
//! vorhandene Quelltexte ohne eigene EQUs übersetzen.

use crate::expr::Value;

const SUPPORTED: &[&str] = &["16F84", "16F84A", "16C84", "16F83", "16C83"];

pub fn supported(name: &str) -> bool {
    SUPPORTED.contains(&name)
}

/// (Name, Wert). Text-Werte mit Komma stehen für "Register,Bit".
const PIC16F84: &[(&str, &str)] = &[
    ("W", "0"),
    ("F", "1"),
    ("INDIRECT", "00H"),
    ("INDF", "00H"),
    ("TMR0", "01H"),
    ("RTCC", "01H"),
    ("PCL", "02H"),
    ("PC", "02H"),
    ("STATUS", "03H"),
    ("IRP", "7"),
    ("RP1", "6"),
    ("RP0", "5"),
    ("TO", "4"),
    ("PD", "3"),
    ("Z", "2"),
    ("DC", "1"),
    ("C", "0"),
    ("FSR", "04H"),
    ("PORTA", "05H"),
    ("RA", "05H"),
    ("PORTB", "06H"),
    ("RB", "06H"),
    ("EEDATA", "08H"),
    ("EEDTA", "08H"),
    ("EEADR", "09H"),
    ("PCLATH", "0AH"),
    ("INTCON", "0BH"),
    ("GIE", "0BH,7"),
    ("EEIE", "0BH,6"),
    ("T0IE", "0BH,5"),
    ("INTE", "0BH,4"),
    ("RBIE", "0BH,3"),
    ("T0IF", "0BH,2"),
    ("INTF", "0BH,1"),
    ("RBIF", "0BH,0"),
    ("OPTION_R", "81H"),
    ("OPTION_REG", "81H"),
    ("RBPU", "81H,7"),
    ("INTEDG", "81H,6"),
    ("T0CS", "81H,5"),
    ("T0SE", "81H,4"),
    ("PSA", "81H,3"),
    ("PS2", "81H,2"),
    ("PS1", "81H,1"),
    ("PS0", "81H,0"),
    ("TRISA", "85H"),
    ("TRISB", "86H"),
    ("EECON1", "88H"),
    ("EEIF", "88H,4"),
    ("WRERR", "88H,3"),
    ("WREN", "88H,2"),
    ("WR_", "88H,1"),
    ("RD_", "88H,0"),
    ("EECON2", "89H"),
    // Laufzeitvariablen des iL-BASIC, von manchen Vorlagen benutzt
    ("PUPOPW", "0CH"),
    ("PUPOPS", "0DH"),
    ("PUPOPP", "0EH"),
    ("PUPOPF", "0FH"),
    ("CONF", "2007H"),
];

pub fn symbols(_name: &str) -> Vec<(&'static str, Value)> {
    PIC16F84
        .iter()
        .map(|(k, v)| {
            let val = if v.contains(',') {
                Value::Text(v.to_string())
            } else {
                Value::Num(crate::expr::parse_number(v).expect("Tabelle ist gültig"))
            };
            (*k, val)
        })
        .collect()
}
