use std::io::{stdout, Result, Stdout, Write};
use chrono::DateTime;
use crossterm::{
    cursor::MoveTo,
    event::EnableMouseCapture,
    style::{style, Attribute, Color, Print, PrintStyledContent, Stylize},
    terminal::{self, disable_raw_mode, enable_raw_mode, Clear, ClearType},
    QueueableCommand
};

const DATETIME_FMT: &str = "%m/%d/%y %H:%M:%S";

fn hex_to_color(hex: &str) -> Color {
    Color::Rgb {
        r: u8::from_str_radix(&hex[0..2], 16).unwrap(),
        g: u8::from_str_radix(&hex[2..4], 16).unwrap(),
        b: u8::from_str_radix(&hex[4..6], 16).unwrap(),
    }
}

pub struct Display {
    pub stdout: Stdout,
    pub terminal_size: (u16, u16),
}

impl Display {
    pub fn new() -> Result<Self> {
        let mut stdout = stdout();
        let terminal_size = terminal::size()?;

        enable_raw_mode()?;
        stdout
            .queue(EnableMouseCapture)?
            .queue(Clear(ClearType::All))?
            .queue(MoveTo(0, terminal_size.1 - 1))?
            .queue(Print(" > "))?;

        Ok(Display { stdout, terminal_size })
    }

    pub fn draw(&mut self, msg: &str, log: &Vec::<Vec::<u8>>, cursor_pos: u16, scroll_pos: u16) -> Result<()> {
        self.draw_log(log, scroll_pos)?;
        self.draw_msg(msg, cursor_pos)?;
        Ok(())
    }

    pub fn draw_msg(&mut self, msg: &str, cursor_pos: u16) -> Result<()> {
        self.stdout
            .queue(MoveTo(0, self.terminal_size.1 - 1))?
            .queue(Clear(ClearType::CurrentLine))?
            .queue(Print(" > "))?
            .queue(Print(msg))?
            .queue(MoveTo(cursor_pos + 3, self.terminal_size.1 - 1))?
            .flush()?;
        Ok(())
    }

    pub fn draw_log(&mut self, log: &Vec::<Vec::<u8>>, scroll_pos: u16) -> Result<()> {
        self.stdout
            .queue(Clear(ClearType::All))?;

        let x: usize = 0;
        let y: usize = if log.len() > self.terminal_size.1 as usize - 1 { self.terminal_size.1 as usize - 1 } else { log.len() };

        for i in x..y {
            let data = &log[log.len() - (y + scroll_pos as usize) + i];

            let is_sys: bool = data[0] == 1;

            let ts_bytes: [u8; 8] = data[1..9].try_into().unwrap();
            let dt = DateTime::from_timestamp(i64::from_be_bytes(ts_bytes), 0).unwrap();

            if is_sys {
                let msg = String::from_utf8_lossy(&data[9..]);
                self.stdout
                    .queue(MoveTo(0, i as u16))?
                    .queue(PrintStyledContent(style(format!("{} {}", dt.format(DATETIME_FMT), &msg)).with(Color::DarkGrey)))?
                    .flush()?;
            } else {
                let hex = String::from_utf8_lossy(&data[9..15]);
                let username = String::from_utf8_lossy(&data[15..79]);
                let msg = String::from_utf8_lossy(&data[79..]);
                self.stdout
                    .queue(MoveTo(0, i as u16))?
                    .queue(PrintStyledContent(style(dt.format(DATETIME_FMT)).with(Color::DarkGrey)))?
                    .queue(Print(" "))?
                    .queue(PrintStyledContent(style(username.to_string()).with(hex_to_color(&hex)).attribute(Attribute::Bold)))?
                    .queue(Print(" "))?
                    .queue(Print(&msg))?
                    .flush()?;
            }
        }
        Ok(())
    }

    pub fn reset(&mut self) -> Result<()> {
        self.stdout
            .queue(Clear(ClearType::All))?;
        disable_raw_mode()?;
        Ok(())
    }
}

