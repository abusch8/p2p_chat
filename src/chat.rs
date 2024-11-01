use std::io::{stdout, Write};
use crossterm::{execute, terminal::{enable_raw_mode, Clear, ClearType, WindowSize}};
use libp2p::{gossipsub, noise, swarm::{NetworkBehaviour, SwarmEvent}, futures::StreamExt, tcp, yamux, Multiaddr};
use std::{error::Error, time::Duration};
use tracing_subscriber::EnvFilter;
use tokio::{io::{self, AsyncBufReadExt, AsyncWriteExt}, select};

#[derive(NetworkBehaviour)]
struct MessageBehaviour {
    gossipsub: gossipsub::Behaviour,
}

pub async fn chat() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    enable_raw_mode()?;

    let mut stdout = stdout();

    execute!(stdout, MoveTo(), Clear(ClearType::All))?;


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
        println!("Dialed {addr}")
    }

    let topic = gossipsub::IdentTopic::new("test-net");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();

    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {
                println!("Sending: {}", line);
                let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), line.as_bytes());
                print!(" > ");
                stdout().flush().unwrap();
            },
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {address:?}");
                },
                SwarmEvent::Behaviour(MessageBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    message,
                    ..
                })) => {
                    println!("Received: {}", String::from_utf8_lossy(&message.data));
                    print!(" > ");
                    stdout().flush().unwrap();
                },
                _ => {},
            },
        }
    }
}
