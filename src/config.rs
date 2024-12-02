use ini::Ini;
use home::home_dir;
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref CONFIG_PATH: String = format!("{}/.config/chat.ini", home_dir().unwrap().to_str().unwrap());
    static ref CONFIG: Ini = Ini::load_from_file(&*CONFIG_PATH).unwrap_or(Ini::new());

    pub static ref USERNAME: String = CONFIG
        .get_from_or(Some("user"), "name", "User")
        .parse()
        .unwrap_or_else(|_| panic!("Invalid username config value"));

    pub static ref HEX: String = {
        let hex = CONFIG
            .get_from_or(Some("user"), "color", "#000000")
            .parse::<String>()
            .unwrap_or_else(|_| panic!("Invalid user hex color config value"));

        if Regex::new(r"#(\d[a-fA-F]){6}$").unwrap().is_match(&hex) {
            panic!("Invalid user hex color config value");
        }

        hex.strip_prefix('#').unwrap().to_string()
    };
}


