use std::{error::Error, io::{self, stdout, Stdout, Write}, time::Duration};
use libp2p::{futures::StreamExt, gossipsub, noise, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux, Multiaddr};
use tracing_subscriber::EnvFilter;
use tokio::select;
use futures::FutureExt;
use chrono::Utc;
use crossterm::{
    QueueableCommand,
    cursor::MoveTo,
    event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind},
    style::{style, Attribute, Color, Print, PrintStyledContent, Stylize},
    terminal::{self, disable_raw_mode, enable_raw_mode, Clear, ClearType},
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

fn print_feed_msg(stdout: &mut Stdout, msg: &str, user: &str, hex: &str, scroll: &mut u16, terminal_size: (u16, u16), cursor_pos: u16) -> Result<(), io::Error> {
    stdout
        .queue(MoveTo(0, *scroll))?
        .queue(PrintStyledContent(style(Utc::now().format(DATETIME_FMT)).with(Color::DarkGrey)))?
        .queue(Print(" "))?
        .queue(PrintStyledContent(style(user.to_string()).with(hex_to_color(hex)).attribute(Attribute::Bold)))?
        .queue(Print(" "))?
        .queue(Print(&msg))?
        .queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;

    *scroll += 1;

    Ok(())
}

fn print_sys_msg(stdout: &mut Stdout, msg: &str, scroll: &mut u16, terminal_size: (u16, u16), cursor_pos: u16) -> Result<(), io::Error> {
    stdout
        .queue(MoveTo(0, *scroll))?
        .queue(PrintStyledContent(style(format!("{} {}", Utc::now().format(DATETIME_FMT), msg).with(Color::DarkGrey))))?
        .queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;

    *scroll += 1;

    Ok(())
}

fn print_user_msg(stdout: &mut Stdout, msg: &str, terminal_size: (u16, u16), cursor_pos: u16) -> Result<(), io::Error> {
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

    let stdout = &mut stdout();
    let terminal_size = terminal::size().unwrap();

    stdout
        .queue(Clear(ClearType::All))?
        .queue(MoveTo(0, terminal_size.1 - 1))?
        .queue(Print(" > "))?;

    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
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

    let topic = gossipsub::IdentTopic::new("test-net");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let mut reader = EventStream::new();

    let mut msg = String::new();
    let mut scroll = 0;
    let mut cursor_pos = 0;

    Ok(loop {
        select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    print_sys_msg(stdout, &format!("Listening on {address:?}"), &mut scroll, terminal_size, cursor_pos)?;
                },
                SwarmEvent::Behaviour(MessageBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                    let hex = String::from_utf8_lossy(&message.data[0..6]);
                    let username = String::from_utf8_lossy(&message.data[6..70]);
                    let msg = String::from_utf8_lossy(&message.data[70..]);
                    print_feed_msg(stdout, &msg, &username, &hex, &mut scroll, terminal_size, cursor_pos)?;
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
                    (KeyCode::Char(c), _) => {
                        msg.insert(cursor_pos.into(), c);
                        cursor_pos += 1;
                        stdout.queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
                        print_user_msg(stdout, &msg, terminal_size, cursor_pos)?;
                    },
                    (KeyCode::Enter, _) => {
                        let mut padded_username = username.as_bytes().to_vec();
                        padded_username.resize(64, 0);
                        let mut data = Vec::new();
                        data.extend_from_slice(hex.as_bytes());
                        data.extend_from_slice(&padded_username);
                        data.extend_from_slice(msg.as_bytes());
                        let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), data);
                        print_feed_msg(stdout, &msg, &username, &hex, &mut scroll, terminal_size, cursor_pos)?;
                        msg.clear();
                        cursor_pos = 0;
                        print_user_msg(stdout, &msg, terminal_size, cursor_pos)?;
                    },
                    (KeyCode::Backspace, KeyModifiers::ALT) => {
                        let mut a = msg[..cursor_pos as usize].trim_end().to_string();
                        let b = &msg[cursor_pos as usize..];
                        cursor_pos -= cursor_pos - a.len() as u16;
                        while cursor_pos > 0 && !a.chars().nth(cursor_pos as usize - 1).unwrap().is_whitespace() {
                            cursor_pos -= 1;
                            a.remove(cursor_pos as usize);
                        }
                        msg = a + b;
                        print_user_msg(stdout, &msg, terminal_size, cursor_pos)?;
                    },
                    (KeyCode::Backspace, _) => {
                        if cursor_pos > 0 {
                            if cursor_pos < msg.len() as u16 {
                                msg.remove(cursor_pos as usize - 1);
                            } else {
                                msg.pop();
                            }
                            cursor_pos -= 1;
                            print_user_msg(stdout, &msg, terminal_size, cursor_pos)?;
                        }
                    },
                    (KeyCode::Right, _) => {
                        if cursor_pos < msg.len() as u16 { cursor_pos += 1 };
                        stdout.queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
                    },
                    (KeyCode::Left, _) => {
                        if cursor_pos > 0 { cursor_pos -= 1 };
                        stdout.queue(MoveTo(cursor_pos + 3, terminal_size.1 - 1))?;
                    },
                    _ => {},
                },
                // Event::Mouse(MouseEvent { kind, .. }) => match kind {
                //     MouseEventKind::ScrollUp => {
                //
                //     },
                //     MouseEventKind::ScrollDown => {
                //
                //     },
                //     _ => {},
                // },
                _ => {},
            },
        }
        stdout.flush().unwrap();
    })
}
