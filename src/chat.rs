use std::io::{stdout, Write};
use crossterm::{cursor::MoveTo, event::{EventStream, KeyModifiers}, style::{style, Attribute, Color, PrintStyledContent, Stylize}, terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType}};
use libp2p::{futures::StreamExt, gossipsub, noise, swarm::{NetworkBehaviour, SwarmEvent}, tcp, yamux, Multiaddr};
use std::{error::Error, time::Duration};
use tracing_subscriber::EnvFilter;
use tokio::select;
use futures::FutureExt;
use crossterm::{
    QueueableCommand,
    event::{Event, KeyEvent, KeyCode},
    style::Print,
    terminal,
};
use chrono::Utc;

#[derive(NetworkBehaviour)]
struct MessageBehaviour {
    gossipsub: gossipsub::Behaviour,
}

fn print_user_msg(msg: &str, terminal_size: (u16, u16)) -> Result<(), std::io::Error> {
    let mut stdout = stdout();

    stdout
        .queue(MoveTo(0, terminal_size.1 - 1))?
        .queue(Clear(ClearType::CurrentLine))?
        .queue(Print(" > "))?
        .queue(Print(&msg))?;

    Ok(())
}

fn hex_to_color(hex: &str) -> Color {
    Color::Rgb {
        r: u8::from_str_radix(&hex[0..2], 16).unwrap(),
        g: u8::from_str_radix(&hex[2..4], 16).unwrap(),
        b: u8::from_str_radix(&hex[4..6], 16).unwrap(),
    }
}

fn print_feed_msg(msg: &str, user: &str, hex: &str, scroll: &mut u16) -> Result<(), std::io::Error> {
    let mut stdout = stdout();

    stdout
        .queue(MoveTo(0, *scroll))?
        .queue(PrintStyledContent(style(Utc::now().format("%m/%d/%y %H:%M:%S")).with(Color::DarkGrey)))?
        .queue(Print(" "))?
        .queue(PrintStyledContent(style(user.to_string()).with(hex_to_color(hex)).attribute(Attribute::Bold)))?
        .queue(Print(" "))?
        .queue(Print(&msg))?;

    *scroll += 1;

    Ok(())
}

pub async fn chat() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let username = "Alex";
    let hex = "5effa5";

    enable_raw_mode()?;

    let mut stdout = stdout();
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

    Ok(loop {
        select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {address:?}");
                },
                SwarmEvent::Behaviour(MessageBehaviourEvent::Gossipsub(gossipsub::Event::Message { message, .. })) => {
                    let hex = String::from_utf8_lossy(&message.data[0..6]);
                    let username = String::from_utf8_lossy(&message.data[6..70]);
                    let msg = String::from_utf8_lossy(&message.data[70..]);
                    print_feed_msg(&msg, &username, &hex, &mut scroll)?;
                },
                _ => {},
            },
            Some(Ok(event)) = reader.next().fuse() => match event {
                Event::Key(KeyEvent { code, modifiers, .. }) => match (code, modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) |
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        disable_raw_mode()?;
                        break;
                    },
                    (KeyCode::Char(c), _) => {
                        msg += &c.to_string();
                        print_user_msg(&msg, terminal_size)?;
                    },
                    (KeyCode::Enter, _) => {
                        let mut padded_username = username.as_bytes().to_vec();
                        padded_username.resize(64, 0);
                        let mut data = Vec::new();
                        data.extend_from_slice(hex.as_bytes());
                        data.extend_from_slice(&padded_username);
                        data.extend_from_slice(msg.as_bytes());
                        swarm.behaviour_mut().gossipsub.publish(topic.clone(), data)?;
                        print_feed_msg(&msg, username, hex, &mut scroll)?;
                        msg.clear();
                        print_user_msg(&msg, terminal_size)?;
                    },
                    (KeyCode::Backspace, _) => {
                        msg.pop();
                        print_user_msg(&msg, terminal_size)?;
                    },
                    _ => {},
                },
                _ => {},
            },
        }
        stdout.flush().unwrap();
    })
}
