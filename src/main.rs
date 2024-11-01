use std::error::Error;

mod chat;

use crate::chat::chat;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    Ok(chat().await?)
}
