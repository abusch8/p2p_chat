use std::{error::Error, fs::OpenOptions, io::{self, BufRead, Write}, path::Path, time::Duration};
use libp2p::{futures::StreamExt, gossipsub, noise, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux, Multiaddr, SwarmBuilder};
use tracing_subscriber::EnvFilter;
use tokio::select;
use futures::FutureExt;
use chrono::Utc;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

use crate::config;
use crate::display::*;

#[derive(NetworkBehaviour)]
struct MessageBehaviour {
    gossipsub: gossipsub::Behaviour,
}

fn read_log(path: &str, log: &mut Vec<Vec<u8>>) -> Result<(), io::Error> {
    if Path::new(path).exists() {
        let file = OpenOptions::new().read(true).open(path)?;
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
    let file = OpenOptions::new().append(true).create(true).open(path)?;
    let mut writer = io::BufWriter::new(file);
    let mut buf = data.clone();
    buf.extend(b"\n");
    writer.write_all(&buf)?;
    Ok(())
}

pub async fn chat() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let username = &*config::USERNAME;
    let hex = &*config::HEX;

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

    let mut topic_name = String::from("default");
    for arg in std::env::args() {
        match arg.split_once('=') {
            Some(("--address", addr)) => {
                let remote: Multiaddr = addr.parse()?;
                swarm.dial(remote)?;
            },
            Some(("--topic", topic)) => {
                topic_name = topic.to_string();
            },
            _ => {},
        }
    }
    let path = &format!("{}.log", topic_name);

    let mut log = Vec::<Vec::<u8>>::new();
    read_log(path, &mut log)?;

    let mut display = Display::new()?;

    let topic = gossipsub::IdentTopic::new(topic_name);
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let mut reader = EventStream::new();

    let mut msg = String::new();
    let mut cursor_pos = 0;
    let mut scroll_pos = 0;

    display.draw(&msg, &log, cursor_pos, scroll_pos)?;

    Ok(loop {
        select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    let mut data = Vec::new();
                    data.extend_from_slice(&[1u8]);
                    data.extend_from_slice(&Utc::now().timestamp().to_be_bytes());
                    data.extend_from_slice(&format!("Listening on {address:?}").as_bytes());
                    log.push(data);
                    display.draw(&msg, &log, cursor_pos, scroll_pos)?;
                },
                SwarmEvent::Behaviour(MessageBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                    let data = &message.data;
                    log.push(data.clone());
                    display.draw(&msg, &log, cursor_pos, scroll_pos)?;
                },
                _ => {},
            },
            Some(Ok(event)) = reader.next().fuse() => match event {
                Event::Key(KeyEvent { code, modifiers, .. }) => match (code, modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) |
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        display.reset()?;
                        break;
                    },
                    (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                        msg.clear();
                        cursor_pos = 0;
                        display.draw_msg(&msg, cursor_pos)?;
                    },
                    (KeyCode::Char(c), _) => {
                        msg.insert(cursor_pos.into(), c);
                        cursor_pos += 1;
                        display.draw_msg(&msg, cursor_pos)?;
                    },
                    (KeyCode::Enter, _) => {
                        let mut padded_username = username.as_bytes().to_vec();
                        padded_username.resize(64, 0);
                        let mut data = Vec::new();
                        data.extend_from_slice(&[0u8]);
                        data.extend_from_slice(&Utc::now().timestamp().to_be_bytes());
                        data.extend_from_slice(hex.as_bytes());
                        data.extend_from_slice(&padded_username);
                        data.extend_from_slice(msg.as_bytes());
                        scroll_pos = 0;
                        cursor_pos = 0;
                        msg.clear();
                        log.push(data.clone());
                        write_log(path, &data)?;
                        display.draw(&msg, &log, cursor_pos, scroll_pos)?;
                        let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), data);
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
                        display.draw_msg(&msg, cursor_pos)?;
                    },
                    (KeyCode::Backspace, _) => {
                        if cursor_pos > 0 {
                            if cursor_pos < msg.len() as u16 {
                                msg.remove(cursor_pos as usize - 1);
                            } else {
                                msg.pop();
                            }
                            cursor_pos -= 1;
                            display.draw_msg(&msg, cursor_pos)?;
                        }
                    },
                    (KeyCode::Right, _) => {
                        if cursor_pos < msg.len() as u16 {
                            cursor_pos += 1;
                        }
                        display.draw_msg(&msg, cursor_pos)?;
                    },
                    (KeyCode::Left, _) => {
                        if cursor_pos > 0 {
                            cursor_pos -= 1;
                        }
                        display.draw_msg(&msg, cursor_pos)?;
                    },
                    _ => {},
                },
                Event::Mouse(MouseEvent { kind, .. }) => match kind {
                    MouseEventKind::ScrollUp => {
                        if log.len() as u16 > display.terminal_size.1 - 1 && scroll_pos < log.len() as u16 - display.terminal_size.1 + 1 {
                            scroll_pos += 1;
                        }
                        display.draw(&msg, &log, cursor_pos, scroll_pos)?;
                    },
                    MouseEventKind::ScrollDown => {
                        if scroll_pos > 0 {
                            scroll_pos -= 1;
                        }
                        display.draw(&msg, &log, cursor_pos, scroll_pos)?;
                    },
                    _ => {},
                },
                _ => {},
            },
        }
    })
}

