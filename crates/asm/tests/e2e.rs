//! Quelltext assemblieren, im Board ausführen und die Peripherie prüfen.

use picsim_core::{Board, Ini, Listing, Stop};

fn board_with(src: &str, ini: &str) -> Board {
    let out = picsim_asm::assemble(src, "test.src", None);
    assert!(out.ok(), "{:?}", out.errors);
    let mut b = Board::new(Ini::parse(ini));
    b.load_listing(Listing::parse(&out.lst));
    b
}

#[test]
fn lcd_example_shows_text() {
    let ini = "[LCD]\nRS=RA0\nE=RA1\nRW=GND\nD4=RB4\nD5=RB5\nD6=RB6\nD7=RB7\nFORMAT_4=1\n";
    let mut b = board_with(include_str!("../../../examples/lcd_hallo.src"), ini);
    b.go();
    assert_eq!(b.run(200_000), Stop::Budget);
    let text = b.lcd.text();
    assert_eq!(text[0], "Hallo PicSim    ");
    assert!(text[1].starts_with(|c: char| c.is_ascii_digit()), "{:?}", text[1]);
}

#[test]
fn listing_breakpoint_on_label() {
    let src = "\torg 0\nstart\tnop\nziel\tnop\n\tgoto start\n";
    let mut b = board_with(src, "");
    let addr = b.listing.lines.iter().find(|l| l.text.contains("ziel")).and_then(|l| l.code).unwrap().0;
    b.toggle_breakpoint(addr);
    b.go();
    assert_eq!(b.run(100), Stop::Breakpoint);
    assert_eq!(b.cpu.pc, 1);
}
