use std::error::Error;
use tlc::cal;

fn main() -> Result<(), Box<dyn Error>> {
    cal("./config/config.json")?;
    Ok(())
}
