use crate::bits2d::Bits2d;
use crossterm::{ExecutableCommand, QueueableCommand};
use std::io::{Result, Stdout, Write};

pub fn run<T>(stdout: Stdout, memory: T, on_event: impl Fn(&mut Handler<T>, Option<crossterm::event::KeyEvent>) -> bool) -> Result<()> {
    let mut handler = Handler::new(stdout, memory)?;
    on_event(&mut handler, None);
    handler.render_bits()?;
    loop {
        match crossterm::event::read()? {
                crossterm::event::Event::FocusGained => {},
                crossterm::event::Event::FocusLost => {},
                crossterm::event::Event::Key(key_event) => {
                    if on_event(&mut handler, Some(key_event)) {
                        break;
                    }
                },
                crossterm::event::Event::Mouse(_mouse_event) => {},
                crossterm::event::Event::Paste(_) => {},
                crossterm::event::Event::Resize(new_term_width, new_term_height) => {
                    let (new_bit_width, new_bit_height) = sextant_size((new_term_width, new_term_height));
                    handler.term_width = new_term_width;
                    handler.term_height = new_term_height;
                    handler.bits.resize(new_bit_width, new_bit_height, false);
                    if on_event(&mut handler, None) {
                        break
                    }
                    handler.render_bits()?;
                },
            }
    }
    Ok(())
}

fn sextant_size(term_size: (u16, u16)) -> (usize, usize) {
    (term_size.0 as usize * 2, term_size.1 as usize * 3)
}

pub struct Handler<T>
{
    stdout: Stdout,
    pub bits: Bits2d,
    term_width: u16,
    term_height: u16,
    pub memory: T
}

impl<T> Handler<T> {
    fn new(mut stdout: Stdout, memory: T) -> Result<Self> {
        stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
        crossterm::terminal::enable_raw_mode()?;
        let (term_width, term_height) = crossterm::terminal::size()?;
        let (sextant_width, sextant_height) = sextant_size((term_width, term_height));
        Ok(Self {
            stdout,
            bits: Bits2d::new(sextant_width, sextant_height),
            term_width,
            term_height,
            memory
        })
    }

    pub fn render_bits(&mut self) -> Result<()> {
        self.stdout.queue(crossterm::terminal::Clear(
            crossterm::terminal::ClearType::All,
        ))?;
        for row in 0..self.term_height {
            self.stdout.queue(crossterm::cursor::MoveTo(0, row))?;
            for column in 0..self.term_width {
                let anchor_bit_x = column as usize * 2;
                let anchor_bit_y = row as usize * 3;
                let top_left = self.get_bit(anchor_bit_x, anchor_bit_y).unwrap();
                let top_right = self.get_bit(anchor_bit_x+1, anchor_bit_y).unwrap();
                let middle_left = self.get_bit(anchor_bit_x, anchor_bit_y+1).unwrap();
                let middle_right = self.get_bit(anchor_bit_x+1, anchor_bit_y+1).unwrap();
                let bottom_left = self.get_bit(anchor_bit_x, anchor_bit_y+2).unwrap();
                let bottom_right = self.get_bit(anchor_bit_x+1, anchor_bit_y+2).unwrap();
                let sextant = sextant_from_bits(top_left, top_right, middle_left, middle_right, bottom_left, bottom_right);
                write!(self.stdout, "{sextant}")?;
                //write!(self.stdout, "{c}")?;
            }
        }

        self.stdout.flush()
    }
    pub fn get_bit(&self, x: usize, y: usize) -> Option<bool> {
        self.bits.get(x, y)
    }
    pub fn set_bit(&mut self, x: usize, y: usize, b: bool) {
        self.bits.set(x, y, b);
    }
    pub fn set_bits_all_zero(&mut self) {
        self.bits.set_all_zero();
    }
    pub fn set_bits_all_one(&mut self) {
        self.bits.set_all_one();
    }
    pub fn bit_width(&self) -> usize {
        self.bits.width()
    }
    pub fn bit_height(&self) -> usize {
        self.bits.height()
    }
    pub fn bit_area(&self) -> usize {
        self.bits.area()
    }
    pub fn set_title(&mut self, title: impl std::fmt::Display) -> Result<()> {
        self.stdout.execute(crossterm::terminal::SetTitle(title)).map(|_|())
    }
}

impl<T> Drop for Handler<T> {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = self.stdout
            .execute(crossterm::terminal::LeaveAlternateScreen);
    }
}

fn sextant_from_bits(
    top_left: bool,
    top_right: bool,
    middle_left: bool,
    middle_right: bool,
    bottom_left: bool,
    bottom_right: bool,
) -> char {
    let mut amount_to_add = (top_left as u32)
        | ((top_right as u32) << 1)
        | ((middle_left as u32) << 2)
        | ((middle_right as u32) << 3)
        | ((bottom_left as u32) << 4)
        | ((bottom_right as u32) << 5);
    match amount_to_add {
        0 => return ' ',
        1..0b010101 => {}
        0b010101 => {
            // Right half
            return '▐';
        }
        0b010110..0b101010 => amount_to_add -= 1,
        0b101010 => {
            // Left half
            return '▌';
        }
        0b101011..0b111111 => amount_to_add -= 2,
        0b111111 => {
            // Full block
            return '█';
        }
        _ => unreachable!(),
    }
    let amount = 0x1FB00 - 1 + amount_to_add;
    char::from_u32(amount).unwrap()
}

#[cfg(test)]
mod tests {
    use super::sextant_from_bits;
    use paste::paste;
    macro_rules! check {
        ($sextant:literal BLOCK SEXTANT-$nums:literal) => {
            paste! {
                #[test]
                fn [<test_ $nums>]() {
                    let s = stringify!($nums);
                    let top_left = s.contains('1');
                    let top_right = s.contains('2');
                    let middle_left = s.contains('3');
                    let middle_right = s.contains('4');
                    let bottom_left = s.contains('5');
                    let bottom_right = s.contains('6');
                    assert_eq!(sextant_from_bits(top_left, top_right, middle_left, middle_right, bottom_left, bottom_right), $sextant);
                }
            }
        };
    }
    check!('🬀' BLOCK SEXTANT-1);
    check!('🬁' BLOCK SEXTANT-2);
    check!('🬂' BLOCK SEXTANT-12);
    check!('🬃' BLOCK SEXTANT-3);
    check!('🬄' BLOCK SEXTANT-13);
    check!('🬅' BLOCK SEXTANT-23);
    check!('🬆' BLOCK SEXTANT-123);
    check!('🬇' BLOCK SEXTANT-4);
    check!('🬈' BLOCK SEXTANT-14);
    check!('🬉' BLOCK SEXTANT-24);
    check!('🬊' BLOCK SEXTANT-124);
    check!('🬋' BLOCK SEXTANT-34);
    check!('🬌' BLOCK SEXTANT-134);
    check!('🬍' BLOCK SEXTANT-234);
    check!('🬎' BLOCK SEXTANT-1234);
    check!('🬏' BLOCK SEXTANT-5);
    check!('🬐' BLOCK SEXTANT-15);
    check!('🬑' BLOCK SEXTANT-25);
    check!('🬒' BLOCK SEXTANT-125);
    check!('🬓' BLOCK SEXTANT-35);
    check!('🬔' BLOCK SEXTANT-235);
    check!('🬕' BLOCK SEXTANT-1235);
    check!('🬖' BLOCK SEXTANT-45);
    check!('🬗' BLOCK SEXTANT-145);
    check!('🬘' BLOCK SEXTANT-245);
    check!('🬙' BLOCK SEXTANT-1245);
    check!('🬚' BLOCK SEXTANT-345);
    check!('🬛' BLOCK SEXTANT-1345);
    check!('🬜' BLOCK SEXTANT-2345);
    check!('🬝' BLOCK SEXTANT-12345);
    check!('🬞' BLOCK SEXTANT-6);
    check!('🬟' BLOCK SEXTANT-16);
    check!('🬠' BLOCK SEXTANT-26);
    check!('🬡' BLOCK SEXTANT-126);
    check!('🬢' BLOCK SEXTANT-36);
    check!('🬣' BLOCK SEXTANT-136);
    check!('🬤' BLOCK SEXTANT-236);
    check!('🬥' BLOCK SEXTANT-1236);
    check!('🬦' BLOCK SEXTANT-46);
    check!('🬧' BLOCK SEXTANT-146);
    check!('🬨' BLOCK SEXTANT-1246);
    check!('🬩' BLOCK SEXTANT-346);
    check!('🬪' BLOCK SEXTANT-1346);
    check!('🬫' BLOCK SEXTANT-2346);
    check!('🬬' BLOCK SEXTANT-12346);
    check!('🬭' BLOCK SEXTANT-56);
    check!('🬮' BLOCK SEXTANT-156);
    check!('🬯' BLOCK SEXTANT-256);
    check!('🬰' BLOCK SEXTANT-1256);
    check!('🬱' BLOCK SEXTANT-356);
    check!('🬲' BLOCK SEXTANT-1356);
    check!('🬳' BLOCK SEXTANT-2356);
    check!('🬴' BLOCK SEXTANT-12356);
    check!('🬵' BLOCK SEXTANT-456);
    check!('🬶' BLOCK SEXTANT-1456);
    check!('🬷' BLOCK SEXTANT-2456);
    check!('🬸' BLOCK SEXTANT-12456);
    check!('🬹' BLOCK SEXTANT-3456);
    check!('🬺' BLOCK SEXTANT-13456);
    check!('🬻' BLOCK SEXTANT-23456);
}
