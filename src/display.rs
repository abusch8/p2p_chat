use std::io::{self, Stdout};
use chrono::{DateTime, Utc};
use crossterm::{
    cursor::MoveTo,
    event::EnableMouseCapture,
    style::{style, Attribute, Color, Print, PrintStyledContent, Stylize},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
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

pub fn init_display(stdout: &mut Stdout, terminal_size: (u16, u16)) -> Result<(), io::Error> {
    enable_raw_mode()?;
    stdout
        .queue(EnableMouseCapture)?
        .queue(Clear(ClearType::All))?
        .queue(MoveTo(0, terminal_size.1 - 1))?
        .queue(Print(" > "))?;
    Ok(())
}

pub fn reset_display(stdout: &mut Stdout) -> Result<(), io::Error> {
    stdout.queue(Clear(ClearType::All))?;
    disable_raw_mode()?;
    Ok(())
}

pub fn print_log(stdout: &mut Stdout, log: &Vec::<Vec::<u8>>, scroll_pos: u16, terminal_size: (u16, u16)) -> Result<(), io::Error> {
    stdout
        .queue(Clear(ClearType::All))?;

    let x: usize = 0;
    let y: usize = if log.len() > terminal_size.1 as usize - 1 { terminal_size.1 as usize - 1 } else { log.len() };

    for i in x..y {
        let data = &log[log.len() - (y + scroll_pos as usize) + i];

        let ts_bytes: [u8; 8] = data[0..8].try_into().unwrap();
        let dt = DateTime::from_timestamp(i64::from_be_bytes(ts_bytes), 0).unwrap();
        let hex = String::from_utf8_lossy(&data[8..14]);
        let username = String::from_utf8_lossy(&data[14..78]);
        let msg = String::from_utf8_lossy(&data[78..]);

        stdout
            .queue(MoveTo(0, i as u16))?
            .queue(PrintStyledContent(style(dt.format(DATETIME_FMT)).with(Color::DarkGrey)))?
            .queue(Print(" "))?
            .queue(PrintStyledContent(style(username.to_string()).with(hex_to_color(&hex)).attribute(Attribute::Bold)))?
            .queue(Print(" "))?
            .queue(Print(&msg))?;
    }
    Ok(())
}

pub fn print_sys(stdout: &mut Stdout, msg: &str, scroll: &mut u16, cursor_pos: u16, terminal_size: (u16, u16)) -> Result<(), io::Error> {
    stdout
        .queue(MoveTo(0, *scroll))?
        .queue(PrintStyledContent(style(format!("{} {}", Utc::now().format(DATETIME_FMT), msg).with(Color::DarkGrey))))?
        .queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
    *scroll += 1;
    Ok(())
}

pub fn print_msg(stdout: &mut Stdout, msg: &str, cursor_pos: u16, terminal_size: (u16, u16)) -> Result<(), io::Error> {
    stdout
        .queue(MoveTo(0, terminal_size.1 - 1))?
        .queue(Clear(ClearType::CurrentLine))?
        .queue(Print(" > "))?
        .queue(Print(msg))?
        .queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
    Ok(())
}

