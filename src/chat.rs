use std::{error::Error, fs::File, io::{self, stdout, BufRead, Stdout, Write}, path::Path, time::Duration};
use libp2p::{futures::StreamExt, gossipsub, noise, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux, Multiaddr, SwarmBuilder};
use tracing_subscriber::EnvFilter;
use tokio::select;
use futures::FutureExt;
use chrono::{DateTime, Utc};
use crossterm::{
    cursor::MoveTo, event::{EnableMouseCapture, Event, EventStream, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind},
    style::{style, Attribute, Color, Print, PrintStyledContent, Stylize},
    terminal::{self, disable_raw_mode, enable_raw_mode, Clear, ClearType},
    QueueableCommand,
};

use crate::config;

const DATETIME_FMT: &str = "%m/%d/%y %H:%M:%S";

#[derive(NetworkBehaviour)]
struct MessageBehaviour {
    gossipsub: gossipsub::Behaviour,
}

fn hex_to_color(hex: &str) -> Color {
    Color::Rgb {
        r: u8::from_str_radix(&hex[0..2], 16).unwrap(),
        g: u8::from_str_radix(&hex[2..4], 16).unwrap(),
        b: u8::from_str_radix(&hex[4..6], 16).unwrap(),
    }
}

fn print_log(stdout: &mut Stdout, log: &Vec::<Vec::<u8>>, scroll_pos: u16, terminal_size: (u16, u16)) -> Result<(), io::Error> {
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

fn read_log(path: &str, log: &mut Vec<Vec<u8>>) -> Result<(), io::Error> {
    if Path::new(path).exists() {
        let file = File::open(path)?;
        let mut reader = io::BufReader::new(file);
        let mut buf = Vec::new();
        while reader.read_until(b'\n', &mut buf)? != 0 {
            log.push(buf.clone());
            buf.clear();
        }
    }
    Ok(())
}

fn write_log(path: &str, data: &Vec<u8>) -> Result<(), io::Error> {
    let file = File::create(path)?;
    let mut writer = io::BufWriter::new(file);
    let mut buf = data.clone();
    buf.extend(b"\n");
    writer.write_all(&buf)?;
    Ok(())
}

fn print_sys(stdout: &mut Stdout, msg: &str, scroll: &mut u16, cursor_pos: u16, terminal_size: (u16, u16)) -> Result<(), io::Error> {
    stdout
        .queue(MoveTo(0, *scroll))?
        .queue(PrintStyledContent(style(format!("{} {}", Utc::now().format(DATETIME_FMT), msg).with(Color::DarkGrey))))?
        .queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
    *scroll += 1;
    Ok(())
}

fn print_msg(stdout: &mut Stdout, msg: &str, cursor_pos: u16, terminal_size: (u16, u16)) -> Result<(), io::Error> {
    stdout
        .queue(MoveTo(0, terminal_size.1 - 1))?
        .queue(Clear(ClearType::CurrentLine))?
        .queue(Print(" > "))?
        .queue(Print(msg))?
        .queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
    Ok(())
}

pub async fn chat() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let username = &*config::USERNAME;
    let hex = &*config::HEX;

    enable_raw_mode()?;

    let topic_name = "test-net";
    let path = &format!("{}.log", topic_name);

    let mut log = Vec::<Vec::<u8>>::new();
    read_log(path, &mut log)?;

    let stdout = &mut stdout();
    let terminal_size = terminal::size().unwrap();

    stdout
        .queue(EnableMouseCapture)?
        .queue(Clear(ClearType::All))?
        .queue(MoveTo(0, terminal_size.1 - 1))?
        .queue(Print(" > "))?;

    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| {
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub::Config::default(),
            )?;
            Ok(MessageBehaviour { gossipsub })
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX)))
        .build();

    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    if let Some(addr) = std::env::args().nth(1) {
        let remote: Multiaddr = addr.parse()?;
        swarm.dial(remote)?;
    }

    let topic = gossipsub::IdentTopic::new(topic_name);
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let mut reader = EventStream::new();

    let mut msg = String::new();
    let mut scroll = 0;
    let mut cursor_pos = 0;
    let mut scroll_pos = 0;

    print_log(stdout, &log, scroll_pos, terminal_size)?;

    Ok(loop {
        select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    print_sys(stdout, &format!("Listening on {address:?}"), &mut scroll, cursor_pos, terminal_size)?;
                },
                SwarmEvent::Behaviour(MessageBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                    let data = &message.data;
                    log.push(data.clone());
                    write_log(path, data)?;
                    print_log(stdout, &log, scroll_pos, terminal_size)?;
                },
                _ => {},
            },
            Some(Ok(event)) = reader.next().fuse() => match event {
                Event::Key(KeyEvent { code, modifiers, .. }) => match (code, modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) |
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        stdout.queue(Clear(ClearType::All))?;
                        disable_raw_mode()?;
                        break;
                    },
                    (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                        msg.clear();
                        cursor_pos = 0;
                        print_msg(stdout, &msg, cursor_pos, terminal_size)?;
                    },
                    (KeyCode::Char(c), _) => {
                        msg.insert(cursor_pos.into(), c);
                        cursor_pos += 1;
                        stdout.queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
                        print_msg(stdout, &msg, cursor_pos, terminal_size)?;
                    },
                    (KeyCode::Enter, _) => {
                        let mut padded_username = username.as_bytes().to_vec();
                        padded_username.resize(64, 0);
                        let mut data = Vec::new();
                        data.extend_from_slice(&Utc::now().timestamp().to_be_bytes());
                        data.extend_from_slice(hex.as_bytes());
                        data.extend_from_slice(&padded_username);
                        data.extend_from_slice(msg.as_bytes());
                        log.push(data.clone());
                        write_log(topic_name, &data)?;
                        print_log(stdout, &log, scroll_pos, terminal_size)?;
                        let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), data);
                        scroll_pos = 0;
                        cursor_pos = 0;
                        msg.clear();
                        print_msg(stdout, &msg, cursor_pos, terminal_size)?;
                    },
                    (KeyCode::Backspace, KeyModifiers::ALT) => {
                        let mut a = msg[..cursor_pos as usize].trim_end().to_string();
                        let b = &msg[cursor_pos as usize..];
                        cursor_pos -= cursor_pos - a.len() as u16;
                        while cursor_pos > 0 && !a.chars().nth(cursor_pos as usize - 1).unwrap().is_whitespace() {
                            cursor_pos -= 1;
                            a.remove(cursor_pos.into());
                        }
                        msg = a + b;
                        print_msg(stdout, &msg, cursor_pos, terminal_size)?;
                    },
                    (KeyCode::Backspace, _) => {
                        if cursor_pos > 0 {
                            if cursor_pos < msg.len() as u16 {
                                msg.remove(cursor_pos as usize - 1);
                            } else {
                                msg.pop();
                            }
                            cursor_pos -= 1;
                            print_msg(stdout, &msg, cursor_pos, terminal_size)?;
                        }
                    },
                    (KeyCode::Right, _) => {
                        if cursor_pos < msg.len() as u16 {
                            cursor_pos += 1;
                        }
                        stdout.queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
                    },
                    (KeyCode::Left, _) => {
                        if cursor_pos > 0 {
                            cursor_pos -= 1;
                        }
                        stdout.queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
                    },
                    _ => {},
                },
                Event::Mouse(MouseEvent { kind, .. }) => match kind {
                    MouseEventKind::ScrollUp => {
                        if log.len() as u16 > terminal_size.1 - 1 && scroll_pos < log.len() as u16 - terminal_size.1 + 1 {
                            scroll_pos += 1;
                        }
                        print_log(stdout, &log, scroll_pos, terminal_size)?;
                    },
                    MouseEventKind::ScrollDown => {
                        if scroll_pos > 0 {
                            scroll_pos -= 1;
                        }
                        print_log(stdout, &log, scroll_pos, terminal_size)?;
                    },
                    _ => {},
                },
                _ => {},
            },
        }
        stdout.flush().unwrap();
    })
}
